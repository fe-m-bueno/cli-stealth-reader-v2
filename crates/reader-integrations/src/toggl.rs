//! The Toggl 2.0 Focus client.
//!
//! Focus is quota-limited — 30 requests an hour on the free plan — so the client
//! is deliberately frugal: it caches projects and recent descriptions, reads the
//! quota headers off every response, and never polls in the background faster
//! than [`REFRESH_INTERVAL_MS`].
//!
//! State lives in the settings table through a [`SettingsStore`], which keeps
//! this crate independent of the database.

use reader_core::compare_text;

use crate::http::{HttpRequest, HttpResponse, Transport};
use crate::parse::{ParseError, TOKEN_PAGE, extract_scope, format_elapsed, format_seconds};
use crate::types::{TogglCache, TogglProject, TogglQuota, TogglRecentDescription, TogglTimeEntry};

const API_BASE: &str = "https://focus.toggl.com/api";
/// Focus caps a page at 100 rows.
const MAX_PAGE_SIZE: usize = 100;
/// How many recent descriptions are worth remembering.
const MAX_DESCRIPTIONS: usize = 25;
/// Background refresh interval: 12 requests an hour, leaving the free plan's 30
/// with headroom for commands the reader actually types.
pub const REFRESH_INTERVAL_MS: i64 = 300_000;

/// Settings keys this integration owns.
pub const TOKEN_KEY: &str = "togglApiToken";
pub const CACHE_KEY: &str = "togglCache";
pub const QUOTA_KEY: &str = "togglQuota";
pub const CURRENT_ENTRY_KEY: &str = "togglCurrentEntry";

/// Where the integration keeps its state between sessions.
pub trait SettingsStore {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&self, key: &str, value: &str);
}

/// Anything that can go wrong talking to Toggl.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TogglError {
    /// No API key has been stored yet.
    NotConnected,
    /// Setup never completed, so there is no organization to work in.
    SetupIncomplete,
    /// The account has no workspace the reader can use.
    NoWorkspace,
    /// The API answered with an error status.
    Api { status: u16, message: String },
    /// The request could not be made at all.
    Transport(String),
    /// The API answered with something unreadable.
    Malformed(String),
    /// A pasted value could not be used.
    Parse(ParseError),
    /// A project was named that the cache does not know.
    UnknownProject(String),
}

impl std::fmt::Display for TogglError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConnected => write!(
                formatter,
                "Toggl is not connected. Run /toggl auth <toggl_sk_...>. Key: {TOKEN_PAGE}"
            ),
            Self::SetupIncomplete => write!(
                formatter,
                "Toggl setup is incomplete. Run /toggl auth again and paste the workspace URL when prompted."
            ),
            Self::NoWorkspace => write!(
                formatter,
                "No Toggl workspace found. Reconnect with /toggl auth <toggl_sk_...>."
            ),
            Self::Api { message, .. } => write!(formatter, "{message}"),
            Self::Transport(detail) => write!(formatter, "Could not reach Toggl: {detail}"),
            Self::Malformed(detail) => {
                write!(formatter, "Toggl sent an unreadable response: {detail}")
            }
            Self::Parse(error) => write!(formatter, "{error}"),
            Self::UnknownProject(name) => write!(
                formatter,
                "Toggl project \"{name}\" was not found. Run /toggl sync and try again."
            ),
        }
    }
}

impl std::error::Error for TogglError {}

impl From<ParseError> for TogglError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

type Result<T> = std::result::Result<T, TogglError>;

/// The Toggl integration.
pub struct TogglClient<'a, S: SettingsStore, T: Transport> {
    settings: &'a S,
    transport: &'a T,
    /// Current time in epoch milliseconds, supplied so quota countdowns and
    /// entry timestamps are deterministic under test.
    now: i64,
}

impl<'a, S: SettingsStore, T: Transport> TogglClient<'a, S, T> {
    pub const fn new(settings: &'a S, transport: &'a T, now: i64) -> Self {
        Self {
            settings,
            transport,
            now,
        }
    }

    // ── state ────────────────────────────────────────────────────────────────

    /// Whether an API key has been stored.
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.token().is_some()
    }

    fn token(&self) -> Option<String> {
        self.settings
            .get(TOKEN_KEY)
            .filter(|token| !token.trim().is_empty())
    }

    /// Everything cached from the last sync.
    #[must_use]
    pub fn cache(&self) -> TogglCache {
        self.settings
            .get(CACHE_KEY)
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    fn write_cache(&self, cache: &TogglCache) {
        if let Ok(raw) = serde_json::to_string(cache) {
            self.settings.set(CACHE_KEY, &raw);
        }
    }

    /// The API budget observed on the last response.
    #[must_use]
    pub fn quota(&self) -> Option<TogglQuota> {
        self.settings
            .get(QUOTA_KEY)
            .filter(|raw| !raw.is_empty())
            .and_then(|raw| serde_json::from_str(&raw).ok())
    }

    /// The running timer, if the reader started one.
    #[must_use]
    pub fn current_entry(&self) -> Option<TogglTimeEntry> {
        self.settings
            .get(CURRENT_ENTRY_KEY)
            .filter(|raw| !raw.is_empty())
            .and_then(|raw| serde_json::from_str(&raw).ok())
    }

    fn save_current_entry(&self, entry: Option<&TogglTimeEntry>) {
        let raw = entry
            .and_then(|entry| serde_json::to_string(entry).ok())
            .unwrap_or_default();
        self.settings.set(CURRENT_ENTRY_KEY, &raw);
    }

    // ── requests ─────────────────────────────────────────────────────────────

    fn request(&self, method: &'static str, path: &str, body: Option<String>) -> Result<String> {
        let token = self.token().ok_or(TogglError::NotConnected)?;
        self.request_with_token(method, path, body, &token)
    }

    fn request_with_token(
        &self,
        method: &'static str,
        path: &str,
        body: Option<String>,
        token: &str,
    ) -> Result<String> {
        let response = self
            .transport
            .send(&HttpRequest {
                method,
                url: format!("{API_BASE}{path}"),
                token: token.to_owned(),
                body,
            })
            .map_err(TogglError::Transport)?;

        self.record_quota(&response);
        if response.is_success() {
            return Ok(response.body);
        }
        Err(self.api_error(&response))
    }

    /// Store the quota headers, when the response carried them.
    fn record_quota(&self, response: &HttpResponse) {
        let (Some(remaining), Some(resets_in)) = (
            response.header("X-Toggl-Quota-Remaining"),
            response.header("X-Toggl-Quota-Resets-In"),
        ) else {
            return;
        };
        let (Ok(remaining), Ok(resets_in)) = (remaining.parse::<i64>(), resets_in.parse::<i64>())
        else {
            return;
        };
        let quota = TogglQuota {
            remaining: remaining.max(0),
            resets_in_seconds: resets_in.max(0),
            observed_at: self.now,
        };
        if let Ok(raw) = serde_json::to_string(&quota) {
            self.settings.set(QUOTA_KEY, &raw);
        }
    }

    /// Turn an error status into something the reader can act on.
    fn api_error(&self, response: &HttpResponse) -> TogglError {
        let detail = error_detail(&response.body);
        let message = match response.status {
            401 => format!(
                "Toggl authentication failed (401). Create a Toggl 2.0 key at {TOKEN_PAGE}, then run /toggl auth <toggl_sk_...>."
            ),
            402 => {
                let reset = self.quota().map_or_else(
                    || "the quota window resets".to_owned(),
                    |quota| format_seconds(quota.resets_in_seconds),
                );
                format!("Toggl quota exhausted (402). Try again in {reset}.")
            }
            403 => {
                let suffix = if detail.is_empty() {
                    ".".to_owned()
                } else {
                    format!(": {detail}")
                };
                format!(
                    "Toggl denied this request (403). Run /toggl auth again and check your workspace permissions{suffix}"
                )
            }
            status => format!(
                "Toggl Focus API {status}: {}",
                if detail.is_empty() {
                    "request failed"
                } else {
                    &detail
                }
            ),
        };
        TogglError::Api {
            status: response.status,
            message,
        }
    }

    fn parse_json<D: serde::de::DeserializeOwned>(&self, body: &str) -> Result<D> {
        serde_json::from_str(body).map_err(|error| TogglError::Malformed(error.to_string()))
    }

    /// The organization and workspace to work in.
    fn scope(&self, workspace_override: Option<i64>) -> Result<(i64, i64)> {
        let cache = self.cache();
        let organization_id = cache
            .default_organization_id
            .ok_or(TogglError::SetupIncomplete)?;
        let workspace_id = workspace_override
            .or(cache.default_workspace_id)
            .ok_or(TogglError::NoWorkspace)?;
        Ok((organization_id, workspace_id))
    }

    fn scoped_path(&self, suffix: &str, workspace_override: Option<i64>) -> Result<String> {
        let (organization_id, workspace_id) = self.scope(workspace_override)?;
        Ok(format!(
            "/organizations/{organization_id}/workspaces/{workspace_id}{suffix}"
        ))
    }

    /// Follow Focus paging until the rows run out.
    fn fetch_pages<D: serde::de::DeserializeOwned>(&self, path: &str) -> Result<Vec<D>> {
        let mut items: Vec<D> = Vec::new();
        for page in 1.. {
            let separator = if path.contains('?') { '&' } else { '?' };
            let body = self.request(
                "GET",
                &format!("{path}{separator}page={page}&per_page={MAX_PAGE_SIZE}"),
                None,
            )?;
            let response: Page<D> = self.parse_json(&body)?;
            let received = response.data.len();
            items.extend(response.data);

            let per_page = response
                .per_page
                .filter(|value| *value > 0)
                .unwrap_or(MAX_PAGE_SIZE);
            let complete = response.total.is_some_and(|total| items.len() >= total)
                || received == 0
                || received < per_page;
            if complete {
                break;
            }
        }
        Ok(items)
    }

    // ── commands ─────────────────────────────────────────────────────────────

    /// Store an API key and read back which organization it belongs to.
    ///
    /// Returns the organization id when Focus knows it; `None` means the reader
    /// still has to paste a workspace URL.
    pub fn connect(&self, token: &str) -> Result<Option<i64>> {
        let candidate = token.trim();
        let body = self.request_with_token("GET", "/users/me/settings", None, candidate)?;
        let settings: UserSettings = self.parse_json(&body)?;

        // Switching accounts must not carry the old cache or timer across.
        let same_account = self.token().as_deref() == Some(candidate);
        let mut cache = if same_account {
            self.cache()
        } else {
            TogglCache::default()
        };
        cache.default_organization_id = settings
            .current_organization_id
            .or(settings.organization_id)
            .filter(|id| *id > 0)
            .or(cache.default_organization_id);
        cache.default_workspace_id = settings
            .current_workspace_id
            .filter(|id| *id > 0)
            .or(cache.default_workspace_id);
        if !same_account {
            self.save_current_entry(None);
        }

        self.settings.set(TOKEN_KEY, candidate);
        self.write_cache(&cache);
        Ok(cache.default_organization_id)
    }

    /// Forget the key and everything cached with it.
    pub fn disconnect(&self) {
        self.settings.set(TOKEN_KEY, "");
        self.settings.set(CURRENT_ENTRY_KEY, "");
        self.settings.set(QUOTA_KEY, "");
        self.write_cache(&TogglCache::default());
    }

    /// Finish setup from a pasted workspace URL or organization id.
    ///
    /// A rejected scope is rolled back, so a typo does not leave the reader
    /// pointing at an organization that is not theirs.
    pub fn complete_setup(&self, pasted: &str) -> Result<TogglCache> {
        let scope = extract_scope(pasted)?;
        let previous = self.cache();
        let mut cache = previous.clone();
        cache.default_organization_id = Some(scope.organization_id);
        cache.default_workspace_id = scope.workspace_id.or(previous.default_workspace_id);
        self.write_cache(&cache);

        match self.sync() {
            Ok(cache) => Ok(cache),
            Err(error) => {
                if matches!(
                    &error,
                    TogglError::Api { status, .. } if matches!(status, 400 | 403 | 404)
                ) {
                    self.write_cache(&previous);
                }
                Err(error)
            }
        }
    }

    /// Refresh projects, recent descriptions, and the running timer.
    pub fn sync(&self) -> Result<TogglCache> {
        let (_, workspace_id) = self.scope(None)?;

        let rows: Vec<ProjectRow> = self.fetch_pages(&self.scoped_path("/projects", None)?)?;
        let mut projects: Vec<TogglProject> = rows
            .into_iter()
            .filter(|row| row.active.unwrap_or(true))
            .map(|row| TogglProject {
                id: row.id,
                workspace_id: row.workspace_id.unwrap_or(workspace_id),
                name: row.name,
                client_name: row.client.and_then(|client| client.name),
                color: row.color,
            })
            .collect();
        projects.sort_by(|left, right| compare_text(&left.name, &right.name));

        // Only the most recent month is worth offering as a completion.
        let from = self.now - 30 * 24 * 60 * 60 * 1000;
        let query = format!(
            "?date_from={}&date_to={}&page=1&per_page={MAX_DESCRIPTIONS}&order_by=-start&include_taskless=true",
            iso8601(from),
            iso8601(self.now)
        );
        let body = self.request(
            "GET",
            &format!("{}{query}", self.scoped_path("/time-entries", None)?),
            None,
        )?;
        let entries: Page<TogglTimeEntry> = self.parse_json(&body)?;

        let previous = self.cache();
        let cache = TogglCache {
            default_organization_id: previous.default_organization_id,
            default_workspace_id: Some(workspace_id),
            projects,
            descriptions: unique_descriptions(&entries.data, self.now),
            synced_at: Some(iso8601(self.now)),
        };
        self.write_cache(&cache);
        let _ = self.refresh_current_entry();
        Ok(cache)
    }

    /// Ask Focus what is running now.
    ///
    /// The stored entry is only replaced if nothing else changed it while the
    /// request was in flight, so a timer the reader just started is not clobbered
    /// by a slower background refresh.
    pub fn refresh_current_entry(&self) -> Result<Option<TogglTimeEntry>> {
        let before = self.settings.get(CURRENT_ENTRY_KEY).unwrap_or_default();
        let body = self.request("GET", &self.scoped_path("/tracking/current", None)?, None)?;
        let current: Option<TogglTimeEntry> = if body.trim().is_empty() || body.trim() == "null" {
            None
        } else {
            self.parse_json::<TogglTimeEntry>(&body)
                .ok()
                .filter(|entry| entry.id > 0)
        };

        if self.settings.get(CURRENT_ENTRY_KEY).unwrap_or_default() == before {
            self.save_current_entry(current.as_ref());
        }
        Ok(current)
    }

    /// The project matching `query`: exact name, then client and name, then a
    /// partial name.
    #[must_use]
    pub fn resolve_project(&self, query: &str) -> Option<TogglProject> {
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            return None;
        }
        let cache = self.cache();
        cache
            .projects
            .iter()
            .find(|project| project.name.to_lowercase() == needle)
            .or_else(|| {
                cache
                    .projects
                    .iter()
                    .find(|project| project.label().to_lowercase().contains(&needle))
            })
            .or_else(|| {
                cache
                    .projects
                    .iter()
                    .find(|project| project.name.to_lowercase().contains(&needle))
            })
            .cloned()
    }

    fn required_project(&self, query: Option<&str>) -> Result<Option<TogglProject>> {
        let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) else {
            return Ok(None);
        };
        self.resolve_project(query)
            .map(Some)
            .ok_or_else(|| TogglError::UnknownProject(query.to_owned()))
    }

    /// Start a running timer.
    pub fn start_entry(
        &self,
        description: &str,
        project_query: Option<&str>,
    ) -> Result<TogglTimeEntry> {
        let project = self.required_project(project_query)?;
        let workspace = project.as_ref().map(|project| project.workspace_id);
        let path = self.scoped_path("/tracking/start", workspace)?;
        let payload = serde_json::json!({
            "description": description,
            "project_id": project.as_ref().map(|project| project.id),
            "start": iso8601(self.now),
            "type": "activity",
        });
        let body = self.request("POST", &path, Some(payload.to_string()))?;
        let entry: TogglTimeEntry = self.parse_json(&body)?;
        self.save_current_entry(Some(&entry));
        Ok(entry)
    }

    /// Stop the running timer.
    ///
    /// A 404 means it was already stopped elsewhere, which is not an error worth
    /// showing: the local record is cleared and the reader moves on.
    pub fn stop_entry(&self) -> Result<Option<TogglTimeEntry>> {
        let path = self.scoped_path("/tracking/stop", None)?;
        let payload = serde_json::json!({ "end": iso8601(self.now) });
        match self.request("POST", &path, Some(payload.to_string())) {
            Ok(body) => {
                let entry: TogglTimeEntry = self.parse_json(&body)?;
                self.save_current_entry(None);
                Ok(Some(entry))
            }
            Err(TogglError::Api { status: 404, .. }) => {
                self.save_current_entry(None);
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    /// Record time that has already passed.
    pub fn log_entry(
        &self,
        description: &str,
        duration_seconds: i64,
        project_query: Option<&str>,
    ) -> Result<TogglTimeEntry> {
        let project = self.required_project(project_query)?;
        let workspace = project.as_ref().map(|project| project.workspace_id);
        let path = self.scoped_path("/time-entries", workspace)?;
        let start = self.now - duration_seconds * 1000;
        let payload = serde_json::json!({
            "description": description,
            "project_id": project.as_ref().map(|project| project.id),
            "start": iso8601(start),
            "tracked_at": iso8601(self.now),
            "duration": duration_seconds,
            "type": "activity",
        });
        let body = self.request("POST", &path, Some(payload.to_string()))?;
        self.parse_json(&body)
    }

    // ── display ──────────────────────────────────────────────────────────────

    /// The quota line shown under the command bar, if a quota is known.
    #[must_use]
    pub fn quota_line(&self) -> Option<String> {
        let quota = self.quota()?;
        let elapsed = ((self.now - quota.observed_at).max(0)) / 1000;
        let resets_in = (quota.resets_in_seconds - elapsed).max(0);
        Some(format!(
            "quota {} · resets in {}",
            quota.remaining,
            format_seconds(resets_in)
        ))
    }

    /// The running-timer line for the footer.
    #[must_use]
    pub fn running_timer_line(&self) -> Option<String> {
        let entry = self.current_entry()?;
        if !entry.is_running() {
            return None;
        }
        let started = parse_iso8601_millis(entry.start.as_deref()?)?;
        let elapsed = ((self.now - started).max(0)) / 1000;
        let description = entry
            .description
            .as_deref()
            .map(str::trim)
            .filter(|description| !description.is_empty())
            .map_or_else(|| format!("#{}", entry.id), str::to_owned);
        Some(format!("Toggl {} · {description}", format_elapsed(elapsed)))
    }

    /// The `/toggl recent` report.
    #[must_use]
    pub fn recents_report(&self) -> Vec<String> {
        let cache = self.cache();
        let describe = |value: Option<i64>| {
            value.map_or_else(|| "not configured".to_owned(), |id| id.to_string())
        };
        let mut lines = vec![
            format!(
                "Organization {} · Workspace {}",
                describe(cache.default_organization_id),
                describe(cache.default_workspace_id)
            ),
            format!("Projects ({})", cache.projects.len()),
        ];
        lines.extend(
            cache
                .projects
                .iter()
                .take(10)
                .map(|project| format!("  {}", project.label())),
        );
        lines.push(format!("Descriptions ({})", cache.descriptions.len()));
        lines.extend(
            cache
                .descriptions
                .iter()
                .take(10)
                .map(|item| format!("  {}", item.description)),
        );
        if let Some(quota) = self.quota_line() {
            lines.push(format!("API {quota}"));
        }
        if let Some(synced_at) = cache.synced_at {
            lines.push(format!("Synced {synced_at}"));
        }
        lines
    }

    /// Projects and descriptions in the shape the command bar completes from.
    #[must_use]
    pub fn completions(&self) -> reader_core::command::TogglCompletions {
        let cache = self.cache();
        reader_core::command::TogglCompletions {
            projects: cache
                .projects
                .into_iter()
                .map(|project| reader_core::command::TogglProjectRef {
                    name: project.name,
                    client_name: project.client_name,
                })
                .collect(),
            descriptions: cache
                .descriptions
                .into_iter()
                .map(|item| item.description)
                .collect(),
        }
    }
}

/// One page of a Focus list response.
#[derive(Debug, serde::Deserialize)]
struct Page<D> {
    #[serde(default = "Vec::new")]
    data: Vec<D>,
    #[serde(default)]
    per_page: Option<usize>,
    #[serde(default)]
    total: Option<usize>,
}

#[derive(Debug, serde::Deserialize)]
struct ProjectRow {
    id: i64,
    #[serde(default)]
    workspace_id: Option<i64>,
    name: String,
    #[serde(default)]
    client: Option<ClientRow>,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    active: Option<bool>,
}

#[derive(Debug, serde::Deserialize)]
struct ClientRow {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct UserSettings {
    #[serde(default)]
    current_workspace_id: Option<i64>,
    #[serde(default)]
    current_organization_id: Option<i64>,
    #[serde(default)]
    organization_id: Option<i64>,
}

/// The first useful string in an error body, or a truncated raw body.
fn error_detail(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        for key in ["message", "error", "details"] {
            if let Some(text) = value.get(key).and_then(serde_json::Value::as_str) {
                return text.to_owned();
            }
        }
        return String::new();
    }
    trimmed.chars().take(180).collect()
}

/// Keep the first appearance of each description, newest first.
fn unique_descriptions(entries: &[TogglTimeEntry], now: i64) -> Vec<TogglRecentDescription> {
    let mut seen: Vec<String> = Vec::new();
    let mut result: Vec<TogglRecentDescription> = Vec::new();
    for entry in entries {
        let Some(description) = entry
            .description
            .as_deref()
            .map(str::trim)
            .filter(|description| !description.is_empty())
        else {
            continue;
        };
        let Some(workspace_id) = entry.workspace_id else {
            continue;
        };
        let key = description.to_lowercase();
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        result.push(TogglRecentDescription {
            description: description.to_owned(),
            project_id: entry.project_id,
            workspace_id,
            last_used_at: entry.start.clone().unwrap_or_else(|| iso8601(now)),
        });
        if result.len() >= MAX_DESCRIPTIONS {
            break;
        }
    }
    result
}

/// Format epoch milliseconds as the API expects them.
fn iso8601(millis: i64) -> String {
    let days = millis.div_euclid(86_400_000);
    let time_of_day = millis.rem_euclid(86_400_000);
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    let year = if month <= 2 { year + 1 } else { year };

    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        time_of_day / 3_600_000,
        (time_of_day % 3_600_000) / 60_000,
        (time_of_day % 60_000) / 1_000,
        time_of_day % 1_000
    )
}

/// Parse the timestamps the API returns, which may or may not carry millis.
fn parse_iso8601_millis(value: &str) -> Option<i64> {
    let value = value.trim();
    let (date, rest) = value.split_once('T')?;
    let time = rest.strip_suffix('Z').unwrap_or(rest);

    let mut date_parts = date.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: i64 = date_parts.next()?.parse().ok()?;
    let day: i64 = date_parts.next()?.parse().ok()?;

    let (clock, fraction) = time.split_once('.').unwrap_or((time, "0"));
    let mut clock_parts = clock.split(':');
    let hour: i64 = clock_parts.next()?.parse().ok()?;
    let minute: i64 = clock_parts.next()?.parse().ok()?;
    let second: i64 = clock_parts.next().unwrap_or("0").parse().ok()?;
    let millis: i64 = format!("{fraction:0<3}").get(..3)?.parse().ok()?;

    let year_adjusted = if month <= 2 { year - 1 } else { year };
    let era = year_adjusted.div_euclid(400);
    let year_of_era = year_adjusted - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;

    Some(((days * 24 + hour) * 60 + minute) * 60_000 + second * 1_000 + millis)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    use crate::http::testing::FakeTransport;

    use super::{
        CACHE_KEY, CURRENT_ENTRY_KEY, QUOTA_KEY, SettingsStore, TOKEN_KEY, TogglClient, TogglError,
        iso8601, parse_iso8601_millis, unique_descriptions,
    };
    use crate::types::TogglTimeEntry;

    /// Settings held in memory, so tests touch no database.
    #[derive(Default)]
    struct MemorySettings {
        values: RefCell<BTreeMap<String, String>>,
    }

    impl MemorySettings {
        fn connected() -> Self {
            let settings = Self::default();
            settings.set(TOKEN_KEY, "toggl_sk_test");
            settings.set(
                CACHE_KEY,
                r#"{"defaultOrganizationId":123,"defaultWorkspaceId":456,"projects":[],"descriptions":[]}"#,
            );
            settings
        }
    }

    impl SettingsStore for MemorySettings {
        fn get(&self, key: &str) -> Option<String> {
            self.values.borrow().get(key).cloned()
        }

        fn set(&self, key: &str, value: &str) {
            self.values
                .borrow_mut()
                .insert(key.to_owned(), value.to_owned());
        }
    }

    const NOW: i64 = 1_700_000_000_000;

    #[test]
    fn a_client_without_a_key_refuses_before_reaching_the_network() {
        let settings = MemorySettings::default();
        let transport = FakeTransport::json(&[]);
        let client = TogglClient::new(&settings, &transport, NOW);

        assert!(!client.is_connected());
        assert_eq!(client.sync().unwrap_err(), TogglError::SetupIncomplete);
        assert!(transport.urls().is_empty(), "nothing should be requested");
    }

    #[test]
    fn connecting_stores_the_key_and_the_organization() {
        let settings = MemorySettings::default();
        let transport =
            FakeTransport::json(&[r#"{"current_organization_id":123,"current_workspace_id":456}"#]);
        let client = TogglClient::new(&settings, &transport, NOW);

        let organization = client.connect(" toggl_sk_abc ").expect("connect");

        assert_eq!(organization, Some(123));
        assert_eq!(settings.get(TOKEN_KEY).as_deref(), Some("toggl_sk_abc"));
        let cache = client.cache();
        assert_eq!(cache.default_organization_id, Some(123));
        assert_eq!(cache.default_workspace_id, Some(456));
        assert_eq!(
            transport.requests.borrow()[0].token,
            "toggl_sk_abc",
            "the new key is used for its own verification"
        );
    }

    #[test]
    fn connecting_without_an_organization_asks_for_setup() {
        let settings = MemorySettings::default();
        let transport = FakeTransport::json(&[r#"{"current_workspace_id":456}"#]);
        let client = TogglClient::new(&settings, &transport, NOW);

        assert_eq!(client.connect("toggl_sk_abc").expect("connect"), None);
    }

    #[test]
    fn connecting_a_different_account_drops_the_previous_cache_and_timer() {
        let settings = MemorySettings::connected();
        settings.set(
            CURRENT_ENTRY_KEY,
            r#"{"id":9,"start":"2023-11-14T22:00:00Z"}"#,
        );
        let transport = FakeTransport::json(&[r#"{"current_organization_id":777}"#]);
        let client = TogglClient::new(&settings, &transport, NOW);

        client.connect("toggl_sk_other").expect("connect");

        assert_eq!(client.cache().default_organization_id, Some(777));
        assert_eq!(
            client.cache().default_workspace_id,
            None,
            "the old workspace is gone"
        );
        assert!(client.current_entry().is_none(), "the old timer is gone");
    }

    #[test]
    fn reconnecting_the_same_account_keeps_what_was_cached() {
        let settings = MemorySettings::connected();
        settings.set(
            CURRENT_ENTRY_KEY,
            r#"{"id":9,"start":"2023-11-14T22:00:00Z"}"#,
        );
        let transport = FakeTransport::json(&[r#"{"current_organization_id":123}"#]);
        let client = TogglClient::new(&settings, &transport, NOW);

        client.connect("toggl_sk_test").expect("connect");

        assert_eq!(client.cache().default_workspace_id, Some(456));
        assert!(client.current_entry().is_some());
    }

    #[test]
    fn disconnecting_clears_every_stored_value() {
        let settings = MemorySettings::connected();
        settings.set(CURRENT_ENTRY_KEY, r#"{"id":9}"#);
        settings.set(
            QUOTA_KEY,
            r#"{"remaining":5,"resetsInSeconds":60,"observedAt":1}"#,
        );
        let transport = FakeTransport::json(&[]);
        let client = TogglClient::new(&settings, &transport, NOW);

        client.disconnect();

        assert!(!client.is_connected());
        assert!(client.current_entry().is_none());
        assert!(client.quota().is_none());
        assert_eq!(client.cache(), crate::types::TogglCache::default());
    }

    #[test]
    fn quota_headers_are_recorded_and_counted_down() {
        let settings = MemorySettings::connected();
        let transport = FakeTransport::with_quota("null", "28", "1800");
        let client = TogglClient::new(&settings, &transport, NOW);

        client.refresh_current_entry().expect("refresh");

        let quota = client.quota().expect("a quota was recorded");
        assert_eq!(quota.remaining, 28);
        assert_eq!(quota.resets_in_seconds, 1_800);

        // Ten minutes later, the countdown has moved.
        let later = TogglClient::new(&settings, &transport, NOW + 600_000);
        assert_eq!(
            later.quota_line().as_deref(),
            Some("quota 28 · resets in 20m 0s")
        );
    }

    #[test]
    fn a_response_without_quota_headers_leaves_the_previous_reading_alone() {
        let settings = MemorySettings::connected();
        settings.set(
            QUOTA_KEY,
            r#"{"remaining":5,"resetsInSeconds":60,"observedAt":1700000000000}"#,
        );
        let transport = FakeTransport::json(&["null"]);
        let client = TogglClient::new(&settings, &transport, NOW);

        client.refresh_current_entry().expect("refresh");

        assert_eq!(client.quota().expect("quota").remaining, 5);
    }

    #[test]
    fn error_statuses_become_messages_the_reader_can_act_on() {
        let settings = MemorySettings::connected();

        let unauthorized = FakeTransport::status(401, "{}");
        let error = TogglClient::new(&settings, &unauthorized, NOW)
            .refresh_current_entry()
            .expect_err("401");
        assert!(
            error.to_string().contains("/toggl auth <toggl_sk_...>"),
            "{error}"
        );

        settings.set(
            QUOTA_KEY,
            r#"{"remaining":0,"resetsInSeconds":900,"observedAt":1700000000000}"#,
        );
        let exhausted = FakeTransport::status(402, "{}");
        let error = TogglClient::new(&settings, &exhausted, NOW)
            .refresh_current_entry()
            .expect_err("402");
        assert_eq!(
            error.to_string(),
            "Toggl quota exhausted (402). Try again in 15m 0s."
        );

        let denied = FakeTransport::status(403, r#"{"message":"workspace is read-only"}"#);
        let error = TogglClient::new(&settings, &denied, NOW)
            .refresh_current_entry()
            .expect_err("403");
        assert!(
            error.to_string().ends_with(": workspace is read-only"),
            "{error}"
        );

        let other = FakeTransport::status(500, "server exploded");
        let error = TogglClient::new(&settings, &other, NOW)
            .refresh_current_entry()
            .expect_err("500");
        assert_eq!(error.to_string(), "Toggl Focus API 500: server exploded");
    }

    #[test]
    fn a_transport_failure_is_reported_as_unreachable() {
        let settings = MemorySettings::connected();
        let transport = FakeTransport::new(vec![Err("dns failure".to_owned())]);
        let client = TogglClient::new(&settings, &transport, NOW);

        let error = client.refresh_current_entry().expect_err("transport");
        assert_eq!(error.to_string(), "Could not reach Toggl: dns failure");
    }

    #[test]
    fn syncing_follows_paging_and_sorts_projects() {
        let settings = MemorySettings::connected();
        let transport = FakeTransport::json(&[
            // Two full pages of projects, then a short one.
            &format!(
                r#"{{"data":[{}],"per_page":2,"total":3}}"#,
                r#"{"id":1,"workspace_id":456,"name":"Zebra"},{"id":2,"workspace_id":456,"name":"apple"}"#
            ),
            r#"{"data":[{"id":3,"workspace_id":456,"name":"Mango","client":{"name":"Personal"}}],"per_page":2,"total":3}"#,
            // Recent entries.
            r#"{"data":[{"id":10,"workspace_id":456,"description":"Reading","start":"2023-11-14T20:00:00Z"}]}"#,
            // The current-timer refresh that sync performs.
            "null",
        ]);
        let client = TogglClient::new(&settings, &transport, NOW);

        let cache = client.sync().expect("sync");

        assert_eq!(
            cache
                .projects
                .iter()
                .map(|project| project.name.as_str())
                .collect::<Vec<_>>(),
            vec!["apple", "Mango", "Zebra"],
            "projects sort with UI collation"
        );
        assert_eq!(cache.descriptions.len(), 1);
        assert_eq!(cache.default_workspace_id, Some(456));
        assert!(cache.synced_at.is_some());

        let urls = transport.urls();
        assert!(urls[0].contains("/organizations/123/workspaces/456/projects?page=1"));
        assert!(urls[1].contains("page=2"), "paging continues: {urls:?}");
        assert!(urls[2].contains("/time-entries?date_from="));
    }

    #[test]
    fn syncing_skips_archived_projects() {
        let settings = MemorySettings::connected();
        let transport = FakeTransport::json(&[
            r#"{"data":[{"id":1,"workspace_id":456,"name":"Old","active":false},{"id":2,"workspace_id":456,"name":"New","active":true}],"per_page":100,"total":2}"#,
            r#"{"data":[]}"#,
            "null",
        ]);
        let client = TogglClient::new(&settings, &transport, NOW);

        let cache = client.sync().expect("sync");

        assert_eq!(cache.projects.len(), 1);
        assert_eq!(cache.projects[0].name, "New");
    }

    #[test]
    fn setup_from_a_url_is_rolled_back_when_the_organization_is_refused() {
        let settings = MemorySettings::connected();
        let transport = FakeTransport::status(403, "{}");
        let client = TogglClient::new(&settings, &transport, NOW);

        let error = client
            .complete_setup("https://focus.toggl.com/organizations/999/workspaces/888")
            .expect_err("403");

        assert!(matches!(error, TogglError::Api { status: 403, .. }));
        assert_eq!(
            client.cache().default_organization_id,
            Some(123),
            "the previous scope is restored"
        );
    }

    #[test]
    fn setup_rejects_something_that_is_not_a_workspace_url() {
        let settings = MemorySettings::connected();
        let transport = FakeTransport::json(&[]);
        let client = TogglClient::new(&settings, &transport, NOW);

        let error = client
            .complete_setup("my workspace")
            .expect_err("bad input");
        assert!(
            error
                .to_string()
                .starts_with("Could not find an organization ID")
        );
    }

    #[test]
    fn starting_a_timer_stores_it_and_names_its_project() {
        let settings = MemorySettings::connected();
        settings.set(
            CACHE_KEY,
            r#"{"defaultOrganizationId":123,"defaultWorkspaceId":456,"projects":[{"id":7,"workspaceId":456,"name":"Reading books"}],"descriptions":[]}"#,
        );
        let transport = FakeTransport::json(&[
            r#"{"id":42,"workspace_id":456,"project_id":7,"description":"O Nome do Vento","start":"2023-11-14T22:13:20.000Z"}"#,
        ]);
        let client = TogglClient::new(&settings, &transport, NOW);

        let entry = client
            .start_entry("O Nome do Vento", Some("reading"))
            .expect("start");

        assert_eq!(entry.id, 42);
        assert_eq!(client.current_entry().expect("stored").id, 42);
        let body = transport.bodies()[0].clone().expect("a payload");
        assert!(body.contains("\"project_id\":7"), "{body}");
        assert!(
            body.contains("\"start\":\"2023-11-14T22:13:20.000Z\""),
            "{body}"
        );
    }

    #[test]
    fn naming_a_project_that_is_not_cached_is_refused_before_the_request() {
        let settings = MemorySettings::connected();
        let transport = FakeTransport::json(&[]);
        let client = TogglClient::new(&settings, &transport, NOW);

        let error = client
            .start_entry("Reading", Some("nonexistent"))
            .expect_err("unknown project");

        assert_eq!(
            error.to_string(),
            "Toggl project \"nonexistent\" was not found. Run /toggl sync and try again."
        );
        assert!(transport.urls().is_empty(), "nothing should be sent");
    }

    #[test]
    fn stopping_a_timer_that_is_already_stopped_is_not_an_error() {
        let settings = MemorySettings::connected();
        settings.set(
            CURRENT_ENTRY_KEY,
            r#"{"id":42,"start":"2023-11-14T22:00:00Z"}"#,
        );
        let transport = FakeTransport::status(404, "{}");
        let client = TogglClient::new(&settings, &transport, NOW);

        assert_eq!(client.stop_entry().expect("stop"), None);
        assert!(
            client.current_entry().is_none(),
            "the stale record is cleared"
        );
    }

    #[test]
    fn stopping_a_running_timer_clears_it() {
        let settings = MemorySettings::connected();
        settings.set(
            CURRENT_ENTRY_KEY,
            r#"{"id":42,"start":"2023-11-14T22:00:00Z"}"#,
        );
        let transport =
            FakeTransport::json(&[r#"{"id":42,"stop":"2023-11-14T22:13:20.000Z","duration":800}"#]);
        let client = TogglClient::new(&settings, &transport, NOW);

        let stopped = client.stop_entry().expect("stop").expect("an entry");

        assert_eq!(stopped.id, 42);
        assert!(client.current_entry().is_none());
    }

    #[test]
    fn logging_time_backdates_the_start_by_the_duration() {
        let settings = MemorySettings::connected();
        let transport = FakeTransport::json(&[r#"{"id":50,"duration":2700}"#]);
        let client = TogglClient::new(&settings, &transport, NOW);

        client.log_entry("Choujin X", 2_700, None).expect("log");

        let body = transport.bodies()[0].clone().expect("a payload");
        assert!(body.contains("\"duration\":2700"), "{body}");
        assert!(
            body.contains("\"start\":\"2023-11-14T21:28:20.000Z\""),
            "45 minutes before now: {body}"
        );
        assert!(
            body.contains("\"tracked_at\":\"2023-11-14T22:13:20.000Z\""),
            "{body}"
        );
    }

    #[test]
    fn a_refresh_does_not_overwrite_a_timer_started_while_it_was_in_flight() {
        struct RacingSettings {
            inner: MemorySettings,
        }

        impl SettingsStore for RacingSettings {
            fn get(&self, key: &str) -> Option<String> {
                // The first read happens before the request; the second read,
                // after it, sees a timer another command just stored.
                if key == CURRENT_ENTRY_KEY {
                    let existing = self.inner.get(key).unwrap_or_default();
                    if existing.is_empty() {
                        self.inner
                            .set(key, r#"{"id":99,"start":"2023-11-14T22:00:00Z"}"#);
                        return Some(String::new());
                    }
                }
                self.inner.get(key)
            }

            fn set(&self, key: &str, value: &str) {
                self.inner.set(key, value);
            }
        }

        let settings = RacingSettings {
            inner: MemorySettings::connected(),
        };
        let transport = FakeTransport::json(&["null"]);
        let client = TogglClient::new(&settings, &transport, NOW);

        client.refresh_current_entry().expect("refresh");

        assert_eq!(
            client.current_entry().map(|entry| entry.id),
            Some(99),
            "the newer timer must survive a slower refresh"
        );
    }

    #[test]
    fn project_resolution_prefers_an_exact_name_then_client_then_partial() {
        let settings = MemorySettings::connected();
        settings.set(
            CACHE_KEY,
            r#"{"defaultOrganizationId":123,"defaultWorkspaceId":456,"projects":[
                {"id":1,"workspaceId":456,"name":"Reading manga","clientName":"Personal"},
                {"id":2,"workspaceId":456,"name":"Reading"}
            ],"descriptions":[]}"#,
        );
        let transport = FakeTransport::json(&[]);
        let client = TogglClient::new(&settings, &transport, NOW);

        assert_eq!(client.resolve_project("Reading").map(|p| p.id), Some(2));
        assert_eq!(client.resolve_project("personal").map(|p| p.id), Some(1));
        assert_eq!(client.resolve_project("manga").map(|p| p.id), Some(1));
        assert_eq!(client.resolve_project("  "), None);
        assert_eq!(client.resolve_project("nope"), None);
    }

    #[test]
    fn the_running_timer_line_shows_elapsed_time_and_a_description() {
        let settings = MemorySettings::connected();
        settings.set(
            CURRENT_ENTRY_KEY,
            r#"{"id":42,"description":"Reading","start":"2023-11-14T21:13:20.000Z"}"#,
        );
        let transport = FakeTransport::json(&[]);
        let client = TogglClient::new(&settings, &transport, NOW);

        assert_eq!(
            client.running_timer_line().as_deref(),
            Some("Toggl 1h 0m · Reading")
        );
    }

    #[test]
    fn a_timer_without_a_description_falls_back_to_its_id() {
        let settings = MemorySettings::connected();
        settings.set(
            CURRENT_ENTRY_KEY,
            r#"{"id":42,"description":"   ","start":"2023-11-14T22:03:20.000Z"}"#,
        );
        let transport = FakeTransport::json(&[]);
        let client = TogglClient::new(&settings, &transport, NOW);

        assert_eq!(
            client.running_timer_line().as_deref(),
            Some("Toggl 10m · #42")
        );
    }

    #[test]
    fn a_stopped_or_missing_timer_shows_no_line() {
        let settings = MemorySettings::connected();
        let transport = FakeTransport::json(&[]);
        let client = TogglClient::new(&settings, &transport, NOW);
        assert!(client.running_timer_line().is_none());

        settings.set(
            CURRENT_ENTRY_KEY,
            r#"{"id":42,"start":"2023-11-14T21:00:00Z","stop":"2023-11-14T22:00:00Z"}"#,
        );
        assert!(client.running_timer_line().is_none());
    }

    #[test]
    fn the_recents_report_covers_scope_projects_descriptions_and_quota() {
        let settings = MemorySettings::connected();
        settings.set(
            CACHE_KEY,
            r#"{"defaultOrganizationId":123,"defaultWorkspaceId":456,
                "projects":[{"id":1,"workspaceId":456,"name":"Reading","clientName":"Personal"}],
                "descriptions":[{"description":"Choujin X","workspaceId":456,"lastUsedAt":"2023-11-14T20:00:00Z"}],
                "syncedAt":"2023-11-14T22:00:00.000Z"}"#,
        );
        settings.set(
            QUOTA_KEY,
            r#"{"remaining":28,"resetsInSeconds":600,"observedAt":1700000000000}"#,
        );
        let transport = FakeTransport::json(&[]);
        let client = TogglClient::new(&settings, &transport, NOW);

        let report = client.recents_report();

        assert_eq!(report[0], "Organization 123 · Workspace 456");
        assert_eq!(report[1], "Projects (1)");
        assert_eq!(report[2], "  Personal / Reading");
        assert!(report.iter().any(|line| line == "  Choujin X"));
        assert!(report.iter().any(|line| line.starts_with("API quota 28")));
        assert!(report.iter().any(|line| line.starts_with("Synced ")));
    }

    #[test]
    fn an_unconfigured_account_still_reports_something_readable() {
        let settings = MemorySettings::default();
        let transport = FakeTransport::json(&[]);
        let client = TogglClient::new(&settings, &transport, NOW);

        let report = client.recents_report();
        assert_eq!(
            report[0],
            "Organization not configured · Workspace not configured"
        );
    }

    #[test]
    fn completions_come_from_the_cache_in_the_command_bar_shape() {
        let settings = MemorySettings::connected();
        settings.set(
            CACHE_KEY,
            r#"{"defaultOrganizationId":123,"defaultWorkspaceId":456,
                "projects":[{"id":1,"workspaceId":456,"name":"Reading","clientName":"Personal"}],
                "descriptions":[{"description":"Choujin X","workspaceId":456,"lastUsedAt":"2023-11-14T20:00:00Z"}]}"#,
        );
        let transport = FakeTransport::json(&[]);
        let client = TogglClient::new(&settings, &transport, NOW);

        let completions = client.completions();

        assert_eq!(completions.projects.len(), 1);
        assert_eq!(completions.projects[0].name, "Reading");
        assert_eq!(
            completions.projects[0].client_name.as_deref(),
            Some("Personal")
        );
        assert_eq!(completions.descriptions, vec!["Choujin X"]);
    }

    #[test]
    fn descriptions_are_deduplicated_case_insensitively_and_capped() {
        let entries: Vec<TogglTimeEntry> = (0..40)
            .map(|index| TogglTimeEntry {
                id: index,
                workspace_id: Some(456),
                description: Some(format!("Entry {}", index % 30)),
                start: Some("2023-11-14T20:00:00Z".into()),
                ..TogglTimeEntry::default()
            })
            .chain([TogglTimeEntry {
                id: 100,
                workspace_id: Some(456),
                description: Some("ENTRY 0".into()),
                ..TogglTimeEntry::default()
            }])
            .collect();

        let unique = unique_descriptions(&entries, NOW);

        assert_eq!(unique.len(), 25, "the list is capped");
        assert_eq!(unique[0].description, "Entry 0");
        assert_eq!(
            unique
                .iter()
                .filter(|item| item.description.eq_ignore_ascii_case("entry 0"))
                .count(),
            1
        );
    }

    #[test]
    fn entries_without_a_description_or_workspace_are_skipped() {
        let entries = vec![
            TogglTimeEntry {
                id: 1,
                workspace_id: Some(456),
                description: Some("   ".into()),
                ..TogglTimeEntry::default()
            },
            TogglTimeEntry {
                id: 2,
                description: Some("No workspace".into()),
                ..TogglTimeEntry::default()
            },
        ];
        assert!(unique_descriptions(&entries, NOW).is_empty());
    }

    #[test]
    fn timestamps_round_trip_in_the_api_format() {
        assert_eq!(iso8601(NOW), "2023-11-14T22:13:20.000Z");
        assert_eq!(parse_iso8601_millis("2023-11-14T22:13:20.000Z"), Some(NOW));
        assert_eq!(
            parse_iso8601_millis("2023-11-14T22:13:20Z"),
            Some(NOW),
            "the API sometimes omits milliseconds"
        );
        assert_eq!(parse_iso8601_millis("nonsense"), None);
    }
}

//! Wiring the Toggl integration into the command executor.
//!
//! The integration crate knows nothing about the database and the executor knows
//! nothing about HTTP; this module is the seam. It also owns the wording of what
//! `/toggl` reports, since that is reader-facing rather than protocol detail.

use reader_core::command::ParsedCommand;
use reader_integrations::toggl::SettingsStore;
use reader_integrations::{TOKEN_PAGE, TogglClient, Transport, parse_duration_seconds};
use reader_storage::Storage;

use crate::executor::CommandContext;
use crate::state::ReaderState;

/// Adapts the library database to the integration's settings interface.
pub struct StorageSettings<'a> {
    storage: &'a Storage,
}

impl<'a> StorageSettings<'a> {
    #[must_use]
    pub const fn new(storage: &'a Storage) -> Self {
        Self { storage }
    }
}

impl SettingsStore for StorageSettings<'_> {
    fn get(&self, key: &str) -> Option<String> {
        self.storage.setting(key).ok().flatten()
    }

    fn set(&self, key: &str, value: &str) {
        // A failed settings write must not abort a command; the integration
        // still works this session, it just will not be remembered.
        let _ = self.storage.set_setting(key, value);
    }
}

/// What the reader should do after a `/toggl` command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TogglOutcome {
    /// Show this in the status line.
    Status(String),
    /// Show these lines in the diagnostics overlay.
    Report(Vec<String>),
    /// Prefill the command bar, so the reader can paste a workspace URL.
    Prompt { status: String, prefill: String },
}

/// Run a `/toggl` command.
///
/// Every failure becomes a status message: an integration that cannot reach its
/// API must not take the reader down with it.
pub fn run<S: SettingsStore, T: Transport>(
    parsed: &ParsedCommand,
    client: &TogglClient<'_, S, T>,
) -> TogglOutcome {
    if parsed.has_flag("disconnect") {
        client.disconnect();
        return TogglOutcome::Status("Toggl disconnected.".to_owned());
    }

    let action = parsed
        .args
        .first()
        .map(|value| value.to_lowercase())
        .unwrap_or_else(|| "recent".to_owned());
    let rest = parsed.args[1.min(parsed.args.len())..].join(" ");
    let rest = rest.trim();
    let project = parsed.flag_value("project");

    match action.as_str() {
        "auth" => auth(client, rest, parsed.has_flag("open")),
        "setup" => setup(client, rest),
        "sync" => match client.sync() {
            Ok(cache) => with_quota(
                client,
                format!(
                    "Synced Toggl: {} projects, {} recent names.",
                    cache.projects.len(),
                    cache.descriptions.len()
                ),
            ),
            Err(error) => TogglOutcome::Status(error.to_string()),
        },
        "recent" => TogglOutcome::Report(client.recents_report()),
        "start" => {
            if rest.is_empty() {
                return TogglOutcome::Status(
                    "Use /toggl start <description> [--project name]".to_owned(),
                );
            }
            match client.start_entry(rest, project) {
                Ok(entry) => with_quota(
                    client,
                    format!(
                        "Started Toggl timer: {}",
                        entry.description.unwrap_or_else(|| rest.to_owned())
                    ),
                ),
                Err(error) => TogglOutcome::Status(error.to_string()),
            }
        }
        "stop" => match client.stop_entry() {
            Ok(Some(_)) => with_quota(client, "Stopped the Toggl timer.".to_owned()),
            Ok(None) => TogglOutcome::Status("No Toggl timer was running.".to_owned()),
            Err(error) => TogglOutcome::Status(error.to_string()),
        },
        "log" => log(client, rest, parsed.flag_value("duration"), project),
        unknown => TogglOutcome::Status(format!(
            "Unknown Toggl action \"{unknown}\". Actions: auth · setup · sync · recent · start · stop · log"
        )),
    }
}

fn auth<S: SettingsStore, T: Transport>(
    client: &TogglClient<'_, S, T>,
    token: &str,
    open_requested: bool,
) -> TogglOutcome {
    if open_requested || token.is_empty() {
        // Kept short so the link survives an 80-column footer: the URL is the
        // part the reader actually needs.
        return TogglOutcome::Status(format!("Get a key at {TOKEN_PAGE}, then /toggl auth <key>"));
    }
    match client.connect(token) {
        Err(error) => TogglOutcome::Status(error.to_string()),
        // Focus did not name an organization, so the reader has to supply one.
        Ok(None) => TogglOutcome::Prompt {
            status: "Toggl key connected. Paste your Focus workspace URL once to finish setup."
                .to_owned(),
            prefill: "toggl setup ".to_owned(),
        },
        Ok(Some(_)) => match client.sync() {
            Ok(cache) => with_quota(
                client,
                format!(
                    "Connected Toggl 2.0. Cached {} projects and {} recent names.",
                    cache.projects.len(),
                    cache.descriptions.len()
                ),
            ),
            Err(error) => TogglOutcome::Status(error.to_string()),
        },
    }
}

fn setup<S: SettingsStore, T: Transport>(
    client: &TogglClient<'_, S, T>,
    pasted: &str,
) -> TogglOutcome {
    if pasted.is_empty() {
        return TogglOutcome::Prompt {
            status: "Paste your Focus workspace URL to finish Toggl setup.".to_owned(),
            prefill: "toggl setup ".to_owned(),
        };
    }
    match client.complete_setup(pasted) {
        Ok(cache) => with_quota(
            client,
            format!(
                "Toggl setup complete. Cached {} projects and {} recent names.",
                cache.projects.len(),
                cache.descriptions.len()
            ),
        ),
        Err(error) => TogglOutcome::Status(error.to_string()),
    }
}

fn log<S: SettingsStore, T: Transport>(
    client: &TogglClient<'_, S, T>,
    description: &str,
    duration: Option<&str>,
    project: Option<&str>,
) -> TogglOutcome {
    if description.is_empty() {
        return TogglOutcome::Status(
            "Use /toggl log <description> --duration 25m [--project name]".to_owned(),
        );
    }
    let Some(duration) = duration else {
        return TogglOutcome::Status(
            "Use /toggl log <description> --duration 25m [--project name]".to_owned(),
        );
    };
    let seconds = match parse_duration_seconds(duration) {
        Ok(seconds) => seconds,
        Err(error) => return TogglOutcome::Status(error.to_string()),
    };
    match client.log_entry(description, seconds, project) {
        Ok(_) => with_quota(client, format!("Logged {duration} to Toggl.")),
        Err(error) => TogglOutcome::Status(error.to_string()),
    }
}

/// Append the remaining API budget, when one is known.
fn with_quota<S: SettingsStore, T: Transport>(
    client: &TogglClient<'_, S, T>,
    message: String,
) -> TogglOutcome {
    TogglOutcome::Status(match client.quota_line() {
        Some(quota) => format!("{message} · {quota}"),
        None => message,
    })
}

/// Apply an outcome to the reader.
pub fn apply_outcome(state: &mut ReaderState, outcome: TogglOutcome) -> Option<String> {
    match outcome {
        TogglOutcome::Status(status) => {
            state.status = status;
            None
        }
        TogglOutcome::Report(lines) => {
            state.status = lines.first().cloned().unwrap_or_default();
            state.integration_report = lines;
            state.overlay = crate::state::Overlay::Diagnostics;
            state.overlay_cursor = 0;
            None
        }
        TogglOutcome::Prompt { status, prefill } => {
            state.status = status;
            Some(prefill)
        }
    }
}

/// Whether enough time has passed to poll the running timer again.
///
/// Focus allows 30 requests an hour on the free plan, so the background refresh
/// is deliberately slow and user-typed commands keep the headroom.
#[must_use]
pub fn should_refresh(last_refresh: Option<i64>, context: CommandContext) -> bool {
    last_refresh.is_none_or(|last| context.now - last >= reader_integrations::REFRESH_INTERVAL_MS)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    use reader_core::command::parse_slash_command;
    use reader_integrations::http::{HttpRequest, HttpResponse};
    use reader_integrations::toggl::{CACHE_KEY, SettingsStore, TOKEN_KEY};
    use reader_integrations::{TogglClient, Transport};

    use super::{TogglOutcome, run, should_refresh};
    use crate::executor::CommandContext;

    const NOW: i64 = 1_700_000_000_000;
    const CONTEXT: CommandContext = CommandContext {
        now: NOW,
        content_width: 80,
        body_height: 20,
    };

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

    struct StubTransport {
        responses: RefCell<Vec<HttpResponse>>,
    }

    impl StubTransport {
        fn new(bodies: &[&str]) -> Self {
            Self {
                responses: RefCell::new(
                    bodies
                        .iter()
                        .rev()
                        .map(|body| HttpResponse::ok(*body))
                        .collect(),
                ),
            }
        }
    }

    impl Transport for StubTransport {
        fn send(&self, _request: &HttpRequest) -> Result<HttpResponse, String> {
            self.responses
                .borrow_mut()
                .pop()
                .ok_or_else(|| "no response queued".to_owned())
        }
    }

    fn outcome(
        command: &str,
        settings: &MemorySettings,
        transport: &StubTransport,
    ) -> TogglOutcome {
        let parsed = parse_slash_command(command).expect("valid command");
        let client = TogglClient::new(settings, transport, NOW);
        run(&parsed, &client)
    }

    #[test]
    fn auth_without_a_key_points_at_the_key_page() {
        let settings = MemorySettings::default();
        let transport = StubTransport::new(&[]);

        let TogglOutcome::Status(status) = outcome("/toggl auth", &settings, &transport) else {
            panic!("expected a status");
        };
        assert!(status.contains("focus.toggl.com/settings"), "{status}");
    }

    #[test]
    fn auth_with_a_key_connects_and_syncs() {
        let settings = MemorySettings::default();
        let transport = StubTransport::new(&[
            r#"{"current_organization_id":123,"current_workspace_id":456}"#,
            r#"{"data":[{"id":1,"workspace_id":456,"name":"Reading"}],"per_page":100,"total":1}"#,
            r#"{"data":[{"id":9,"workspace_id":456,"description":"Choujin X","start":"2023-11-14T20:00:00Z"}]}"#,
            "null",
        ]);

        let TogglOutcome::Status(status) =
            outcome("/toggl auth toggl_sk_abc", &settings, &transport)
        else {
            panic!("expected a status");
        };
        assert_eq!(
            status,
            "Connected Toggl 2.0. Cached 1 projects and 1 recent names."
        );
    }

    #[test]
    fn an_account_without_an_organization_prompts_for_the_workspace_url() {
        let settings = MemorySettings::default();
        let transport = StubTransport::new(&[r#"{"current_workspace_id":456}"#]);

        let TogglOutcome::Prompt { status, prefill } =
            outcome("/toggl auth toggl_sk_abc", &settings, &transport)
        else {
            panic!("expected a prompt");
        };
        assert!(
            status.contains("Paste your Focus workspace URL"),
            "{status}"
        );
        assert_eq!(prefill, "toggl setup ");
    }

    #[test]
    fn setup_without_a_url_prompts_and_with_one_completes() {
        let settings = MemorySettings::connected();
        let empty = StubTransport::new(&[]);
        assert!(matches!(
            outcome("/toggl setup", &settings, &empty),
            TogglOutcome::Prompt { .. }
        ));

        let transport = StubTransport::new(&[
            r#"{"data":[],"per_page":100,"total":0}"#,
            r#"{"data":[]}"#,
            "null",
        ]);
        let TogglOutcome::Status(status) = outcome(
            "/toggl setup https://focus.toggl.com/organizations/123/workspaces/456",
            &settings,
            &transport,
        ) else {
            panic!("expected a status");
        };
        assert!(status.starts_with("Toggl setup complete."), "{status}");
    }

    #[test]
    fn recent_opens_a_report_rather_than_a_status_line() {
        let settings = MemorySettings::connected();
        let transport = StubTransport::new(&[]);

        let TogglOutcome::Report(lines) = outcome("/toggl recent", &settings, &transport) else {
            panic!("expected a report");
        };
        assert!(lines[0].starts_with("Organization 123"), "{lines:?}");
    }

    #[test]
    fn a_bare_toggl_command_reports_recents() {
        let settings = MemorySettings::connected();
        let transport = StubTransport::new(&[]);
        assert!(matches!(
            outcome("/toggl", &settings, &transport),
            TogglOutcome::Report(_)
        ));
    }

    #[test]
    fn start_needs_a_description_and_reports_the_started_timer() {
        let settings = MemorySettings::connected();
        let empty = StubTransport::new(&[]);
        let TogglOutcome::Status(status) = outcome("/toggl start", &settings, &empty) else {
            panic!("expected a status");
        };
        assert_eq!(status, "Use /toggl start <description> [--project name]");

        let transport = StubTransport::new(&[r#"{"id":42,"description":"Reading"}"#]);
        let TogglOutcome::Status(status) = outcome("/toggl start Reading", &settings, &transport)
        else {
            panic!("expected a status");
        };
        assert_eq!(status, "Started Toggl timer: Reading");
    }

    #[test]
    fn stop_distinguishes_a_stopped_timer_from_none_at_all() {
        let settings = MemorySettings::connected();
        let running = StubTransport::new(&[r#"{"id":42,"stop":"2023-11-14T22:13:20Z"}"#]);
        let TogglOutcome::Status(status) = outcome("/toggl stop", &settings, &running) else {
            panic!("expected a status");
        };
        assert_eq!(status, "Stopped the Toggl timer.");

        struct NotFound;
        impl Transport for NotFound {
            fn send(&self, _request: &HttpRequest) -> Result<HttpResponse, String> {
                Ok(HttpResponse {
                    status: 404,
                    body: String::new(),
                    headers: std::collections::BTreeMap::new(),
                })
            }
        }
        let parsed = parse_slash_command("/toggl stop").expect("valid");
        let client = TogglClient::new(&settings, &NotFound, NOW);
        assert_eq!(
            run(&parsed, &client),
            TogglOutcome::Status("No Toggl timer was running.".to_owned())
        );
    }

    #[test]
    fn log_requires_a_description_and_a_duration_it_understands() {
        let settings = MemorySettings::connected();
        let empty = StubTransport::new(&[]);

        for command in ["/toggl log", "/toggl log Reading"] {
            let TogglOutcome::Status(status) = outcome(command, &settings, &empty) else {
                panic!("expected a status");
            };
            assert_eq!(
                status, "Use /toggl log <description> --duration 25m [--project name]",
                "{command}"
            );
        }

        let TogglOutcome::Status(status) =
            outcome("/toggl log Reading --duration soon", &settings, &empty)
        else {
            panic!("expected a status");
        };
        assert_eq!(status, "Duration must look like 25m, 1.5h, or 900s.");

        let transport = StubTransport::new(&[r#"{"id":50}"#]);
        let TogglOutcome::Status(status) =
            outcome("/toggl log Reading --duration 45m", &settings, &transport)
        else {
            panic!("expected a status");
        };
        assert_eq!(status, "Logged 45m to Toggl.");
    }

    #[test]
    fn disconnect_clears_the_integration_whatever_else_was_typed() {
        let settings = MemorySettings::connected();
        let transport = StubTransport::new(&[]);

        let outcome = outcome("/toggl sync --disconnect", &settings, &transport);

        assert_eq!(
            outcome,
            TogglOutcome::Status("Toggl disconnected.".to_owned())
        );
        assert_eq!(settings.get(TOKEN_KEY).as_deref(), Some(""));
    }

    #[test]
    fn an_unknown_action_lists_the_ones_that_exist() {
        let settings = MemorySettings::connected();
        let transport = StubTransport::new(&[]);
        let TogglOutcome::Status(status) = outcome("/toggl frobnicate", &settings, &transport)
        else {
            panic!("expected a status");
        };
        assert!(
            status.starts_with("Unknown Toggl action \"frobnicate\""),
            "{status}"
        );
        assert!(status.contains("auth · setup · sync"), "{status}");
    }

    #[test]
    fn a_failing_api_becomes_a_status_message_not_a_crash() {
        let settings = MemorySettings::connected();
        struct Failing;
        impl Transport for Failing {
            fn send(&self, _request: &HttpRequest) -> Result<HttpResponse, String> {
                Err("network is down".to_owned())
            }
        }
        let parsed = parse_slash_command("/toggl sync").expect("valid");
        let client = TogglClient::new(&settings, &Failing, NOW);

        assert_eq!(
            run(&parsed, &client),
            TogglOutcome::Status("Could not reach Toggl: network is down".to_owned())
        );
    }

    #[test]
    fn background_refresh_waits_out_the_interval() {
        assert!(should_refresh(None, CONTEXT), "the first poll is allowed");
        assert!(
            !should_refresh(Some(NOW), CONTEXT),
            "an immediate repoll is not"
        );
        assert!(
            !should_refresh(Some(NOW - 60_000), CONTEXT),
            "a minute later is still too soon"
        );
        assert!(
            should_refresh(
                Some(NOW - reader_integrations::REFRESH_INTERVAL_MS),
                CONTEXT
            ),
            "the interval having elapsed is enough"
        );
    }
}

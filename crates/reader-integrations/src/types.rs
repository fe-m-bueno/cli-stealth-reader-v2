//! What the Toggl integration stores and shows.
//!
//! Everything here is plain data with a v1-compatible JSON shape, because it is
//! persisted in the settings table and a v1 build must still be able to read it
//! during the beta.

use serde::{Deserialize, Serialize};

/// A project the reader can log time against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TogglProject {
    pub id: i64,
    pub workspace_id: i64,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

impl TogglProject {
    /// How the project reads in a list: `Client / Project`, or just the project.
    #[must_use]
    pub fn label(&self) -> String {
        match &self.client_name {
            Some(client) if !client.is_empty() => format!("{client} / {}", self.name),
            _ => self.name.clone(),
        }
    }
}

/// A description the reader has used recently, offered as a completion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TogglRecentDescription {
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<i64>,
    pub workspace_id: i64,
    pub last_used_at: String,
}

/// The API budget left in the current window.
///
/// Focus is quota-limited per user and organization — 30 requests an hour on the
/// free plan — so the reader shows what is left rather than discovering the
/// limit by hitting it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TogglQuota {
    pub remaining: i64,
    pub resets_in_seconds: i64,
    /// When the headers were observed, so the countdown stays honest.
    pub observed_at: i64,
}

/// Everything cached between sessions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TogglCache {
    #[serde(default)]
    pub default_organization_id: Option<i64>,
    #[serde(default)]
    pub default_workspace_id: Option<i64>,
    #[serde(default)]
    pub projects: Vec<TogglProject>,
    #[serde(default)]
    pub descriptions: Vec<TogglRecentDescription>,
    #[serde(default)]
    pub synced_at: Option<String>,
}

/// A time entry, in the shape the Focus API returns it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TogglTimeEntry {
    pub id: i64,
    #[serde(default)]
    pub workspace_id: Option<i64>,
    #[serde(default)]
    pub project_id: Option<i64>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub start: Option<String>,
    #[serde(default)]
    pub duration: Option<i64>,
    #[serde(default)]
    pub stop: Option<String>,
}

impl TogglTimeEntry {
    /// Whether this entry is still running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.start.is_some() && self.stop.is_none()
    }
}

/// Where in Toggl the reader is working.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TogglScope {
    pub organization_id: i64,
    pub workspace_id: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::{TogglCache, TogglProject, TogglTimeEntry};

    #[test]
    fn a_project_reads_with_its_client_when_it_has_one() {
        let mut project = TogglProject {
            id: 1,
            workspace_id: 2,
            name: "Reading books".into(),
            client_name: Some("Personal".into()),
            color: None,
        };
        assert_eq!(project.label(), "Personal / Reading books");

        project.client_name = None;
        assert_eq!(project.label(), "Reading books");

        project.client_name = Some(String::new());
        assert_eq!(
            project.label(),
            "Reading books",
            "an empty client is no client"
        );
    }

    #[test]
    fn an_entry_is_running_until_it_has_a_stop_time() {
        let mut entry = TogglTimeEntry {
            id: 7,
            start: Some("2023-11-14T22:13:20Z".into()),
            ..TogglTimeEntry::default()
        };
        assert!(entry.is_running());

        entry.stop = Some("2023-11-14T23:13:20Z".into());
        assert!(!entry.is_running());

        entry.start = None;
        entry.stop = None;
        assert!(!entry.is_running(), "an entry with no start is not running");
    }

    #[test]
    fn the_cache_round_trips_through_the_v1_json_shape() {
        let cache = TogglCache {
            default_organization_id: Some(123),
            default_workspace_id: Some(456),
            projects: vec![TogglProject {
                id: 1,
                workspace_id: 456,
                name: "Reading".into(),
                client_name: Some("Personal".into()),
                color: Some("#ff0000".into()),
            }],
            descriptions: Vec::new(),
            synced_at: Some("2023-11-14T22:13:20.000Z".into()),
        };

        let json = serde_json::to_value(&cache).expect("serialize");
        assert_eq!(json["defaultOrganizationId"], 123);
        assert_eq!(json["defaultWorkspaceId"], 456);
        assert_eq!(json["projects"][0]["clientName"], "Personal");

        let parsed: TogglCache = serde_json::from_value(json).expect("deserialize");
        assert_eq!(parsed, cache);
    }

    #[test]
    fn a_partial_cache_from_an_older_build_still_loads() {
        let parsed: TogglCache =
            serde_json::from_str(r#"{"defaultOrganizationId":9}"#).expect("deserialize");
        assert_eq!(parsed.default_organization_id, Some(9));
        assert!(parsed.projects.is_empty());
        assert!(parsed.synced_at.is_none());
    }
}

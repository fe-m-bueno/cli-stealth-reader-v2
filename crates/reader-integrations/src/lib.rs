//! Outside services the reader can talk to.
//!
//! Today that is Toggl 2.0 Focus, for logging reading time. The integration owns
//! its HTTP client and its cache but not its storage: state moves through a
//! [`SettingsStore`], so this crate never depends on the database.

pub mod http;
pub mod parse;
pub mod toggl;
pub mod types;

pub use http::{HttpRequest, HttpResponse, NetworkTransport, Transport};
pub use parse::{ParseError, TOKEN_PAGE, extract_scope, parse_duration_seconds};
pub use toggl::{REFRESH_INTERVAL_MS, SettingsStore, TogglClient, TogglError};
pub use types::{TogglCache, TogglProject, TogglQuota, TogglScope, TogglTimeEntry};

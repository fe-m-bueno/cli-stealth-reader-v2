//! Reading what the user pasted, and formatting what the API returned.
//!
//! Focus does not always report which organization a key belongs to, so the
//! reader asks for the workspace URL and digs the ids out of it. People paste
//! whatever their browser shows, so several URL shapes are accepted.

use crate::types::TogglScope;

/// The page where a Toggl 2.0 API key is created.
pub const TOKEN_PAGE: &str = "https://focus.toggl.com/settings";

/// Why a pasted value could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// No organization id could be found in the input.
    NoOrganization,
    /// A duration that is not `25m`, `1.5h`, or `900s`.
    BadDuration,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoOrganization => write!(
                formatter,
                "Could not find an organization ID. Paste the Focus workspace URL or the numeric organization ID."
            ),
            Self::BadDuration => write!(formatter, "Duration must look like 25m, 1.5h, or 900s."),
        }
    }
}

impl std::error::Error for ParseError {}

fn positive(value: i64) -> Option<i64> {
    (value > 0).then_some(value)
}

/// The value of a query parameter, from the first of `keys` that has one.
fn query_id(query: &str, keys: &[&str]) -> Option<i64> {
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=')?;
        if keys.contains(&key)
            && let Ok(id) = value.parse::<i64>()
            && let Some(id) = positive(id)
        {
            return Some(id);
        }
    }
    None
}

/// The numeric segment following `marker` in a path, e.g. `/organizations/123`.
fn path_id_after(path: &str, markers: &[&str]) -> Option<i64> {
    let segments: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    segments.windows(2).find_map(|pair| {
        let name = pair[0].to_lowercase();
        markers
            .contains(&name.as_str())
            .then(|| pair[1].parse::<i64>().ok())
            .flatten()
            .and_then(positive)
    })
}

/// Read an organization, and if present a workspace, out of what was pasted.
///
/// Accepts a bare numeric id, or a `focus.toggl.com` URL in any of the shapes
/// the app produces: `/organizations/<id>/workspaces/<id>`, `/<id>/workspaces/<id>`,
/// or ids in the query string.
pub fn extract_scope(input: &str) -> Result<TogglScope, ParseError> {
    let candidate = input.trim();
    if let Ok(id) = candidate.parse::<i64>()
        && let Some(organization_id) = positive(id)
    {
        return Ok(TogglScope {
            organization_id,
            workspace_id: None,
        });
    }

    // Only Focus URLs are trusted, so a pasted link to anywhere else is refused
    // rather than mined for numbers.
    let rest = candidate
        .strip_prefix("https://")
        .or_else(|| candidate.strip_prefix("http://"))
        .ok_or(ParseError::NoOrganization)?;
    let (host, tail) = rest.split_once('/').unwrap_or((rest, ""));
    if !host.eq_ignore_ascii_case("focus.toggl.com") {
        return Err(ParseError::NoOrganization);
    }
    let (path, query) = tail.split_once('?').unwrap_or((tail, ""));
    let path = format!("/{path}");

    let organization_id = query_id(
        query,
        &[
            "organization_id",
            "organizationId",
            "organization",
            "org_id",
        ],
    )
    .or_else(|| path_id_after(&path, &["organizations", "organization", "orgs", "org"]))
    .or_else(|| {
        // The compact shape is `/<organization>/workspaces/<workspace>`.
        let segments: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
        match segments.as_slice() {
            [organization, workspaces, ..] if workspaces.eq_ignore_ascii_case("workspaces") => {
                organization.parse::<i64>().ok().and_then(positive)
            }
            _ => None,
        }
    });

    let workspace_id = query_id(query, &["workspace_id", "workspaceId", "workspace"])
        .or_else(|| path_id_after(&path, &["workspaces", "workspace"]));

    organization_id
        .map(|organization_id| TogglScope {
            organization_id,
            workspace_id,
        })
        .ok_or(ParseError::NoOrganization)
}

/// Parse `25m`, `1.5h`, or `900s` into seconds. A bare number means minutes.
pub fn parse_duration_seconds(input: &str) -> Result<i64, ParseError> {
    let trimmed = input.trim();
    let (number, unit) = match trimmed.chars().last() {
        Some(last) if last.is_ascii_alphabetic() => (
            &trimmed[..trimmed.len() - last.len_utf8()],
            last.to_ascii_lowercase(),
        ),
        _ => (trimmed, 'm'),
    };
    let value: f64 = number.parse().map_err(|_| ParseError::BadDuration)?;
    if !value.is_finite() || value < 0.0 {
        return Err(ParseError::BadDuration);
    }
    let seconds = match unit {
        'h' => value * 3600.0,
        's' => value,
        'm' => value * 60.0,
        _ => return Err(ParseError::BadDuration),
    };
    Ok(seconds.round() as i64)
}

/// Human-readable seconds: `2h 5m`, `5m 30s`, or `30s`.
#[must_use]
pub fn format_seconds(seconds: i64) -> String {
    let rounded = seconds.max(0);
    let hours = rounded / 3600;
    let minutes = (rounded % 3600) / 60;
    let remainder = rounded % 60;
    if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {remainder}s")
    } else {
        format!("{remainder}s")
    }
}

/// Elapsed time of a running timer, as the footer shows it.
#[must_use]
pub fn format_elapsed(seconds: i64) -> String {
    let minutes = seconds.max(0) / 60;
    let hours = minutes / 60;
    let remainder = minutes % 60;
    if hours > 0 {
        format!("{hours}h {remainder}m")
    } else {
        format!("{remainder}m")
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ParseError, extract_scope, format_elapsed, format_seconds, parse_duration_seconds,
    };

    #[test]
    fn a_bare_number_is_an_organization_id() {
        let scope = extract_scope(" 12345 ").expect("a numeric id is enough");
        assert_eq!(scope.organization_id, 12345);
        assert_eq!(scope.workspace_id, None);
    }

    #[test]
    fn the_full_workspace_url_yields_both_ids() {
        let scope = extract_scope("https://focus.toggl.com/organizations/123/workspaces/456")
            .expect("the canonical URL parses");
        assert_eq!(scope.organization_id, 123);
        assert_eq!(scope.workspace_id, Some(456));
    }

    #[test]
    fn the_compact_and_query_url_shapes_also_work() {
        let compact =
            extract_scope("https://focus.toggl.com/123/workspaces/456/timer").expect("compact URL");
        assert_eq!(compact.organization_id, 123);
        assert_eq!(compact.workspace_id, Some(456));

        let query =
            extract_scope("https://focus.toggl.com/reports?organization_id=99&workspace_id=88")
                .expect("query URL");
        assert_eq!(query.organization_id, 99);
        assert_eq!(query.workspace_id, Some(88));
    }

    #[test]
    fn a_url_without_a_workspace_still_gives_the_organization() {
        let scope =
            extract_scope("https://focus.toggl.com/organizations/77").expect("organization");
        assert_eq!(scope.organization_id, 77);
        assert_eq!(scope.workspace_id, None);
    }

    #[test]
    fn anything_that_is_not_a_focus_url_is_refused() {
        for input in [
            "",
            "not a url",
            "https://track.toggl.com/organizations/123",
            "https://focus.toggl.com/settings",
            "https://focus.toggl.com/organizations/0/workspaces/1",
            "-5",
        ] {
            assert_eq!(
                extract_scope(input),
                Err(ParseError::NoOrganization),
                "{input:?} should be refused"
            );
        }
    }

    #[test]
    fn the_refusal_tells_the_reader_what_to_paste() {
        assert_eq!(
            ParseError::NoOrganization.to_string(),
            "Could not find an organization ID. Paste the Focus workspace URL or the numeric organization ID."
        );
    }

    #[test]
    fn durations_accept_minutes_hours_and_seconds() {
        assert_eq!(parse_duration_seconds("25m"), Ok(1_500));
        assert_eq!(parse_duration_seconds("1.5h"), Ok(5_400));
        assert_eq!(parse_duration_seconds("900s"), Ok(900));
        assert_eq!(
            parse_duration_seconds(" 45 "),
            Ok(2_700),
            "a bare number is minutes"
        );
        assert_eq!(parse_duration_seconds("0.5m"), Ok(30));
        assert_eq!(
            parse_duration_seconds("25M"),
            Ok(1_500),
            "case does not matter"
        );
    }

    #[test]
    fn a_duration_that_makes_no_sense_is_refused() {
        for input in ["", "soon", "-5m", "25x", "1.5.5h"] {
            assert_eq!(
                parse_duration_seconds(input),
                Err(ParseError::BadDuration),
                "{input:?} should be refused"
            );
        }
        assert_eq!(
            ParseError::BadDuration.to_string(),
            "Duration must look like 25m, 1.5h, or 900s."
        );
    }

    #[test]
    fn seconds_format_by_the_largest_useful_unit() {
        assert_eq!(format_seconds(0), "0s");
        assert_eq!(format_seconds(45), "45s");
        assert_eq!(format_seconds(90), "1m 30s");
        assert_eq!(format_seconds(3_600), "1h 0m");
        assert_eq!(format_seconds(7_530), "2h 5m");
        assert_eq!(format_seconds(-10), "0s", "a passed deadline reads as zero");
    }

    #[test]
    fn elapsed_time_reads_in_whole_minutes() {
        assert_eq!(format_elapsed(0), "0m");
        assert_eq!(format_elapsed(59), "0m");
        assert_eq!(format_elapsed(60), "1m");
        assert_eq!(format_elapsed(3_600), "1h 0m");
        assert_eq!(format_elapsed(5_400), "1h 30m");
    }
}

//! Command-bar suggestions and completion.
//!
//! Suggestions are resolved against the text left of the cursor, while the
//! replacement range extends over the full token under the cursor, so completing
//! in the middle of a line does not truncate it. Three sources are tried in
//! order: Toggl values, flags, then command names.

use crate::locale::compare_text;

use super::spec::{COMMANDS, Category, CommandSpec, FlagSpec, find_command};

/// A recent Toggl project offered as a completion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TogglProjectRef {
    pub name: String,
    pub client_name: Option<String>,
}

/// The cached Toggl values the command bar can complete from. The core never
/// fetches these; the integration passes what it already has.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TogglCompletions {
    pub projects: Vec<TogglProjectRef>,
    pub descriptions: Vec<String>,
}

/// One suggestion, plus the buffer range its completion replaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// Command this suggestion belongs to.
    pub name: &'static str,
    pub usage: String,
    pub description: String,
    pub category: &'static str,
    pub detail: String,
    pub aliases: Vec<String>,
    /// The alias that matched, when the typed prefix matched an alias.
    pub matched_alias: Option<String>,
    pub completion: String,
    /// Replacement range, in character offsets into the buffer.
    pub completion_start: usize,
    pub completion_end: usize,
}

/// A token of the command bar, with its character offsets.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BufferToken {
    text: String,
    start: usize,
    end: usize,
}

/// Split the buffer the way the command bar sees it: quotes are consumed but the
/// offsets still cover them, so a completion can replace a quoted value whole.
fn buffer_tokens(input: &str) -> Vec<BufferToken> {
    let chars: Vec<char> = input.chars().collect();
    let mut tokens: Vec<BufferToken> = Vec::new();
    let mut text = String::new();
    let mut start: Option<usize> = None;
    let mut quote: Option<char> = None;

    for (index, char) in chars.iter().copied().enumerate() {
        if start.is_none() && !char.is_whitespace() {
            start = Some(index);
            if char == '"' || char == '\'' {
                quote = Some(char);
                continue;
            }
        }
        if let Some(active) = quote {
            if char == active {
                tokens.push(BufferToken {
                    text: std::mem::take(&mut text),
                    start: start.take().unwrap_or(index),
                    end: index + 1,
                });
                quote = None;
            } else {
                text.push(char);
            }
            continue;
        }
        if char.is_whitespace() {
            if let Some(token_start) = start.take() {
                tokens.push(BufferToken {
                    text: std::mem::take(&mut text),
                    start: token_start,
                    end: index,
                });
            }
            continue;
        }
        if char == '"' || char == '\'' {
            quote = Some(char);
            continue;
        }
        text.push(char);
    }
    if let Some(token_start) = start {
        tokens.push(BufferToken {
            text,
            start: token_start,
            end: chars.len(),
        });
    }
    tokens
}

fn char_len(text: &str) -> usize {
    text.chars().count()
}

fn slice_chars(text: &str, range: std::ops::Range<usize>) -> String {
    text.chars()
        .skip(range.start)
        .take(range.end.saturating_sub(range.start))
        .collect()
}

fn format_flag(spec: &FlagSpec) -> String {
    let names = match spec.alias {
        Some(alias) => format!("-{alias}, --{}", spec.name),
        None => format!("--{}", spec.name),
    };
    if spec.takes_value {
        format!("{names} <value>")
    } else {
        names
    }
}

/// The trailing detail line of a command suggestion.
fn suggestion_detail(command: &CommandSpec, matched_alias: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    if matched_alias != command.name {
        parts.push(format!("alias /{matched_alias}"));
    } else if !command.aliases.is_empty() {
        parts.push(format!(
            "alias {}",
            command
                .aliases
                .iter()
                .map(|alias| format!("/{alias}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !command.flags.is_empty() {
        parts.push(format!(
            "flags {}",
            command
                .flags
                .iter()
                .map(format_flag)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if let Some(example) = command.examples.first() {
        parts.push(format!("try {example}"));
    }
    parts.join(" · ")
}

/// The name or alias of `command` that starts with `prefix`.
fn matches_prefix(command: &'static CommandSpec, prefix: &str) -> Option<&'static str> {
    if prefix.is_empty() || command.name.starts_with(prefix) {
        return Some(command.name);
    }
    command
        .aliases
        .iter()
        .copied()
        .find(|alias| alias.starts_with(prefix))
}

fn quote_completion(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\\\""))
}

fn contains_ignoring_case(value: &str, query: &str) -> bool {
    value.to_lowercase().contains(&query.to_lowercase())
}

fn toggl_suggestion(
    usage: String,
    description: String,
    completion: String,
    start: usize,
    end: usize,
) -> Suggestion {
    Suggestion {
        name: "toggl",
        usage,
        description,
        category: Category::Integrations.label(),
        detail: "Tab complete".to_owned(),
        aliases: Vec::new(),
        matched_alias: None,
        completion,
        completion_start: start,
        completion_end: end,
    }
}

const TOGGL_ACTIONS: [&str; 7] = ["auth", "setup", "sync", "recent", "start", "stop", "log"];

/// Flags of the command being typed that are still unused and match the prefix.
fn flag_suggestions(buffer: &str, cursor: usize) -> Vec<Suggestion> {
    let position = cursor.min(char_len(buffer));
    let active = slice_chars(buffer, 0..position);
    if active.ends_with(char::is_whitespace) {
        return Vec::new();
    }
    let tokens = buffer_tokens(&active);
    let (Some(command_token), Some(active_token)) = (tokens.first(), tokens.last()) else {
        return Vec::new();
    };
    if tokens.len() < 2 || !active_token.text.starts_with("--") {
        return Vec::new();
    }
    let Some(command) = find_command(&command_token.text.trim_start_matches('/').to_lowercase())
    else {
        return Vec::new();
    };
    if command.flags.is_empty() {
        return Vec::new();
    }

    let query = active_token.text[2..].to_lowercase();
    let completion_end = buffer_tokens(buffer)
        .into_iter()
        .find(|token| token.start == active_token.start)
        .map_or(active_token.end, |token| token.end);
    let used: Vec<String> = tokens[1..tokens.len() - 1]
        .iter()
        .filter_map(|token| token.text.strip_prefix("--"))
        .map(|flag| {
            flag.split_once('=')
                .map_or_else(|| flag.to_owned(), |(name, _)| name.to_owned())
        })
        .collect();

    // Toggl only accepts certain flags per action.
    let toggl_action = tokens.get(1).map(|token| {
        if token.text.starts_with("--") {
            "recent"
        } else {
            token.text.as_str()
        }
    });
    let allowed: Vec<&FlagSpec> = match (command.name, toggl_action) {
        ("toggl", Some(action)) => {
            let permitted: &[&str] = match action {
                "auth" => &["open"],
                "start" => &["project"],
                "log" => &["project", "duration"],
                "recent" => &["disconnect"],
                _ => &[],
            };
            command
                .flags
                .iter()
                .filter(|spec| permitted.contains(&spec.name))
                .collect()
        }
        _ => command.flags.iter().collect(),
    };

    allowed
        .into_iter()
        .filter(|spec| !used.contains(&spec.name.to_owned()) && spec.name.starts_with(&query))
        .map(|spec| Suggestion {
            name: command.name,
            usage: format!("--{}", spec.name),
            description: if spec.takes_value {
                format!("Set {}", spec.name)
            } else {
                format!("Enable {}", spec.name)
            },
            category: command.category.label(),
            detail: if spec.takes_value {
                "expects a value".to_owned()
            } else {
                "boolean flag".to_owned()
            },
            aliases: spec
                .alias
                .map(|alias| alias.to_string())
                .into_iter()
                .collect(),
            matched_alias: None,
            completion: format!("--{} ", spec.name),
            completion_start: active_token.start,
            completion_end,
        })
        .collect()
}

/// Toggl actions, project names, and recent descriptions.
fn toggl_suggestions(
    buffer: &str,
    cursor: usize,
    completions: Option<&TogglCompletions>,
) -> Vec<Suggestion> {
    let position = cursor.min(char_len(buffer));
    let active = slice_chars(buffer, 0..position);
    let tokens = buffer_tokens(&active);
    let full_tokens = buffer_tokens(buffer);
    if tokens.first().map(|token| token.text.as_str()) != Some("toggl") {
        return Vec::new();
    }
    let action = tokens.get(1);
    if completions.is_none() && action.is_none() {
        return Vec::new();
    }

    // Still typing the action: offer the action list.
    let typing_action = match action {
        None => true,
        Some(token) => {
            tokens.len() == 2
                && !active.ends_with(' ')
                && !TOGGL_ACTIONS.contains(&token.text.as_str())
        }
    };
    if typing_action {
        let query = action.map_or("", |token| token.text.as_str());
        let start = action.map_or(position, |token| token.start);
        let end = action.map_or(position, |token| {
            full_tokens
                .iter()
                .find(|candidate| candidate.start == token.start)
                .map_or(token.end, |candidate| candidate.end)
        });
        let separator = if action.is_some() || active.ends_with(' ') {
            ""
        } else {
            " "
        };
        return TOGGL_ACTIONS
            .into_iter()
            .filter(|candidate| contains_ignoring_case(candidate, query))
            .map(|candidate| {
                toggl_suggestion(
                    candidate.to_owned(),
                    format!("Toggl {candidate}"),
                    format!("{separator}{candidate}"),
                    start,
                    end,
                )
            })
            .collect();
    }

    let action = action.expect("an action is present past this point");
    let Some(completions) = completions.filter(|_| matches!(action.text.as_str(), "start" | "log"))
    else {
        return Vec::new();
    };

    // The value being typed belongs to the last flag, if any.
    let active_flag_index = (2..tokens.len()).rfind(|index| tokens[*index].text.starts_with("--"));
    if let Some(index) = active_flag_index
        && tokens[index].text != "--project"
    {
        return Vec::new();
    }

    if let Some(project_flag_index) = active_flag_index {
        let next_flag_index = tokens
            .iter()
            .enumerate()
            .position(|(index, token)| index > project_flag_index && token.text.starts_with("--"));
        let value_tokens = &tokens[project_flag_index + 1..next_flag_index.unwrap_or(tokens.len())];
        let query = value_tokens
            .iter()
            .map(|token| token.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        let full_project_index = full_tokens
            .iter()
            .position(|token| token.start == tokens[project_flag_index].start);
        let full_value_end = full_project_index.and_then(|start_index| {
            let next = full_tokens
                .iter()
                .enumerate()
                .position(|(index, token)| index > start_index && token.text.starts_with("--"));
            full_tokens[start_index + 1..next.unwrap_or(full_tokens.len())]
                .last()
                .map(|token| token.end)
        });

        let start = value_tokens.first().map_or(position, |token| token.start);
        let end = full_value_end
            .or_else(|| value_tokens.last().map(|token| token.end))
            .unwrap_or(position);
        let prefix = if !value_tokens.is_empty() || active.ends_with(' ') {
            ""
        } else {
            " "
        };

        return completions
            .projects
            .iter()
            .filter(|project| {
                contains_ignoring_case(&project.name, &query)
                    || contains_ignoring_case(
                        format!(
                            "{} {}",
                            project.client_name.as_deref().unwrap_or(""),
                            project.name
                        )
                        .trim(),
                        &query,
                    )
            })
            .map(|project| {
                toggl_suggestion(
                    quote_completion(&project.name),
                    match &project.client_name {
                        Some(client) => format!("{client} / {}", project.name),
                        None => project.name.clone(),
                    },
                    format!("{prefix}{}", quote_completion(&project.name)),
                    start,
                    end,
                )
            })
            .collect();
    }

    // No flag yet: complete the description.
    let description_end = tokens
        .iter()
        .enumerate()
        .position(|(index, token)| index >= 2 && token.text.starts_with("--"));
    let description_tokens = &tokens[2.min(tokens.len())..description_end.unwrap_or(tokens.len())];
    let (Some(first), Some(last)) = (description_tokens.first(), description_tokens.last()) else {
        return Vec::new();
    };
    let query = description_tokens
        .iter()
        .map(|token| token.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let full_description_end = full_tokens
        .iter()
        .enumerate()
        .position(|(index, token)| index >= 2 && token.text.starts_with("--"));
    let end = full_tokens
        [2.min(full_tokens.len())..full_description_end.unwrap_or(full_tokens.len())]
        .last()
        .map_or(last.end, |token| token.end);
    let separator = if buffer.chars().nth(end).is_some_and(char::is_whitespace) {
        ""
    } else {
        " "
    };

    completions
        .descriptions
        .iter()
        .filter(|description| contains_ignoring_case(description, &query))
        .map(|description| {
            toggl_suggestion(
                quote_completion(description),
                "Recent Toggl description".to_owned(),
                format!("{}{separator}", quote_completion(description)),
                first.start,
                end,
            )
        })
        .collect()
}

/// Suggestions for the current command bar contents.
///
/// `cursor` is a character offset; text right of it is preserved.
#[must_use]
pub fn list_command_suggestions(
    buffer: &str,
    cursor: usize,
    toggl: Option<&TogglCompletions>,
) -> Vec<Suggestion> {
    let position = cursor.min(char_len(buffer));
    let active = slice_chars(buffer, 0..position);

    let toggl_matches = toggl_suggestions(buffer, position, toggl);
    if !toggl_matches.is_empty() {
        return toggl_matches;
    }
    let flags = flag_suggestions(buffer, position);
    if !flags.is_empty() {
        return flags;
    }
    // With a connected integration, a `toggl ` line offers only Toggl values.
    if toggl.is_some() {
        let trimmed = active.trim_start();
        if let Some(rest) = trimmed.strip_prefix("toggl")
            && rest.starts_with(char::is_whitespace)
        {
            return Vec::new();
        }
    }

    let trimmed = active.trim_start();
    let prefix = trimmed.split_whitespace().next().unwrap_or("");
    let command_token = buffer_tokens(buffer).into_iter().next();

    let mut suggestions: Vec<Suggestion> = COMMANDS
        .iter()
        .filter_map(|command| {
            let matched = matches_prefix(command, prefix)?;
            Some(Suggestion {
                name: command.name,
                usage: command.usage.to_owned(),
                description: command.description.to_owned(),
                category: command.category.label(),
                detail: suggestion_detail(command, matched),
                aliases: command
                    .aliases
                    .iter()
                    .map(|alias| (*alias).to_owned())
                    .collect(),
                matched_alias: if matched == command.name {
                    None
                } else {
                    Some(matched.to_owned())
                },
                completion: command.name.to_owned(),
                completion_start: command_token.as_ref().map_or(position, |token| token.start),
                completion_end: command_token.as_ref().map_or(position, |token| token.end),
            })
        })
        .collect();
    suggestions.sort_by(|left, right| compare_text(left.name, right.name));
    suggestions
}

/// Apply a suggestion to the buffer, replacing only its completion range.
#[must_use]
pub fn apply_completion(buffer: &str, suggestion: &Suggestion) -> String {
    let length = char_len(buffer);
    let start = suggestion.completion_start.min(length);
    let end = suggestion.completion_end.clamp(start, length);
    format!(
        "{}{}{}",
        slice_chars(buffer, 0..start),
        suggestion.completion,
        slice_chars(buffer, end..length)
    )
}

/// The suggestion index Tab should use.
///
/// Pressing Tab on an already-complete command name cycles to the next
/// suggestion instead of reinserting the same text.
#[must_use]
pub fn next_completion_index(
    buffer: &str,
    selected_index: usize,
    suggestions: &[Suggestion],
) -> usize {
    if suggestions.is_empty() {
        return 0;
    }
    let current = selected_index.min(suggestions.len() - 1);
    if buffer.trim().is_empty() {
        return current;
    }
    let suggestion = &suggestions[current];
    if suggestion.completion_start >= char_len(buffer) {
        return current;
    }
    let parts: Vec<&str> = buffer.split_whitespace().collect();
    if parts.len() > 1 {
        return current;
    }
    let typed = parts.first().copied().unwrap_or("");
    let already_complete = typed == suggestion.name
        || suggestion
            .matched_alias
            .as_deref()
            .is_some_and(|alias| alias == typed);
    if already_complete {
        if current >= suggestions.len() - 1 {
            0
        } else {
            current + 1
        }
    } else {
        current
    }
}

#[cfg(test)]
mod tests {
    use super::{
        COMMANDS, Suggestion, TogglCompletions, TogglProjectRef, apply_completion,
        list_command_suggestions, next_completion_index,
    };

    fn names(suggestions: &[Suggestion]) -> Vec<&str> {
        suggestions.iter().map(|item| item.name).collect()
    }

    fn usages(suggestions: &[Suggestion]) -> Vec<&str> {
        suggestions.iter().map(|item| item.usage.as_str()).collect()
    }

    fn cache() -> TogglCompletions {
        TogglCompletions {
            projects: vec![
                TogglProjectRef {
                    name: "Reading books".into(),
                    client_name: Some("Personal".into()),
                },
                TogglProjectRef {
                    name: "Reading manga".into(),
                    client_name: None,
                },
            ],
            descriptions: vec!["O Nome do Vento".into(), "Choujin X".into()],
        }
    }

    #[test]
    fn an_empty_buffer_offers_every_command_sorted_by_name() {
        let suggestions = list_command_suggestions("", 0, None);
        assert_eq!(suggestions.len(), COMMANDS.len());
        let mut sorted = names(&suggestions);
        sorted.sort_unstable();
        assert_eq!(names(&suggestions), sorted);
    }

    #[test]
    fn a_prefix_filters_by_name_or_alias_and_reports_the_alias() {
        // `bookdir` is an alias of /librarydir, so a "boo" prefix reaches both.
        let suggestions = list_command_suggestions("boo", 3, None);
        assert_eq!(names(&suggestions), vec!["changebook", "librarydir"]);
        assert_eq!(suggestions[0].matched_alias.as_deref(), Some("book"));
        assert!(suggestions[0].detail.starts_with("alias /book"));

        let by_name = list_command_suggestions("mar", 3, None);
        assert_eq!(names(&by_name), vec!["mark", "marks"]);
        assert!(by_name[0].matched_alias.is_none());
    }

    #[test]
    fn completion_replaces_only_the_command_token() {
        let suggestions = list_command_suggestions("mar", 3, None);
        assert_eq!(apply_completion("mar", &suggestions[0]), "mark");

        // Text right of the cursor survives.
        let mid = list_command_suggestions("mar important", 3, None);
        assert_eq!(apply_completion("mar important", &mid[0]), "mark important");
    }

    #[test]
    fn flag_suggestions_appear_for_the_typed_command_and_skip_used_flags() {
        let suggestions = list_command_suggestions("chapters --", 11, None);
        assert_eq!(usages(&suggestions), vec!["--current", "--flat"]);
        assert_eq!(
            apply_completion("chapters --", &suggestions[0]),
            "chapters --current "
        );

        let after_one = list_command_suggestions("chapters --current --", 21, None);
        assert_eq!(usages(&after_one), vec!["--flat"]);
    }

    #[test]
    fn a_value_flag_is_labelled_as_expecting_a_value() {
        let suggestions = list_command_suggestions("changebook --s", 14, None);
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].usage, "--sort");
        assert_eq!(suggestions[0].detail, "expects a value");
        assert_eq!(suggestions[0].description, "Set sort");
    }

    #[test]
    fn toggl_actions_complete_without_a_connected_account() {
        let suggestions = list_command_suggestions("toggl s", 7, None);
        // Matching is a substring test, so "stop" qualifies too.
        assert_eq!(usages(&suggestions), vec!["setup", "sync", "start", "stop"]);
        assert_eq!(apply_completion("toggl s", &suggestions[0]), "toggl setup");
    }

    #[test]
    fn toggl_projects_complete_after_the_project_flag() {
        let buffer = "toggl start \"Reading\" --project rea";
        let suggestions = list_command_suggestions(buffer, buffer.chars().count(), Some(&cache()));
        assert_eq!(suggestions.len(), 2);
        assert_eq!(suggestions[0].description, "Personal / Reading books");
        assert_eq!(
            apply_completion(buffer, &suggestions[0]),
            "toggl start \"Reading\" --project \"Reading books\""
        );
    }

    #[test]
    fn toggl_descriptions_complete_before_any_flag() {
        let buffer = "toggl log Chou";
        let suggestions = list_command_suggestions(buffer, buffer.chars().count(), Some(&cache()));
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].description, "Recent Toggl description");
        assert_eq!(
            apply_completion(buffer, &suggestions[0]),
            "toggl log \"Choujin X\" "
        );
    }

    #[test]
    fn an_unrelated_toggl_flag_suppresses_value_completion() {
        let buffer = "toggl log \"Choujin X\" --duration 45";
        let suggestions = list_command_suggestions(buffer, buffer.chars().count(), Some(&cache()));
        assert!(suggestions.is_empty());
    }

    #[test]
    fn tab_cycles_once_the_typed_command_is_already_complete() {
        let suggestions = list_command_suggestions("mar", 3, None);
        assert_eq!(next_completion_index("mar", 0, &suggestions), 0);

        let complete = list_command_suggestions("mark", 4, None);
        assert_eq!(names(&complete), vec!["mark", "marks"]);
        assert_eq!(next_completion_index("mark", 0, &complete), 1);
        // Selection 1 is /marks, which the buffer does not spell yet, so Tab
        // completes it instead of cycling past it.
        assert_eq!(next_completion_index("mark", 1, &complete), 1);
    }

    #[test]
    fn tab_holds_still_once_arguments_are_being_typed() {
        let suggestions = list_command_suggestions("mark done", 9, None);
        assert_eq!(next_completion_index("mark done", 0, &suggestions), 0);
        assert_eq!(next_completion_index("", 3, &[]), 0);
    }
}

//! Manual pages, contextual hints, and manual styling.
//!
//! Help text is generated from the catalogue in a man-page shape. It is produced
//! as plain lines first, then optionally wrapped to a width and styled, so tests
//! can assert on the text without decoding colors.

use crate::style::{Span, Style, StyledLine};
use crate::theme::Palette;

use super::spec::{ArgSpec, COMMANDS, CommandSpec, FlagSpec, find_command};

fn format_argument(argument: &ArgSpec) -> String {
    if argument.required {
        format!("<{}>", argument.name)
    } else {
        format!("[{}]", argument.name)
    }
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

/// The man page for one command.
fn command_manual(command: &CommandSpec) -> Vec<String> {
    let mut lines = vec![
        format!("/{}(1)", command.name.to_uppercase()),
        String::new(),
        "NAME".to_owned(),
        format!("  /{} - {}", command.name, command.description),
        String::new(),
        "SYNOPSIS".to_owned(),
        format!("  {}", command.usage),
        String::new(),
    ];

    if !command.aliases.is_empty() {
        lines.push("ALIASES".to_owned());
        lines.push(format!(
            "  {}",
            command
                .aliases
                .iter()
                .map(|alias| format!("/{alias}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        lines.push(String::new());
    }
    if !command.args.is_empty() {
        lines.push("ARGUMENTS".to_owned());
        lines.extend(
            command
                .args
                .iter()
                .map(|argument| format!("  {}", format_argument(argument))),
        );
        lines.push(String::new());
    }
    if !command.flags.is_empty() {
        lines.push("FLAGS".to_owned());
        lines.extend(
            command
                .flags
                .iter()
                .map(|spec| format!("  {}", format_flag(spec))),
        );
        lines.push(String::new());
    }
    for (heading, entries) in [
        ("DESCRIPTION", command.details),
        ("EXAMPLES", command.examples),
        ("NOTES", command.notes),
    ] {
        if !entries.is_empty() {
            lines.push(heading.to_owned());
            lines.extend(entries.iter().map(|entry| format!("  {entry}")));
            lines.push(String::new());
        }
    }
    lines
}

/// Wrap one manual line, keeping its indentation on continuation lines.
fn wrap_manual_line(line: &str, width: usize) -> Vec<String> {
    if width == 0 || line.chars().count() <= width {
        return vec![line.to_owned()];
    }
    if line.trim().is_empty() {
        return vec![String::new()];
    }

    let indent: String = line
        .chars()
        .take_while(|char| char.is_whitespace())
        .collect();
    let continuation = if indent.is_empty() {
        "  ".to_owned()
    } else {
        indent.clone()
    };
    let continuation_width = continuation.chars().count();

    let mut lines: Vec<String> = Vec::new();
    let mut current = indent;
    let mut has_text = false;

    for word in line.split_whitespace() {
        let separator = usize::from(has_text);
        let word_width = word.chars().count();
        if current.chars().count() + separator + word_width <= width {
            if has_text {
                current.push(' ');
            }
            current.push_str(word);
            has_text = true;
            continue;
        }
        if has_text {
            lines.push(std::mem::take(&mut current));
            current = continuation.clone();
            has_text = false;
        }
        if continuation_width + word_width <= width {
            current = format!("{continuation}{word}");
            has_text = true;
            continue;
        }
        // A single word wider than the line is split into chunks.
        let chunk_width = width.saturating_sub(continuation_width).max(1);
        let chars: Vec<char> = word.chars().collect();
        let mut offset = 0;
        while offset < chars.len() {
            let end = (offset + chunk_width).min(chars.len());
            let chunk: String = chars[offset..end].iter().collect();
            if end == chars.len() {
                current = format!("{continuation}{chunk}");
                has_text = true;
            } else {
                lines.push(format!("{continuation}{chunk}"));
            }
            offset = end;
        }
    }
    if has_text {
        lines.push(current);
    }
    if lines.is_empty() {
        vec![line.chars().take(width).collect()]
    } else {
        lines
    }
}

fn wrap_manual(lines: Vec<String>, width: Option<usize>) -> Vec<String> {
    match width.filter(|width| *width > 0) {
        None => lines,
        Some(width) => lines
            .iter()
            .flat_map(|line| wrap_manual_line(line, width))
            .collect(),
    }
}

/// Whether a line is a man-page heading, which is rendered bold in full.
fn is_heading(line: &str) -> bool {
    if let Some(rest) = line.strip_suffix("(1)")
        && let Some(name) = rest.strip_prefix('/')
    {
        let mut chars = name.chars();
        return chars.next().is_some_and(|first| first.is_ascii_uppercase())
            && chars.all(|char| char.is_ascii_uppercase() || char.is_ascii_digit() || char == '-');
    }
    let mut chars = line.chars();
    chars.next().is_some_and(|first| first.is_ascii_uppercase())
        && line
            .chars()
            .all(|char| char.is_ascii_uppercase() || char == ' ')
}

fn is_flag_lead(char: Option<char>) -> bool {
    match char {
        None => true,
        Some(value) => value.is_whitespace() || matches!(value, '[' | '(' | '|' | ','),
    }
}

fn is_flag_tail(char: Option<char>) -> bool {
    match char {
        None => true,
        Some(value) => value.is_whitespace() || matches!(value, ',' | ']' | '|' | ')'),
    }
}

fn is_command_tail(char: Option<char>) -> bool {
    match char {
        None => true,
        Some(value) => value.is_whitespace() || matches!(value, ',' | ')'),
    }
}

/// Style one manual line: headings bold, flags in the warning color, and
/// `/command` references bold in the accent color.
#[must_use]
pub fn style_manual_line(line: &str, palette: &Palette) -> StyledLine {
    if line.is_empty() {
        return StyledLine::empty();
    }
    if is_heading(line) {
        return StyledLine::single(line, Style::new().bold());
    }

    let chars: Vec<char> = line.chars().collect();
    let flag_style = Style::fg(palette.warning);
    let command_style = Style::fg(palette.accent).bold();
    let mut styled = StyledLine::empty();
    let mut index = 0usize;

    while index < chars.len() {
        let previous = index.checked_sub(1).map(|position| chars[position]);
        let token_end = |start: usize, allow: fn(char) -> bool| {
            let mut end = start;
            while end < chars.len() && allow(chars[end]) {
                end += 1;
            }
            end
        };
        let is_flag_body =
            |char: char| char.is_ascii_lowercase() || char.is_ascii_digit() || char == '-';

        if chars[index] == '-' && is_flag_lead(previous) {
            let dashes = if chars.get(index + 1) == Some(&'-') {
                2
            } else {
                1
            };
            let body_start = index + dashes;
            let end = token_end(body_start, is_flag_body);
            let body_length = end - body_start;
            let valid = chars
                .get(body_start)
                .is_some_and(|char| char.is_ascii_lowercase())
                && ((dashes == 2 && body_length >= 1) || (dashes == 1 && body_length == 1))
                && is_flag_tail(chars.get(end).copied());
            if valid {
                styled.push(Span::new(
                    chars[index..end].iter().collect::<String>(),
                    flag_style,
                ));
                index = end;
                continue;
            }
        }

        if chars[index] == '/'
            && previous.is_none_or(char::is_whitespace)
            && chars
                .get(index + 1)
                .is_some_and(|char| char.is_ascii_lowercase())
        {
            let end = token_end(index + 1, |char| {
                char.is_ascii_lowercase() || char.is_ascii_digit()
            });
            if is_command_tail(chars.get(end).copied()) {
                styled.push(Span::new(
                    chars[index..end].iter().collect::<String>(),
                    command_style,
                ));
                index = end;
                continue;
            }
        }

        styled.push(Span::raw(chars[index].to_string()));
        index += 1;
    }
    styled
}

/// The manual for one command, or the full manual when `command_name` is `None`.
///
/// `width` wraps the output; `None` leaves lines unwrapped.
#[must_use]
pub fn command_help(command_name: Option<&str>, width: Option<usize>) -> Vec<String> {
    if let Some(name) = command_name {
        let Some(command) = find_command(name) else {
            return wrap_manual(
                vec![
                    "HELP(1)".to_owned(),
                    String::new(),
                    "No manual entry".to_owned(),
                    format!("  No help available for /{name}."),
                    String::new(),
                    "Try".to_owned(),
                    "  /help".to_owned(),
                    "  /help --all".to_owned(),
                ],
                width,
            );
        };
        return wrap_manual(command_manual(command), width);
    }

    let mut lines = vec![
        "CLI-STEALTH-READER(1)".to_owned(),
        String::new(),
        "NAME".to_owned(),
        "  /help - full command manual".to_owned(),
        String::new(),
        "SYNOPSIS".to_owned(),
        "  /help".to_owned(),
        "  /help <command>".to_owned(),
        "  /help --all".to_owned(),
        String::new(),
        "DESCRIPTION".to_owned(),
        "  Opens the complete slash-command manual. Each entry includes usage, aliases, arguments, flags, examples, and notes.".to_owned(),
        String::new(),
        "NAVIGATION".to_owned(),
        "  Scroll this page with j/k, arrow keys, Space, PageUp/PageDown, g, G, Home, and End.".to_owned(),
        "  Press Esc to close it. Use Ctrl+. or /keyboardshortcuts for key bindings.".to_owned(),
        String::new(),
        "COMMANDS".to_owned(),
    ];
    for command in COMMANDS {
        lines.push(format!("  {:<48} {}", command.usage, command.description));
    }
    lines.push(String::new());
    for command in COMMANDS {
        lines.extend(command_manual(command));
        lines.push(String::new());
    }
    wrap_manual(lines, width)
}

/// Hint lines shown under the command bar while typing.
///
/// `toggl_quota` is the formatted quota line, when the integration knows it; the
/// core has no way to fetch it itself.
#[must_use]
pub fn command_context_help(buffer: &str, toggl_quota: Option<&str>) -> Vec<String> {
    let normalized = buffer.trim().trim_start_matches('/');
    let mut tokens = normalized.split_whitespace();
    let Some(name) = tokens.next() else {
        return Vec::new();
    };
    let Some(command) = find_command(&name.to_lowercase()) else {
        return Vec::new();
    };

    if command.name == "toggl" {
        let action = tokens.next().map(str::to_lowercase);
        let quota_line = toggl_quota.map(|quota| format!("API      {quota}"));
        let with_quota = |mut lines: Vec<String>| {
            lines.extend(quota_line.clone());
            lines
        };
        match action.as_deref() {
            Some("auth") => {
                return with_quota(vec![
                    "Connect  Toggl 2.0 / Focus".to_owned(),
                    "Usage    /toggl auth <toggl_sk_...>".to_owned(),
                    "API key  https://focus.toggl.com/settings".to_owned(),
                    "Setup    if needed, the next prompt asks for your workspace URL once"
                        .to_owned(),
                    "Next     authentication syncs Toggl automatically".to_owned(),
                ]);
            }
            Some("setup") => {
                return with_quota(vec![
                    "Setup    paste your Focus workspace URL and press Enter".to_owned(),
                    "Example  https://focus.toggl.com/organizations/123/workspaces/456".to_owned(),
                    "Saved    organization is validated and remembered locally".to_owned(),
                ]);
            }
            Some("start") => {
                return with_quota(vec![
                    "Usage    /toggl start <description> [--project name]".to_owned(),
                    "Tip      type a recent description, then press Tab".to_owned(),
                    "Example  /toggl start \"Reading\" --project \"Books\"".to_owned(),
                ]);
            }
            Some("log") => {
                return with_quota(vec![
                    "Usage    /toggl log <description> --duration 25m [--project name]".to_owned(),
                    "Duration accepts 25m, 1.5h, or 900s".to_owned(),
                ]);
            }
            Some(action @ ("sync" | "recent" | "stop")) => {
                return with_quota(vec![
                    format!("Run      /toggl {action}"),
                    if action == "sync" {
                        "Fetches projects, recent entries, and the running timer".to_owned()
                    } else {
                        format!("Executes Toggl {action}")
                    },
                ]);
            }
            Some(unknown) => {
                return vec![
                    format!("Unknown  Toggl action \"{unknown}\""),
                    "Actions  auth · setup · sync · recent · start · stop · log".to_owned(),
                    "Help     /help toggl".to_owned(),
                ];
            }
            None => {}
        }
    }

    let mut lines = vec![format!("Usage    {}", command.usage)];
    if let Some(example) = command.examples.first() {
        lines.push(format!("Example  {example}"));
    }
    lines.push(format!("Help     /help {}", command.name));
    lines
}

#[cfg(test)]
mod tests {
    use super::{command_context_help, command_help, style_manual_line, wrap_manual_line};
    use crate::theme::Theme;

    #[test]
    fn a_command_manual_documents_every_section_it_has() {
        let lines = command_help(Some("search"), None);
        assert_eq!(lines[0], "/SEARCH(1)");
        assert!(lines.contains(&"SYNOPSIS".to_owned()));
        assert!(lines.contains(&"  /search [-g|--global] <term>".to_owned()));
        assert!(lines.contains(&"FLAGS".to_owned()));
        assert!(lines.contains(&"  -g, --global".to_owned()));
        assert!(lines.contains(&"ARGUMENTS".to_owned()));
        assert!(lines.contains(&"  [term]".to_owned()));
        assert!(lines.contains(&"NOTES".to_owned()));
    }

    #[test]
    fn required_arguments_and_value_flags_are_marked() {
        let goto = command_help(Some("goto"), None);
        assert!(goto.contains(&"  <position>".to_owned()));
        let library = command_help(Some("changebook"), None);
        assert!(library.contains(&"  --sort <value>".to_owned()));
        assert!(library.contains(&"ALIASES".to_owned()));
        assert!(library.contains(&"  /book".to_owned()));
    }

    #[test]
    fn an_alias_opens_the_canonical_manual() {
        assert_eq!(
            command_help(Some("book"), None),
            command_help(Some("changebook"), None)
        );
    }

    #[test]
    fn an_unknown_command_gets_a_no_entry_page() {
        let lines = command_help(Some("nope"), None);
        assert_eq!(lines[0], "HELP(1)");
        assert!(lines.contains(&"  No help available for /nope.".to_owned()));
    }

    #[test]
    fn the_full_manual_lists_every_command_then_documents_it() {
        let lines = command_help(None, None);
        assert_eq!(lines[0], "CLI-STEALTH-READER(1)");
        let summary_index = lines
            .iter()
            .position(|line| line == "COMMANDS")
            .expect("the summary section exists");
        assert!(lines[summary_index + 1].starts_with("  /prev [count]"));
        for command in crate::command::COMMANDS {
            assert!(
                lines.contains(&format!("/{}(1)", command.name.to_uppercase())),
                "/{} has no manual entry",
                command.name
            );
        }
    }

    #[test]
    fn wrapping_keeps_indentation_and_never_exceeds_the_width() {
        let lines = command_help(None, Some(40));
        for line in &lines {
            assert!(line.chars().count() <= 40, "{line:?} is too wide");
        }
        let wrapped = wrap_manual_line("  a short indent then a much longer sentence here", 20);
        assert!(wrapped.iter().skip(1).all(|line| line.starts_with("  ")));
    }

    #[test]
    fn an_unindented_long_line_gains_a_two_space_continuation() {
        let wrapped = wrap_manual_line("alpha beta gamma delta epsilon", 12);
        assert_eq!(wrapped[0], "alpha beta");
        assert!(wrapped[1].starts_with("  "));
    }

    #[test]
    fn a_word_wider_than_the_line_is_split_into_chunks() {
        let wrapped = wrap_manual_line("  supercalifragilisticexpialidocious", 12);
        assert!(wrapped.len() > 1);
        assert!(wrapped.iter().all(|line| line.chars().count() <= 12));
        let rejoined: String = wrapped.iter().map(|line| line.trim_start()).collect();
        assert_eq!(rejoined, "supercalifragilisticexpialidocious");
    }

    #[test]
    fn context_help_shows_usage_example_and_help_pointer() {
        assert_eq!(
            command_context_help("/mode", None),
            vec![
                "Usage    /mode [plain|typescript|python|rust]",
                "Example  /mode plain",
                "Help     /help mode",
            ]
        );
    }

    #[test]
    fn context_help_resolves_aliases_and_ignores_unknown_commands() {
        assert_eq!(
            command_context_help("book dune", None)[2],
            "Help     /help changebook"
        );
        assert!(command_context_help("/nope", None).is_empty());
        assert!(command_context_help("   ", None).is_empty());
    }

    #[test]
    fn toggl_context_help_is_action_specific_and_can_carry_quota() {
        let auth = command_context_help("/toggl auth", Some("28/30 · resets in 12m"));
        assert_eq!(auth[0], "Connect  Toggl 2.0 / Focus");
        assert_eq!(
            auth.last().expect("quota line"),
            "API      28/30 · resets in 12m"
        );

        let sync = command_context_help("/toggl sync", None);
        assert_eq!(sync[0], "Run      /toggl sync");
        assert_eq!(
            sync[1],
            "Fetches projects, recent entries, and the running timer"
        );

        let stop = command_context_help("/toggl stop", None);
        assert_eq!(stop[1], "Executes Toggl stop");

        let unknown = command_context_help("/toggl frobnicate", Some("ignored"));
        assert_eq!(unknown[0], "Unknown  Toggl action \"frobnicate\"");
        assert_eq!(unknown.len(), 3, "an unknown action never shows quota");

        // Without an action, toggl falls back to the generic hint.
        assert_eq!(
            command_context_help("/toggl", None)[0],
            "Usage    /toggl auth|setup|sync|recent|start|stop|log [description] [--project name] [--duration 25m]"
        );
    }

    #[test]
    fn manual_styling_marks_headings_flags_and_command_references() {
        let palette = Theme::default().palette;

        let heading = style_manual_line("SYNOPSIS", &palette);
        assert_eq!(heading.spans.len(), 1);
        assert!(heading.spans[0].style.bold);

        let title = style_manual_line("/SEARCH(1)", &palette);
        assert!(title.spans[0].style.bold);

        let usage = style_manual_line("  /search [-g|--global] <term>", &palette);
        assert_eq!(usage.text(), "  /search [-g|--global] <term>");
        let flags: Vec<&str> = usage
            .spans
            .iter()
            .filter(|span| span.style == crate::style::Style::fg(palette.warning))
            .map(|span| span.text.as_str())
            .collect();
        assert_eq!(flags, vec!["-g", "--global"]);
        let commands: Vec<&str> = usage
            .spans
            .iter()
            .filter(|span| span.style == crate::style::Style::fg(palette.accent).bold())
            .map(|span| span.text.as_str())
            .collect();
        assert_eq!(commands, vec!["/search"]);
    }

    #[test]
    fn manual_styling_leaves_prose_and_paths_alone() {
        let palette = Theme::default().palette;
        let line = style_manual_line("  A well-known path is ./books/example.epub", &palette);
        assert_eq!(line.text(), "  A well-known path is ./books/example.epub");
        assert!(
            line.spans
                .iter()
                .all(|span| span.style == crate::style::Style::new()),
            "prose should stay unstyled: {:?}",
            line.spans
        );
    }
}

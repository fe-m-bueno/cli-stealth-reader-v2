//! Tokenizing and parsing slash commands.

use std::collections::BTreeMap;

use super::spec::{CommandSpec, find_command};

/// Split a command line into tokens, honoring single and double quotes.
///
/// Quotes group whitespace and are removed from the token; an unterminated quote
/// simply runs to the end of the input.
#[must_use]
pub fn tokenize(input: &str) -> Vec<String> {
    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;

    for char in input.chars() {
        if let Some(active) = quote {
            if char == active {
                quote = None;
            } else {
                current.push(char);
            }
            continue;
        }
        match char {
            '"' | '\'' => quote = Some(char),
            _ if char.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(char),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// A parsed flag: present, or present with a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlagValue {
    Set,
    Value(String),
}

impl FlagValue {
    /// The value, for flags declared with `takes_value`.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Set => None,
            Self::Value(value) => Some(value),
        }
    }
}

/// A command line resolved against the catalogue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCommand {
    /// Canonical command name, with aliases already resolved.
    pub name: &'static str,
    pub args: Vec<String>,
    pub flags: BTreeMap<String, FlagValue>,
}

impl ParsedCommand {
    #[must_use]
    pub fn has_flag(&self, name: &str) -> bool {
        self.flags.contains_key(name)
    }

    #[must_use]
    pub fn flag_value(&self, name: &str) -> Option<&str> {
        self.flags.get(name).and_then(FlagValue::as_str)
    }

    /// The positional arguments joined back into one string, as commands that
    /// take free text (`/mark`, `/note`, `/search`) need.
    #[must_use]
    pub fn joined_args(&self) -> String {
        self.args.join(" ")
    }
}

/// Why a command line could not be parsed. Messages match v1 so status-line text
/// stays the same.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    MissingSlash,
    UnknownCommand(String),
    UnknownFlag { command: &'static str, flag: String },
    UnknownShortFlag { command: &'static str, flag: char },
    FlagNeedsValue(String),
    ShortFlagNeedsLongForm { flag: char, name: &'static str },
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingSlash => write!(formatter, "Command must start with /"),
            Self::UnknownCommand(name) => write!(formatter, "Unknown command: {name}"),
            Self::UnknownFlag { command, flag } => {
                write!(formatter, "Unknown flag --{flag} for /{command}")
            }
            Self::UnknownShortFlag { command, flag } => {
                write!(formatter, "Unknown flag -{flag} for /{command}")
            }
            Self::FlagNeedsValue(flag) => write!(formatter, "Flag --{flag} expects a value"),
            Self::ShortFlagNeedsLongForm { flag, name } => {
                write!(formatter, "Flag -{flag} must be written as --{name}=...")
            }
        }
    }
}

impl std::error::Error for CommandError {}

/// Parse a slash command line.
///
/// Long flags accept `--flag`, `--flag value`, and `--flag=value`. Short flags
/// bundle (`-gd`), never take a value, and resolve through their long name.
pub fn parse_slash_command(input: &str) -> Result<ParsedCommand, CommandError> {
    let tokens = tokenize(input.trim());
    let Some(first) = tokens.first() else {
        return Err(CommandError::MissingSlash);
    };
    let Some(name) = first.strip_prefix('/') else {
        return Err(CommandError::MissingSlash);
    };
    let spec: &CommandSpec =
        find_command(name).ok_or_else(|| CommandError::UnknownCommand(name.to_owned()))?;

    let mut args: Vec<String> = Vec::new();
    let mut flags: BTreeMap<String, FlagValue> = BTreeMap::new();
    let mut index = 1usize;

    while index < tokens.len() {
        let token = &tokens[index];
        if let Some(raw) = token.strip_prefix("--") {
            let (flag_name, inline_value) = match raw.split_once('=') {
                Some((name, value)) => (name, Some(value.to_owned())),
                None => (raw, None),
            };
            let flag_spec = spec.flag(flag_name).ok_or(CommandError::UnknownFlag {
                command: spec.name,
                flag: flag_name.to_owned(),
            })?;
            if flag_spec.takes_value {
                let (value, consumed_next) = match inline_value {
                    Some(value) => (Some(value), false),
                    None => (tokens.get(index + 1).cloned(), true),
                };
                let value = value
                    .filter(|value| !value.is_empty() && !value.starts_with("--"))
                    .ok_or_else(|| CommandError::FlagNeedsValue(flag_name.to_owned()))?;
                flags.insert(flag_name.to_owned(), FlagValue::Value(value));
                if consumed_next {
                    index += 1;
                }
            } else {
                flags.insert(flag_name.to_owned(), FlagValue::Set);
            }
        } else if token.len() > 1 && token.starts_with('-') {
            for short in token.chars().skip(1) {
                let flag_spec =
                    spec.flag_by_alias(short)
                        .ok_or(CommandError::UnknownShortFlag {
                            command: spec.name,
                            flag: short,
                        })?;
                if flag_spec.takes_value {
                    return Err(CommandError::ShortFlagNeedsLongForm {
                        flag: short,
                        name: flag_spec.name,
                    });
                }
                flags.insert(flag_spec.name.to_owned(), FlagValue::Set);
            }
        } else {
            args.push(token.clone());
        }
        index += 1;
    }

    Ok(ParsedCommand {
        name: spec.name,
        args,
        flags,
    })
}

#[cfg(test)]
mod tests {
    use super::{CommandError, FlagValue, parse_slash_command, tokenize};

    #[test]
    fn tokenizing_groups_quoted_text_and_drops_the_quotes() {
        assert_eq!(
            tokenize(r#"/search "chapter one" --global"#),
            vec!["/search", "chapter one", "--global"]
        );
        assert_eq!(tokenize("/mark 'a label'"), vec!["/mark", "a label"]);
        assert_eq!(tokenize("   /next   3  "), vec!["/next", "3"]);
        assert_eq!(tokenize(""), Vec::<String>::new());
    }

    #[test]
    fn an_unterminated_quote_runs_to_the_end() {
        assert_eq!(
            tokenize(r#"/mark "open ended"#),
            vec!["/mark", "open ended"]
        );
    }

    #[test]
    fn a_command_must_start_with_a_slash() {
        assert_eq!(parse_slash_command("next"), Err(CommandError::MissingSlash));
        assert_eq!(parse_slash_command(""), Err(CommandError::MissingSlash));
    }

    #[test]
    fn aliases_resolve_to_the_canonical_name() {
        assert_eq!(
            parse_slash_command("/book dune").expect("valid").name,
            "changebook"
        );
        assert_eq!(
            parse_slash_command("/keys").expect("valid").name,
            "keyboardshortcuts"
        );
        assert_eq!(
            parse_slash_command("/config").expect("valid").name,
            "settings"
        );
        assert_eq!(
            parse_slash_command("/bookdir").expect("valid").name,
            "librarydir"
        );
    }

    #[test]
    fn unknown_commands_and_flags_are_reported_with_v1_wording() {
        assert_eq!(
            parse_slash_command("/nope").unwrap_err().to_string(),
            "Unknown command: nope"
        );
        assert_eq!(
            parse_slash_command("/next --sideways")
                .unwrap_err()
                .to_string(),
            "Unknown flag --sideways for /next"
        );
        assert_eq!(
            parse_slash_command("/search -z term")
                .unwrap_err()
                .to_string(),
            "Unknown flag -z for /search"
        );
    }

    #[test]
    fn value_flags_accept_both_spellings_and_reject_a_missing_value() {
        let separate = parse_slash_command("/changebook --sort progress").expect("valid");
        assert_eq!(separate.flag_value("sort"), Some("progress"));
        assert!(separate.args.is_empty());

        let inline = parse_slash_command("/changebook --sort=title dune").expect("valid");
        assert_eq!(inline.flag_value("sort"), Some("title"));
        assert_eq!(inline.args, vec!["dune"]);

        assert_eq!(
            parse_slash_command("/changebook --sort")
                .unwrap_err()
                .to_string(),
            "Flag --sort expects a value"
        );
        assert_eq!(
            parse_slash_command("/changebook --sort --recent")
                .unwrap_err()
                .to_string(),
            "Flag --sort expects a value"
        );
    }

    #[test]
    fn short_flags_bundle_and_map_to_their_long_names() {
        let parsed = parse_slash_command("/note -ld").expect("valid");
        assert!(parsed.has_flag("list"));
        assert!(parsed.has_flag("delete"));
        assert_eq!(parsed.flags.get("list"), Some(&FlagValue::Set));

        let search = parse_slash_command("/search -g mordor").expect("valid");
        assert!(search.has_flag("global"));
        assert_eq!(search.args, vec!["mordor"]);
    }

    #[test]
    fn a_lone_dash_is_an_argument_not_a_flag() {
        let parsed = parse_slash_command("/mark -").expect("valid");
        assert_eq!(parsed.args, vec!["-"]);
        assert!(parsed.flags.is_empty());
    }

    #[test]
    fn free_text_arguments_can_be_rejoined() {
        let parsed = parse_slash_command("/note remember this passage").expect("valid");
        assert_eq!(parsed.joined_args(), "remember this passage");
    }
}

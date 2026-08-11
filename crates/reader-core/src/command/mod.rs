//! Slash commands: catalogue, parsing, help, and completion.

pub mod help;
pub mod parse;
pub mod spec;
pub mod suggest;

pub use help::{command_context_help, command_help, style_manual_line};
pub use parse::{CommandError, FlagValue, ParsedCommand, parse_slash_command, tokenize};
pub use spec::{ArgSpec, COMMANDS, Category, CommandSpec, FlagSpec, find_command};
pub use suggest::{
    Suggestion, TogglCompletions, TogglProjectRef, apply_completion, list_command_suggestions,
    next_completion_index,
};

//! Cross-language command parity.
//!
//! `tests/golden/command-parity.json` is produced from the TypeScript v1 by
//! `tools/generate-command-golden.mjs`. It pins parse results and error wording,
//! contextual hints, manual text at several widths, and suggestion lists with
//! their completion ranges — everything the command bar shows or acts on.

use std::collections::BTreeMap;

use reader_core::command::{
    apply_completion, command_context_help, command_help, list_command_suggestions,
    next_completion_index, parse_slash_command,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Golden {
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Case {
    Parse {
        input: String,
        result: ParseResult,
    },
    Context {
        input: String,
        lines: Vec<String>,
    },
    Help {
        command: Option<String>,
        width: usize,
        lines: Vec<String>,
    },
    Suggest {
        buffer: String,
        cursor: usize,
        suggestions: Vec<GoldenSuggestion>,
        #[serde(rename = "nextIndex")]
        next_index: Vec<usize>,
    },
}

#[derive(Debug, Deserialize)]
struct ParseResult {
    ok: bool,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    /// v1 stored flags as `true` or a string value.
    #[serde(default)]
    flags: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoldenSuggestion {
    name: String,
    usage: String,
    description: String,
    category: String,
    detail: String,
    matched_alias: Option<String>,
    completion: Option<String>,
    completion_start: Option<usize>,
    completion_end: Option<usize>,
    applied: String,
}

fn check_parse(input: &str, expected: &ParseResult, failures: &mut Vec<String>) {
    match (parse_slash_command(input), expected.ok) {
        (Ok(parsed), true) => {
            let expected_name = expected.name.as_deref().unwrap_or_default();
            if parsed.name != expected_name {
                failures.push(format!(
                    "parse {input:?}: name {} != {expected_name}",
                    parsed.name
                ));
            }
            if parsed.args != expected.args {
                failures.push(format!(
                    "parse {input:?}: args {:?} != {:?}",
                    parsed.args, expected.args
                ));
            }
            let actual: BTreeMap<String, serde_json::Value> = parsed
                .flags
                .iter()
                .map(|(name, value)| {
                    let json = match value.as_str() {
                        Some(text) => serde_json::Value::String(text.to_owned()),
                        None => serde_json::Value::Bool(true),
                    };
                    (name.clone(), json)
                })
                .collect();
            if actual != expected.flags {
                failures.push(format!(
                    "parse {input:?}: flags {actual:?} != {:?}",
                    expected.flags
                ));
            }
        }
        (Err(error), false) => {
            let expected_message = expected.message.as_deref().unwrap_or_default();
            if error.to_string() != expected_message {
                failures.push(format!(
                    "parse {input:?}: message {:?} != {expected_message:?}",
                    error.to_string()
                ));
            }
        }
        (Ok(parsed), false) => failures.push(format!(
            "parse {input:?}: v2 accepted it as /{} but v1 failed with {:?}",
            parsed.name, expected.message
        )),
        (Err(error), true) => failures.push(format!(
            "parse {input:?}: v2 rejected it with {:?} but v1 accepted it",
            error.to_string()
        )),
    }
}

fn check_suggest(
    buffer: &str,
    cursor: usize,
    expected: &[GoldenSuggestion],
    next_index: &[usize],
    failures: &mut Vec<String>,
) {
    let suggestions = list_command_suggestions(buffer, cursor, None);
    if suggestions.len() != expected.len() {
        failures.push(format!(
            "suggest {buffer:?}@{cursor}: {} suggestions != {}\n  v1: {:?}\n  v2: {:?}",
            suggestions.len(),
            expected.len(),
            expected.iter().map(|item| &item.usage).collect::<Vec<_>>(),
            suggestions
                .iter()
                .map(|item| &item.usage)
                .collect::<Vec<_>>()
        ));
        return;
    }
    for (actual, golden) in suggestions.iter().zip(expected) {
        let mismatch = actual.name != golden.name
            || actual.usage != golden.usage
            || actual.description != golden.description
            || actual.category != golden.category
            || actual.detail != golden.detail
            || actual.matched_alias != golden.matched_alias
            || Some(&actual.completion) != golden.completion.as_ref()
            || Some(actual.completion_start) != golden.completion_start
            || Some(actual.completion_end) != golden.completion_end;
        if mismatch {
            failures.push(format!(
                "suggest {buffer:?}@{cursor}: {:?} differs\n  v1: {golden:?}\n  v2: {actual:?}",
                golden.usage
            ));
            continue;
        }
        let applied = apply_completion(buffer, actual);
        if applied != golden.applied {
            failures.push(format!(
                "suggest {buffer:?}@{cursor}: applying {:?} gave {applied:?}, v1 gave {:?}",
                golden.usage, golden.applied
            ));
        }
    }
    for (offset, expected_index) in [0usize, 1, 5].iter().zip(next_index) {
        let actual = next_completion_index(buffer, *offset, &suggestions);
        if actual != *expected_index {
            failures.push(format!(
                "suggest {buffer:?}@{cursor}: next index from {offset} was {actual}, v1 gave {expected_index}"
            ));
        }
    }
}

#[test]
fn commands_match_the_v1_golden_behavior() {
    let raw = include_str!("golden/command-parity.json");
    let golden: Golden = serde_json::from_str(raw).expect("golden fixture should parse");
    assert!(
        golden.cases.len() > 200,
        "the golden fixture lost coverage: {} cases",
        golden.cases.len()
    );

    let mut failures: Vec<String> = Vec::new();
    for case in &golden.cases {
        match case {
            Case::Parse { input, result } => check_parse(input, result, &mut failures),
            Case::Context { input, lines } => {
                let actual = command_context_help(input, None);
                if &actual != lines {
                    failures.push(format!(
                        "context {input:?}:\n  v1: {lines:?}\n  v2: {actual:?}"
                    ));
                }
            }
            Case::Help {
                command,
                width,
                lines,
            } => {
                let actual = command_help(
                    command.as_deref(),
                    if *width == 0 { None } else { Some(*width) },
                );
                if &actual != lines {
                    let first_difference = actual
                        .iter()
                        .zip(lines)
                        .position(|(left, right)| left != right)
                        .unwrap_or(lines.len().min(actual.len()));
                    failures.push(format!(
                        "help {command:?}@{width}: first difference at line {first_difference}\n  v1: {:?}\n  v2: {:?}",
                        lines.get(first_difference),
                        actual.get(first_difference)
                    ));
                }
            }
            Case::Suggest {
                buffer,
                cursor,
                suggestions,
                next_index,
            } => check_suggest(buffer, *cursor, suggestions, next_index, &mut failures),
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} command cases differ from v1:\n{}",
        failures.len(),
        golden.cases.len(),
        failures
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

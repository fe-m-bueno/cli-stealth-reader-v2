//! Python disguise.
//!
//! Python has no density control in v1: every line picks from one pool of twelve
//! statement patterns. Indentation carries the structure, so an opener always
//! indents its body by four spaces and never emits a closing line.

use crate::book::CanonicalBlock;
use crate::style::StyledLine;
use crate::theme::Palette;

use super::super::text::{
    esc, extract_words, line_hash, to_snake_func_name, to_snake_name, to_type_name, wrap_text,
};
use super::{LineBuilder, Role, block_text, comment_line, render_structural};

/// Worst case is the dict assignment: name (13) + `["key"] = ` (11) + `""` (2),
/// plus slack for escaped quotes.
const TEXT_OVERHEAD: usize = 32;

type Pattern = fn(&str, &[&str], usize, &Palette) -> StyledLine;

fn literal(line: &str) -> String {
    format!("\"{}\"", esc(line))
}

fn pat_assign(line: &str, words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Ident, to_snake_name(words, seed, ""))
        .push(Role::Operator, " = ")
        .push(Role::Literal, literal(line));
    builder.build()
}

fn pat_typed_assign(line: &str, words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    const TYPES: [&str; 5] = ["str", "int", "bool", "list[str]", "dict"];
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Ident, to_snake_name(words, seed, ""))
        .push(Role::Operator, ": ")
        .push(Role::Type, TYPES[seed % TYPES.len()])
        .push(Role::Operator, " = ")
        .push(Role::Literal, literal(line));
    builder.build()
}

fn pat_comment(line: &str, _words: &[&str], _seed: usize, palette: &Palette) -> StyledLine {
    comment_line(palette, format!("# {line}"))
}

fn pat_print(line: &str, _words: &[&str], _seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Function, "print")
        .push(Role::Operator, "(")
        .push(Role::Literal, format!("f\"{}\"", esc(line)))
        .push(Role::Operator, ")");
    builder.build()
}

fn pat_plain_print(line: &str, _words: &[&str], _seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Function, "print")
        .push(Role::Operator, "(")
        .push(Role::Literal, literal(line))
        .push(Role::Operator, ")");
    builder.build()
}

fn pat_return(line: &str, _words: &[&str], _seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Keyword, "return")
        .space()
        .push(Role::Literal, literal(line));
    builder.build()
}

fn pat_raise(line: &str, _words: &[&str], _seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Keyword, "raise")
        .space()
        .push(Role::Type, "ValueError")
        .push(Role::Operator, "(")
        .push(Role::Literal, literal(line))
        .push(Role::Operator, ")");
    builder.build()
}

fn pat_func_call(line: &str, words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Ident, to_snake_name(words, seed, ""))
        .push(Role::Operator, " = ")
        .push(Role::Function, to_snake_func_name(words, seed + 1))
        .push(Role::Operator, "(")
        .push(Role::Literal, literal(line))
        .push(Role::Operator, ")");
    builder.build()
}

fn pat_logging(line: &str, _words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    const LEVELS: [&str; 4] = ["info", "debug", "warning", "error"];
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Ident, "logging")
        .push(Role::Operator, ".")
        .push(Role::Function, LEVELS[seed % LEVELS.len()])
        .push(Role::Operator, "(")
        .push(Role::Literal, literal(line))
        .push(Role::Operator, ")");
    builder.build()
}

fn pat_f_string(line: &str, words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Ident, to_snake_name(words, seed, ""))
        .push(Role::Operator, " = ")
        .push(Role::Ident, "f")
        .push(Role::Literal, literal(line));
    builder.build()
}

fn pat_dict_assign(line: &str, words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    let key: String = to_snake_name(words, seed + 2, "").chars().take(6).collect();
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Ident, to_snake_name(words, seed, ""))
        .push(Role::Operator, format!("[\"{key}\"] = "))
        .push(Role::Literal, literal(line));
    builder.build()
}

fn pat_assert(line: &str, _words: &[&str], _seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Keyword, "assert")
        .space()
        .push(Role::Ident, "result")
        .push(Role::Operator, ", ")
        .push(Role::Literal, literal(line));
    builder.build()
}

const LINE_PATTERNS: [Pattern; 12] = [
    pat_assign,
    pat_comment,
    pat_print,
    pat_typed_assign,
    pat_return,
    pat_plain_print,
    pat_func_call,
    pat_raise,
    pat_logging,
    pat_f_string,
    pat_dict_assign,
    pat_assert,
];

fn disguise_line(
    line: &str,
    block_index: usize,
    line_index: usize,
    palette: &Palette,
) -> StyledLine {
    let words = extract_words(line);
    let seed = line_hash(block_index, line_index);
    LINE_PATTERNS[seed % LINE_PATTERNS.len()](line, &words, seed, palette)
}

fn indented_body(
    wrapped: &[String],
    block_index: usize,
    line_offset: usize,
    indent: &str,
    palette: &Palette,
) -> Vec<StyledLine> {
    wrapped
        .iter()
        .enumerate()
        .map(|(offset, line)| {
            let mut builder = LineBuilder::indented(palette, indent);
            builder.extend(&disguise_line(
                line,
                block_index,
                line_offset + offset,
                palette,
            ));
            builder.build()
        })
        .collect()
}

const CONDITIONS: [&str; 5] = ["is_valid", "flag", "active", "ready", "loaded"];

fn def_open(words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Keyword, "def")
        .space()
        .push(Role::Function, to_snake_func_name(words, seed))
        .push(Role::Operator, "(self) -> ")
        .push(Role::Type, "str")
        .push(Role::Operator, ":");
    builder.build()
}

fn class_open(words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    const BASES: [&str; 4] = ["BaseModel", "Exception", "Enum", "Protocol"];
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Keyword, "class")
        .space()
        .push(Role::Type, to_type_name(words, seed, ""))
        .push(Role::Operator, "(")
        .push(Role::Type, BASES[seed % BASES.len()])
        .push(Role::Operator, "):");
    builder.build()
}

fn with_open(words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Keyword, "with")
        .space()
        .push(Role::Function, to_snake_func_name(words, seed))
        .push(Role::Operator, "(")
        .push(Role::Literal, "\"data\"")
        .push(Role::Operator, ")")
        .space()
        .push(Role::Keyword, "as")
        .space()
        .push(Role::Ident, "f")
        .push(Role::Operator, ":");
    builder.build()
}

fn if_open(seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Keyword, "if")
        .space()
        .push(Role::Ident, CONDITIONS[seed % CONDITIONS.len()])
        .push(Role::Operator, ":");
    builder.build()
}

pub(super) fn render(
    block: &CanonicalBlock,
    width: usize,
    palette: &Palette,
    block_index: usize,
) -> Vec<StyledLine> {
    if let Some(lines) = render_structural(block, palette, "#", "# · · · · ·") {
        return lines;
    }
    let Some(text) = block_text(block) else {
        return Vec::new();
    };

    let words = extract_words(text);
    let seed = line_hash(block_index, 0);
    let text_width = width.saturating_sub(TEXT_OVERHEAD).max(20);

    if block_index % 41 == 0 {
        let wrapped = wrap_text(text, text_width);
        let half = wrapped.len().div_ceil(2);
        let mut otherwise = LineBuilder::new(palette);
        otherwise
            .push(Role::Keyword, "else")
            .push(Role::Operator, ":");

        let mut lines = vec![if_open(seed, palette)];
        lines.extend(indented_body(
            &wrapped[..half],
            block_index,
            0,
            "    ",
            palette,
        ));
        lines.push(otherwise.build());
        lines.extend(indented_body(
            &wrapped[half..],
            block_index,
            half,
            "    ",
            palette,
        ));
        return lines;
    }

    if block_index % 43 == 0 {
        let wrapped = wrap_text(text, text_width);
        let half = wrapped.len().div_ceil(2);
        let mut open = LineBuilder::new(palette);
        open.push(Role::Keyword, "try").push(Role::Operator, ":");
        let mut except = LineBuilder::new(palette);
        except
            .push(Role::Keyword, "except")
            .space()
            .push(Role::Type, "Exception")
            .space()
            .push(Role::Keyword, "as")
            .space()
            .push(Role::Ident, "e")
            .push(Role::Operator, ":");

        let mut lines = vec![open.build()];
        lines.extend(indented_body(
            &wrapped[..half],
            block_index,
            0,
            "    ",
            palette,
        ));
        lines.push(except.build());
        lines.extend(indented_body(
            &wrapped[half..],
            block_index,
            half,
            "    ",
            palette,
        ));
        return lines;
    }

    let mut structural: Vec<StyledLine> = Vec::new();
    if block_index % 13 == 0 {
        structural.push(def_open(&words, seed, palette));
    } else if block_index % 17 == 0 {
        structural.push(class_open(&words, seed, palette));
    } else if block_index % 23 == 0 {
        structural.push(with_open(&words, seed, palette));
    } else if block_index % 29 == 0 {
        structural.push(if_open(seed, palette));
    }

    let indent = if structural.is_empty() { "" } else { "    " };
    let wrapped = wrap_text(text, text_width);
    let mut lines = structural;
    lines.extend(indented_body(&wrapped, block_index, 0, indent, palette));
    lines
}

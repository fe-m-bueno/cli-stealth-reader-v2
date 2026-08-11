//! Rust disguise.
//!
//! Thirteen statement patterns, four structural openers that always close with a
//! brace, and one block shape (the derive + struct definition) that replaces the
//! prose body entirely.

use crate::book::CanonicalBlock;
use crate::style::StyledLine;
use crate::theme::Palette;

use super::super::text::{
    esc, extract_words, line_hash, to_snake_func_name, to_snake_name, to_type_name, wrap_text,
    wrapped_line_count,
};
use super::{LineBuilder, Role, block_text, comment_line, render_structural};

/// Worst case is the typed `let mut`: `let mut ` (8) + name (13) +
/// `: Vec<String> = ` (16) + `""` (2) + `;` (1), plus slack for escapes.
const TEXT_OVERHEAD: usize = 44;

pub(super) fn line_count(block: &CanonicalBlock, width: usize, block_index: usize) -> usize {
    if block_text(block).is_none() {
        return 1;
    }
    let wrapped = wrapped_line_count(block.text(), width.saturating_sub(TEXT_OVERHEAD).max(20));
    if block_index % 41 == 0 {
        return wrapped + 3;
    }
    if block_index % 19 == 0 {
        return 5;
    }
    let has_scope = block_index % 13 == 0
        || block_index % 17 == 0
        || block_index % 23 == 0
        || block_index % 29 == 0;
    wrapped + usize::from(has_scope) * 2
}

type Pattern = fn(&str, &[&str], usize, &Palette) -> StyledLine;

fn literal(line: &str) -> String {
    format!("\"{}\"", esc(line))
}

fn pat_let(line: &str, words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Keyword, "let")
        .space()
        .push(Role::Ident, to_snake_name(words, seed, ""))
        .push(Role::Operator, " = ")
        .push(Role::Literal, literal(line))
        .push(Role::Operator, ";");
    builder.build()
}

fn pat_let_mut(line: &str, words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    const TYPES: [&str; 5] = ["&str", "String", "&[u8]", "i32", "usize"];
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Keyword, "let")
        .space()
        .push(Role::Keyword, "mut")
        .space()
        .push(Role::Ident, to_snake_name(words, seed, ""))
        .push(Role::Operator, ": ")
        .push(Role::Type, TYPES[seed % TYPES.len()])
        .push(Role::Operator, " = ")
        .push(Role::Literal, literal(line))
        .push(Role::Operator, ";");
    builder.build()
}

fn pat_comment(line: &str, _words: &[&str], _seed: usize, palette: &Palette) -> StyledLine {
    comment_line(palette, format!("// {line}"))
}

fn macro_call(name: &str, line: &str, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Function, name)
        .push(Role::Operator, "(")
        .push(Role::Literal, literal(line))
        .push(Role::Operator, ");");
    builder.build()
}

fn pat_println(line: &str, _words: &[&str], _seed: usize, palette: &Palette) -> StyledLine {
    macro_call("println!", line, palette)
}

fn pat_eprintln(line: &str, _words: &[&str], _seed: usize, palette: &Palette) -> StyledLine {
    macro_call("eprintln!", line, palette)
}

fn pat_ok(line: &str, _words: &[&str], _seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Type, "Ok")
        .push(Role::Operator, "(")
        .push(Role::Literal, literal(line))
        .push(Role::Operator, ")");
    builder.build()
}

fn pat_err(line: &str, _words: &[&str], _seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Type, "Err")
        .push(Role::Operator, "(")
        .push(Role::Literal, literal(line))
        .push(Role::Operator, ")?");
    builder.build()
}

fn pat_let_discard(line: &str, words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Keyword, "let")
        .space()
        .push(Role::Ident, "_")
        .push(Role::Operator, " = ")
        .push(Role::Function, to_snake_func_name(words, seed))
        .push(Role::Operator, "(")
        .push(Role::Literal, literal(line))
        .push(Role::Operator, ");");
    builder.build()
}

fn pat_assert_eq(line: &str, _words: &[&str], _seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Function, "assert_eq!")
        .push(Role::Operator, "(result, ")
        .push(Role::Literal, literal(line))
        .push(Role::Operator, ");");
    builder.build()
}

fn pat_format(line: &str, _words: &[&str], _seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Function, "format!")
        .push(Role::Operator, "(\"{}\", ")
        .push(Role::Literal, literal(line))
        .push(Role::Operator, ")");
    builder.build()
}

fn pat_push(line: &str, words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Ident, to_snake_name(words, seed, ""))
        .push(Role::Operator, ".")
        .push(Role::Function, "push")
        .push(Role::Operator, "(")
        .push(Role::Literal, literal(line))
        .push(Role::Operator, ");");
    builder.build()
}

fn pat_info(line: &str, _words: &[&str], _seed: usize, palette: &Palette) -> StyledLine {
    macro_call("info!", line, palette)
}

fn pat_expect(line: &str, words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Function, to_snake_func_name(words, seed))
        .push(Role::Operator, "(")
        .push(Role::Literal, literal(line))
        .push(Role::Operator, ")")
        .push(Role::Operator, ".")
        .push(Role::Function, "expect")
        .push(Role::Operator, "(")
        .push(Role::Literal, "\"failed\"")
        .push(Role::Operator, ")");
    builder.build()
}

const LINE_PATTERNS: [Pattern; 13] = [
    pat_let,
    pat_comment,
    pat_println,
    pat_let_mut,
    pat_ok,
    pat_err,
    pat_let_discard,
    pat_eprintln,
    pat_assert_eq,
    pat_format,
    pat_push,
    pat_info,
    pat_expect,
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
            builder.extend(disguise_line(
                line,
                block_index,
                line_offset + offset,
                palette,
            ));
            builder.build()
        })
        .collect()
}

fn fn_open(words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Keyword, "fn")
        .space()
        .push(Role::Function, to_snake_func_name(words, seed))
        .push(Role::Operator, "() -> ")
        .push(Role::Type, "&'static str")
        .push(Role::Operator, " {");
    builder.build()
}

fn pub_fn_open(words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    const RETURNS: [&str; 3] = ["String", "Result<(), Box<dyn Error>>", "Option<String>"];
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Keyword, "pub fn")
        .space()
        .push(Role::Function, to_snake_func_name(words, seed))
        .push(Role::Operator, "() -> ")
        .push(Role::Type, RETURNS[seed % RETURNS.len()])
        .push(Role::Operator, " {");
    builder.build()
}

fn impl_open(words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Keyword, "impl")
        .space()
        .push(Role::Type, to_type_name(words, seed, ""))
        .push(Role::Operator, " {");
    builder.build()
}

fn match_open(words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Keyword, "match")
        .space()
        .push(Role::Ident, to_snake_name(words, seed, ""))
        .push(Role::Operator, " {");
    builder.build()
}

fn struct_lines(words: &[&str], seed: usize, palette: &Palette) -> Vec<StyledLine> {
    const TYPES: [&str; 4] = ["String", "u32", "bool", "Vec<String>"];

    let mut derive = LineBuilder::new(palette);
    derive.push(Role::Operator, "#[derive(Debug, Clone)]");

    let mut open = LineBuilder::new(palette);
    open.push(Role::Keyword, "struct")
        .space()
        .push(Role::Type, to_type_name(words, seed, ""))
        .push(Role::Operator, " {");

    let mut first = LineBuilder::indented(palette, "    ");
    first
        .push(Role::Dim, to_snake_name(words, seed + 1, ""))
        .push(Role::Operator, ": ")
        .push(Role::Type, TYPES[seed % 4])
        .push(Role::Operator, ",");

    let mut second = LineBuilder::indented(palette, "    ");
    second
        .push(Role::Dim, to_snake_name(words, seed + 2, "id"))
        .push(Role::Operator, ": ")
        .push(Role::Type, TYPES[(seed + 1) % 4])
        .push(Role::Operator, ",");

    let mut close = LineBuilder::new(palette);
    close.push(Role::Operator, "}");

    vec![
        derive.build(),
        open.build(),
        first.build(),
        second.build(),
        close.build(),
    ]
}

pub(super) fn render(
    block: &CanonicalBlock,
    width: usize,
    palette: &Palette,
    block_index: usize,
) -> Vec<StyledLine> {
    if let Some(lines) = render_structural(block, palette, "//", "/* · · · · · */") {
        return lines;
    }
    let Some(text) = block_text(block) else {
        return Vec::new();
    };

    let words = extract_words(text);
    let seed = line_hash(block_index, 0);
    let text_width = width.saturating_sub(TEXT_OVERHEAD).max(20);

    if block_index % 41 == 0 {
        const CONDITIONS: [&str; 5] = ["is_valid", "flag", "active", "ready", "loaded"];
        let wrapped = wrap_text(text, text_width);
        let half = wrapped.len().div_ceil(2);

        let mut open = LineBuilder::new(palette);
        open.push(Role::Keyword, "if")
            .space()
            .push(Role::Ident, CONDITIONS[seed % CONDITIONS.len()])
            .push(Role::Operator, " {");
        let mut middle = LineBuilder::new(palette);
        middle.push(Role::Operator, "} else {");
        let mut close = LineBuilder::new(palette);
        close.push(Role::Operator, "}");

        let mut lines = vec![open.build()];
        lines.extend(indented_body(
            &wrapped[..half],
            block_index,
            0,
            "    ",
            palette,
        ));
        lines.push(middle.build());
        lines.extend(indented_body(
            &wrapped[half..],
            block_index,
            half,
            "    ",
            palette,
        ));
        lines.push(close.build());
        return lines;
    }

    // A struct definition stands on its own; the prose only names its fields.
    if block_index % 19 == 0 {
        return struct_lines(&words, seed, palette);
    }

    let mut structural: Vec<StyledLine> = Vec::new();
    if block_index % 13 == 0 {
        structural.push(fn_open(&words, seed, palette));
    } else if block_index % 17 == 0 {
        structural.push(pub_fn_open(&words, seed, palette));
    } else if block_index % 23 == 0 {
        structural.push(impl_open(&words, seed, palette));
    } else if block_index % 29 == 0 {
        structural.push(match_open(&words, seed, palette));
    }

    let indent = if structural.is_empty() { "" } else { "    " };
    let wrapped = wrap_text(text, text_width);
    let opened_a_scope = !structural.is_empty();

    let mut lines = structural;
    lines.extend(indented_body(&wrapped, block_index, 0, indent, palette));
    if opened_a_scope {
        let mut close = LineBuilder::new(palette);
        close.push(Role::Operator, "}");
        lines.push(close.build());
    }
    lines
}

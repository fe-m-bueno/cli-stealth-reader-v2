//! TypeScript disguise.
//!
//! Density selects between "readable" patterns, where the prose stays visible as
//! a comment or template literal, and "dense" patterns, where it becomes the
//! value of an assignment or call. Structural openers (imports, interfaces,
//! classes, control flow) are chosen by block index so the page reads like a
//! real source file rather than a wall of assignments.

use crate::book::CanonicalBlock;
use crate::settings::CodeDensity;
use crate::style::{Style, StyledLine};
use crate::theme::Palette;

use super::super::text::{
    esc, extract_words, line_hash, to_func_name, to_type_name, to_var_name, wrap_text,
    wrapped_line_count,
};
use super::{LineBuilder, Role, block_text, comment_line, render_structural};

/// Visual columns the boilerplate around the prose can consume.
///
/// Worst case is the spread pattern: `const ` (6) + name (10) +
/// ` = { ...state, ` (15) + key (5) + `: "";` (6) = 42, plus four columns of
/// slack for escaped quotes.
const TEXT_OVERHEAD: usize = 46;

pub(super) fn line_count(block: &CanonicalBlock, width: usize, block_index: usize) -> usize {
    if block_text(block).is_none() {
        return 1;
    }
    let text = block.text();
    let text_width = width.saturating_sub(TEXT_OVERHEAD).max(20);
    if block_index % 41 == 0 {
        return wrapped_line_count(text, text_width.saturating_sub(2)) + 3;
    }
    if block_index % 43 == 0 {
        return wrapped_line_count(text, text_width.saturating_sub(2)) + 2;
    }
    if block_index % 47 == 0 {
        return wrapped_line_count(text, text_width.saturating_sub(2)) + 3;
    }

    let structural = if block_index % 13 == 0 || block_index % 17 == 0 {
        1
    } else if block_index % 19 == 0 {
        4
    } else if block_index % 23 == 0 || block_index % 29 == 0 {
        1
    } else if block_index % 31 == 0 {
        2
    } else if block_index % 37 == 0 {
        1
    } else {
        0
    };
    let inner_width = if structural == 0 {
        text_width
    } else {
        text_width.saturating_sub(4).max(20)
    };
    let closes_scope = matches!(block_index, index if index % 23 == 0
        || index % 29 == 0
        || index % 31 == 0
        || index % 37 == 0);
    structural + wrapped_line_count(text, inner_width) + usize::from(closes_scope)
}

type Pattern = fn(&str, &[&str], usize, &Palette) -> StyledLine;

fn literal(line: &str) -> String {
    format!("\"{}\"", esc(line))
}

fn template(line: &str) -> String {
    format!("`{}`", esc(line))
}

fn pat_const(line: &str, words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Keyword, "const")
        .space()
        .push(Role::Ident, to_var_name(words, seed, ""))
        .push(Role::Operator, " = ")
        .push(Role::Literal, literal(line))
        .push(Role::Operator, ";");
    builder.build()
}

fn pat_let(line: &str, words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Keyword, "let")
        .space()
        .push(Role::Ident, to_var_name(words, seed, "Value"))
        .push(Role::Operator, " = ")
        .push(Role::Literal, literal(line))
        .push(Role::Operator, ";");
    builder.build()
}

fn pat_comment(line: &str, _words: &[&str], _seed: usize, palette: &Palette) -> StyledLine {
    comment_line(palette, format!("// {line}"))
}

fn pat_return(line: &str, _words: &[&str], _seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Keyword, "return")
        .space()
        .push(Role::Literal, template(line))
        .push(Role::Operator, ";");
    builder.build()
}

fn pat_console_log(line: &str, _words: &[&str], _seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Ident, "console")
        .push(Role::Operator, ".")
        .push(Role::Function, "log")
        .push(Role::Operator, "(")
        .push(Role::Literal, literal(line))
        .push(Role::Operator, ");");
    builder.build()
}

fn pat_arrow(line: &str, words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Keyword, "const")
        .space()
        .push(Role::Ident, to_var_name(words, seed, ""))
        .push(Role::Operator, " = () => ")
        .push(Role::Literal, template(line))
        .push(Role::Operator, ";");
    builder.build()
}

fn pat_export(line: &str, words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Keyword, "export")
        .space()
        .push(Role::Keyword, "const")
        .space()
        .push(Role::Ident, to_func_name(words, seed))
        .push(Role::Operator, " = ")
        .push(Role::Literal, literal(line))
        .push(Role::Operator, ";");
    builder.build()
}

fn pat_throw(line: &str, _words: &[&str], _seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Keyword, "throw")
        .space()
        .push(Role::Keyword, "new")
        .space()
        .push(Role::Type, "Error")
        .push(Role::Operator, "(")
        .push(Role::Literal, literal(line))
        .push(Role::Operator, ");");
    builder.build()
}

fn pat_await(line: &str, words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Keyword, "await")
        .space()
        .push(Role::Function, to_func_name(words, seed))
        .push(Role::Operator, "(")
        .push(Role::Literal, literal(line))
        .push(Role::Operator, ");");
    builder.build()
}

fn pat_nullish(line: &str, words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Keyword, "const")
        .space()
        .push(Role::Ident, to_var_name(words, seed, ""))
        .push(Role::Operator, " = ")
        .push(Role::Ident, "state")
        .push(Role::Operator, ".")
        .push(Role::Dim, "value")
        .push(Role::Operator, " ?? ")
        .push(Role::Literal, literal(line))
        .push(Role::Operator, ";");
    builder.build()
}

fn pat_optional(line: &str, words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Keyword, "const")
        .space()
        .push(Role::Ident, to_var_name(words, seed, ""))
        .push(Role::Operator, " = ")
        .push(Role::Ident, "ctx")
        .push(Role::Operator, "?.")
        .push(Role::Dim, "text")
        .push(Role::Operator, " ?? ")
        .push(Role::Literal, literal(line))
        .push(Role::Operator, ";");
    builder.build()
}

fn pat_type_annotation(line: &str, words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Keyword, "type")
        .space()
        .push(Role::Type, to_type_name(words, seed, ""))
        .push(Role::Operator, " = ")
        .push(Role::Literal, literal(line))
        .push(Role::Operator, ";");
    builder.build()
}

fn pat_cast(line: &str, words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Keyword, "const")
        .space()
        .push(Role::Ident, to_var_name(words, seed, ""))
        .push(Role::Operator, " = ")
        .push(Role::Literal, literal(line))
        .space()
        .push(Role::Keyword, "as")
        .space()
        .push(Role::Type, to_type_name(words, seed + 1, ""))
        .push(Role::Operator, ";");
    builder.build()
}

fn pat_generic_call(line: &str, words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Function, to_func_name(words, seed))
        .push(Role::Operator, "<")
        .push(Role::Type, to_type_name(words, seed + 2, ""))
        .push(Role::Operator, ">(")
        .push(Role::Literal, literal(line))
        .push(Role::Operator, ");");
    builder.build()
}

fn truncate(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

fn pat_destructure(line: &str, words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    let first = truncate(&to_var_name(words, seed, ""), 6);
    let second = truncate(&to_var_name(words, seed + 3, "Id"), 6);
    let function = truncate(&to_func_name(words, seed + 1), 7);
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Keyword, "const")
        .space()
        .push(Role::Operator, "{ ")
        .push(Role::Dim, first)
        .push(Role::Operator, ", ")
        .push(Role::Dim, second)
        .push(Role::Operator, " } = ")
        .push(Role::Function, function)
        .push(Role::Operator, "(")
        .push(Role::Literal, literal(line))
        .push(Role::Operator, ");");
    builder.build()
}

fn pat_spread(line: &str, words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    let key = truncate(&to_var_name(words, seed + 2, ""), 5);
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Keyword, "const")
        .space()
        .push(Role::Ident, to_var_name(words, seed, ""))
        .push(Role::Operator, " = { ...")
        .push(Role::Ident, "state")
        .push(Role::Operator, format!(", {key}: "))
        .push(Role::Literal, literal(line))
        .push(Role::Operator, " };");
    builder.build()
}

const CONDITIONS: [&str; 5] = ["isValid", "flag", "active", "ready", "loaded"];

fn pat_ternary(line: &str, words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Keyword, "const")
        .space()
        .push(Role::Ident, to_var_name(words, seed, ""))
        .push(Role::Operator, " = ")
        .push(Role::Dim, CONDITIONS[seed % CONDITIONS.len()])
        .push(Role::Operator, " ? ")
        .push(Role::Literal, literal(line))
        .push(Role::Operator, " : ")
        .push(Role::Dim, "null")
        .push(Role::Operator, ";");
    builder.build()
}

/// Patterns that keep the prose plainly legible.
const COMMENT_PATTERNS: [Pattern; 2] = [pat_comment, pat_return];

/// Patterns that bury the prose in assignments and calls.
const CODE_PATTERNS: [Pattern; 15] = [
    pat_const,
    pat_let,
    pat_arrow,
    pat_console_log,
    pat_export,
    pat_throw,
    pat_await,
    pat_nullish,
    pat_optional,
    pat_type_annotation,
    pat_cast,
    pat_generic_call,
    pat_destructure,
    pat_spread,
    pat_ternary,
];

fn disguise_line(
    line: &str,
    block_index: usize,
    line_index: usize,
    palette: &Palette,
    density: CodeDensity,
) -> StyledLine {
    let words = extract_words(line);
    let seed = line_hash(block_index, line_index);
    // Density 1 keeps 80% of lines readable; density 5 keeps none.
    let comment_threshold = usize::from(5 - density.get()) * 20;
    let pool: &[Pattern] = if seed % 100 < comment_threshold {
        &COMMENT_PATTERNS
    } else {
        &CODE_PATTERNS
    };
    pool[seed % pool.len()](line, &words, seed, palette)
}

/// Body lines, where roughly a third get an extra indent level so the disguise
/// looks like it contains nested blocks.
fn body_lines(
    wrapped: &[String],
    block_index: usize,
    line_offset: usize,
    base_indent: &str,
    palette: &Palette,
    density: CodeDensity,
) -> Vec<StyledLine> {
    wrapped
        .iter()
        .enumerate()
        .map(|(offset, line)| {
            let line_index = line_offset + offset;
            let nested = line_hash(block_index, line_index + 50) % 3 == 0;
            let indent = if nested { "    " } else { base_indent };
            let mut builder = LineBuilder::indented(palette, indent);
            builder.extend(disguise_line(
                line,
                block_index,
                line_index,
                palette,
                density,
            ));
            builder.build()
        })
        .collect()
}

fn with_generics(type_name: &str, seed: usize) -> String {
    if seed % 10 >= 3 {
        return type_name.to_owned();
    }
    const SUFFIXES: [&str; 3] = ["<T>", "<T, K>", "<T extends Base>"];
    format!("{type_name}{}", SUFFIXES[seed % 3])
}

fn import_line(words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    let module = to_var_name(words, seed + 2, "").to_lowercase();
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Keyword, "import")
        .space()
        .push(Role::Operator, "{ ")
        .push(Role::Ident, to_func_name(words, seed))
        .push(Role::Operator, " }")
        .space()
        .push(Role::Keyword, "from")
        .space()
        .push(Role::Literal, format!("\"./{module}\""));
    builder.build()
}

fn function_open(words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Keyword, "function")
        .space()
        .push(Role::Function, to_func_name(words, seed))
        .push(Role::Operator, "() {");
    builder.build()
}

fn async_open(words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Keyword, "async")
        .space()
        .push(Role::Keyword, "function")
        .space()
        .push(Role::Function, to_func_name(words, seed))
        .push(Role::Operator, "(): ")
        .push(Role::Type, "Promise")
        .push(Role::Operator, "<")
        .push(Role::Type, "void")
        .push(Role::Operator, "> {");
    builder.build()
}

fn interface_lines(words: &[&str], seed: usize, palette: &Palette) -> Vec<StyledLine> {
    const TYPES: [&str; 3] = ["string", "number", "boolean"];
    let type_name = with_generics(&to_type_name(words, seed, ""), seed);

    let mut open = LineBuilder::new(palette);
    open.push(Role::Keyword, "interface")
        .space()
        .push(Role::Type, type_name)
        .push(Role::Operator, " {");

    let mut first = LineBuilder::indented(palette, "  ");
    first
        .push(Role::Dim, to_var_name(words, seed + 1, ""))
        .push(Role::Operator, ": ")
        .push(Role::Type, TYPES[seed % 3])
        .push(Role::Operator, ";");

    let mut second = LineBuilder::indented(palette, "  ");
    second
        .push(Role::Dim, to_var_name(words, seed + 2, "Id"))
        .push(Role::Operator, ": ")
        .push(Role::Type, TYPES[(seed + 1) % 3])
        .push(Role::Operator, ";");

    let mut close = LineBuilder::new(palette);
    close.push(Role::Operator, "}");

    vec![open.build(), first.build(), second.build(), close.build()]
}

fn enum_line(words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    const MEMBERS: [&str; 7] = [
        "Active", "Pending", "Resolved", "Ready", "Loading", "Done", "Error",
    ];
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Keyword, "enum")
        .space()
        .push(Role::Type, to_type_name(words, seed, ""))
        .push(Role::Operator, " { ")
        .push(Role::Ident, MEMBERS[seed % MEMBERS.len()])
        .push(Role::Operator, ", ")
        .push(Role::Ident, MEMBERS[(seed + 2) % MEMBERS.len()])
        .push(Role::Operator, ", ")
        .push(Role::Ident, MEMBERS[(seed + 4) % MEMBERS.len()])
        .push(Role::Operator, " }");
    builder.build()
}

fn class_lines(words: &[&str], seed: usize, palette: &Palette) -> Vec<StyledLine> {
    const DECORATORS: [&str; 5] = [
        "Injectable",
        "Component",
        "Service",
        "Controller",
        "Directive",
    ];
    let mut decorator = LineBuilder::new(palette);
    decorator
        .push(Role::Operator, "@")
        .push(Role::Function, DECORATORS[seed % DECORATORS.len()])
        .push(Role::Operator, "()");

    let mut open = LineBuilder::new(palette);
    open.push(Role::Keyword, "class")
        .space()
        .push(
            Role::Type,
            with_generics(&to_type_name(words, seed, ""), seed + 5),
        )
        .push(Role::Operator, " {");

    vec![decorator.build(), open.build()]
}

fn generic_function_open(words: &[&str], seed: usize, palette: &Palette) -> StyledLine {
    let mut builder = LineBuilder::new(palette);
    builder
        .push(Role::Keyword, "function")
        .space()
        .push(Role::Function, to_func_name(words, seed))
        .push(Role::Operator, "<")
        .push(Role::Type, "T")
        .space()
        .push(Role::Keyword, "extends")
        .space()
        .push(Role::Type, to_type_name(words, seed + 2, ""))
        .push(Role::Operator, ">")
        .push(Role::Operator, "(")
        .push(Role::Ident, "item")
        .push(Role::Operator, ": ")
        .push(Role::Type, "T")
        .push(Role::Operator, "): ")
        .push(Role::Type, "Promise")
        .push(Role::Operator, "<")
        .push(Role::Type, to_type_name(words, seed + 1, ""))
        .push(Role::Operator, "> {");
    builder.build()
}

pub(super) fn render(
    block: &CanonicalBlock,
    width: usize,
    palette: &Palette,
    block_index: usize,
    density: CodeDensity,
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

    // Control-flow blocks split the prose across both branches.
    if block_index % 41 == 0 {
        let wrapped = wrap_text(text, text_width.saturating_sub(2));
        let half = wrapped.len().div_ceil(2);
        let mut open = LineBuilder::new(palette);
        open.push(Role::Keyword, "if")
            .push(Role::Operator, " (")
            .push(Role::Ident, CONDITIONS[seed % CONDITIONS.len()])
            .push(Role::Operator, ") {");
        let mut middle = LineBuilder::new(palette);
        middle.push(Role::Operator, "} else {");
        let mut close = LineBuilder::new(palette);
        close.push(Role::Operator, "}");

        let mut lines = vec![open.build()];
        lines.extend(body_lines(
            &wrapped[..half],
            block_index,
            0,
            "  ",
            palette,
            density,
        ));
        lines.push(middle.build());
        lines.extend(body_lines(
            &wrapped[half..],
            block_index,
            half,
            "  ",
            palette,
            density,
        ));
        lines.push(close.build());
        return lines;
    }

    if block_index % 43 == 0 {
        const ARRAYS: [&str; 5] = ["items", "entries", "records", "nodes", "chunks"];
        let wrapped = wrap_text(text, text_width.saturating_sub(2));
        let mut open = LineBuilder::new(palette);
        open.push(Role::Keyword, "for")
            .push(Role::Operator, " (")
            .push(Role::Keyword, "const")
            .space()
            .push(Role::Ident, "item")
            .space()
            .push(Role::Keyword, "of")
            .space()
            .push(Role::Ident, ARRAYS[seed % ARRAYS.len()])
            .push(Role::Operator, ") {");
        let mut close = LineBuilder::new(palette);
        close.push(Role::Operator, "}");

        let mut lines = vec![open.build()];
        lines.extend(body_lines(&wrapped, block_index, 0, "  ", palette, density));
        lines.push(close.build());
        return lines;
    }

    if block_index % 47 == 0 {
        const ERROR_NAMES: [&str; 4] = ["err", "error", "e", "ex"];
        let wrapped = wrap_text(text, text_width.saturating_sub(2));
        let half = wrapped.len().div_ceil(2);
        let mut open = LineBuilder::new(palette);
        open.push(Role::Keyword, "try")
            .space()
            .push(Role::Operator, "{");
        let mut middle = LineBuilder::new(palette);
        middle
            .push(Role::Operator, "} ")
            .push(Role::Keyword, "catch")
            .space()
            .push(Role::Operator, "(")
            .push(Role::Ident, ERROR_NAMES[seed % ERROR_NAMES.len()])
            .push(Role::Operator, ") {");
        let mut close = LineBuilder::new(palette);
        close.push(Role::Operator, "}");

        let mut lines = vec![open.build()];
        lines.extend(body_lines(
            &wrapped[..half],
            block_index,
            0,
            "  ",
            palette,
            density,
        ));
        lines.push(middle.build());
        lines.extend(body_lines(
            &wrapped[half..],
            block_index,
            half,
            "  ",
            palette,
            density,
        ));
        lines.push(close.build());
        return lines;
    }

    // Otherwise the block may open with a declaration.
    let mut structural: Vec<StyledLine> = Vec::new();
    if block_index % 13 == 0 {
        structural.push(import_line(&words, seed, palette));
    } else if block_index % 17 == 0 {
        structural.push(enum_line(&words, seed, palette));
    } else if block_index % 19 == 0 {
        structural.extend(interface_lines(&words, seed, palette));
    } else if block_index % 23 == 0 {
        structural.push(function_open(&words, seed, palette));
    } else if block_index % 29 == 0 {
        structural.push(async_open(&words, seed, palette));
    } else if block_index % 31 == 0 {
        structural.extend(class_lines(&words, seed, palette));
    } else if block_index % 37 == 0 {
        structural.push(generic_function_open(&words, seed, palette));
    }

    // An indented body has less room for the prose itself.
    let inner_width = if structural.is_empty() {
        text_width
    } else {
        text_width.saturating_sub(4).max(20)
    };
    let base_indent = if structural.is_empty() { "" } else { "  " };
    let wrapped = wrap_text(text, inner_width);

    let mut lines = structural;
    let opened_a_scope = block_index % 23 == 0
        || block_index % 29 == 0
        || block_index % 31 == 0
        || block_index % 37 == 0;
    let needs_close = !lines.is_empty() && opened_a_scope;
    lines.extend(body_lines(
        &wrapped,
        block_index,
        0,
        base_indent,
        palette,
        density,
    ));
    if needs_close {
        lines.push(StyledLine::single("}", Style::fg(palette.border)));
    }
    lines
}

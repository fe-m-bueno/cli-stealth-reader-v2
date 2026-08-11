//! Text helpers shared by the plain and code renderers.
//!
//! Every helper here is a byte-for-byte port of the v1 behavior, including its
//! quirks, because the disguise output is compared against v1 golden files.
//! Widths and slices count characters where v1 counted UTF-16 code units; the
//! two agree for everything outside the astral planes, and the identifier
//! helpers only ever see ASCII because [`extract_words`] strips the rest.

/// Greedy word wrap. A zero or negative width falls back to 20 columns, and an
/// all-whitespace input still yields one (empty) line.
#[must_use]
pub fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let width = if width == 0 { 20 } else { width };
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;

    for word in text.split_whitespace() {
        let word_width = word.chars().count();
        let next_width = if current.is_empty() {
            word_width
        } else {
            current_width + 1 + word_width
        };
        if next_width > width && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
            current_width = word_width;
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
            current_width = next_width;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Count the lines [`wrap_text`] would produce without allocating their text.
///
/// Layout and progress calculations need only this number. Keeping the same
/// greedy loop here avoids constructing every rendered line in a book merely
/// to immediately discard it.
#[must_use]
pub fn wrapped_line_count(text: &str, width: usize) -> usize {
    let width = if width == 0 { 20 } else { width };
    let mut lines = 0usize;
    let mut current_width = 0usize;

    for word in text.split_whitespace() {
        let word_width = word.chars().count();
        let next_width = if current_width == 0 {
            word_width
        } else {
            current_width + 1 + word_width
        };
        if next_width > width && current_width > 0 {
            lines += 1;
            current_width = word_width;
        } else {
            current_width = next_width;
        }
    }

    // `wrap_text` always returns at least one line, including for whitespace.
    lines + 1
}

/// Words usable as synthetic identifiers: ASCII-ish, longer than two characters,
/// and starting with a letter.
///
/// v1 used `\w`, so anything outside `[A-Za-z0-9_]` becomes a separator — an
/// accented word is split rather than kept.
///
/// The result borrows from `text`. This runs once per rendered line in code
/// mode, so it does not allocate a string per word.
#[must_use]
pub fn extract_words(text: &str) -> Vec<&str> {
    let is_word_byte = |char: char| char.is_ascii_alphanumeric() || char == '_';
    let mut words = Vec::new();
    let mut start: Option<usize> = None;

    for (index, char) in text.char_indices() {
        if is_word_byte(char) {
            start.get_or_insert(index);
            continue;
        }
        if let Some(begin) = start.take() {
            push_word(&mut words, &text[begin..index]);
        }
    }
    if let Some(begin) = start {
        push_word(&mut words, &text[begin..]);
    }
    words
}

fn push_word<'a>(words: &mut Vec<&'a str>, word: &'a str) {
    if word.chars().count() > 2 && word.starts_with(|char: char| char.is_ascii_alphabetic()) {
        words.push(word);
    }
}

/// Deterministic per-line seed. Same block and line always disguise the same way.
#[must_use]
pub const fn line_hash(block_index: usize, line_index: usize) -> usize {
    (block_index * 53 + line_index * 17 + 7) & 0xffff
}

/// Escape backslashes and double quotes for embedding prose in a string literal.
#[must_use]
pub fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for char in text.chars() {
        match char {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            other => out.push(other),
        }
    }
    out
}

/// Longest synthetic identifier stem.
pub const MAX_NAME: usize = 10;

fn truncate(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

fn pick<'a>(words: &[&'a str], fallbacks: &[&'a str], seed: usize) -> &'a str {
    if words.is_empty() {
        fallbacks[seed % fallbacks.len()]
    } else {
        words[seed % words.len()]
    }
}

const VAR_FALLBACKS: [&str; 8] = [
    "data", "result", "value", "content", "item", "output", "state", "ctx",
];
const TYPE_FALLBACKS: [&str; 5] = ["Data", "Config", "State", "Result", "Options"];
const FUNC_PREFIXES: [&str; 8] = [
    "handle", "process", "render", "format", "get", "create", "build", "parse",
];

fn lower_first(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_lowercase().chain(chars).collect(),
    }
}

fn upper_first_lower_rest(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first
            .to_uppercase()
            .chain(chars.flat_map(char::to_lowercase))
            .collect(),
    }
}

/// `camelCase` identifier stem, at most [`MAX_NAME`] characters.
#[must_use]
pub fn to_var_name(words: &[&str], seed: usize, suffix: &str) -> String {
    let word = pick(words, &VAR_FALLBACKS, seed);
    truncate(&format!("{}{suffix}", lower_first(word)), MAX_NAME)
}

/// `PascalCase` type name, at most [`MAX_NAME`] characters.
#[must_use]
pub fn to_type_name(words: &[&str], seed: usize, suffix: &str) -> String {
    let word = pick(words, &TYPE_FALLBACKS, seed);
    truncate(
        &format!("{}{suffix}", upper_first_lower_rest(word)),
        MAX_NAME,
    )
}

/// `prefixNoun` function name, at most `MAX_NAME + 4` characters.
#[must_use]
pub fn to_func_name(words: &[&str], seed: usize) -> String {
    let prefix = FUNC_PREFIXES[seed % FUNC_PREFIXES.len()];
    let word = if words.is_empty() {
        "Content"
    } else {
        words[(seed + 1) % words.len()]
    };
    truncate(
        &format!("{prefix}{}", upper_first_lower_rest(word)),
        MAX_NAME + 4,
    )
}

/// `snake_case` identifier, at most `MAX_NAME + 3` characters.
#[must_use]
pub fn to_snake_name(words: &[&str], seed: usize, suffix: &str) -> String {
    let word = pick(words, &VAR_FALLBACKS, seed);
    let mut base = word.to_lowercase();
    if !suffix.is_empty() {
        base.push('_');
        base.push_str(&suffix.to_lowercase());
    }
    truncate(&base, MAX_NAME + 3)
}

/// `prefix_noun` function name, at most `MAX_NAME + 5` characters.
#[must_use]
pub fn to_snake_func_name(words: &[&str], seed: usize) -> String {
    let prefix = FUNC_PREFIXES[seed % FUNC_PREFIXES.len()];
    let word = if words.is_empty() {
        "content"
    } else {
        words[(seed + 1) % words.len()]
    };
    truncate(&format!("{prefix}_{}", word.to_lowercase()), MAX_NAME + 5)
}

#[cfg(test)]
mod tests {
    use super::{
        esc, extract_words, line_hash, to_func_name, to_snake_func_name, to_snake_name,
        to_type_name, to_var_name, wrap_text, wrapped_line_count,
    };

    fn words<'a>(list: &[&'a str]) -> Vec<&'a str> {
        list.to_vec()
    }

    #[test]
    fn wrapping_is_greedy_and_never_returns_nothing() {
        assert_eq!(
            wrap_text("the quiet harbour at dawn", 10),
            vec!["the quiet", "harbour at", "dawn"]
        );
        assert_eq!(wrap_text("", 40), vec![""]);
        assert_eq!(wrap_text("   ", 40), vec![""]);
    }

    #[test]
    fn a_word_longer_than_the_width_still_gets_its_own_line() {
        assert_eq!(
            wrap_text("supercalifragilistic tiny", 8),
            vec!["supercalifragilistic", "tiny"]
        );
    }

    #[test]
    fn zero_width_falls_back_to_twenty_columns() {
        let wrapped = wrap_text("one two three four five six seven", 0);
        assert!(wrapped.iter().all(|line| line.chars().count() <= 20));
        assert!(wrapped.len() > 1);
    }

    #[test]
    fn allocation_free_line_count_matches_wrapping() {
        for width in [0, 1, 8, 20, 80] {
            for text in [
                "",
                "   ",
                "one",
                "one two three four",
                "supercalifragilistic tiny",
                "café no porto ao amanhecer",
            ] {
                assert_eq!(
                    wrapped_line_count(text, width),
                    wrap_text(text, width).len(),
                    "width {width}, text {text:?}"
                );
            }
        }
    }

    #[test]
    fn word_extraction_keeps_only_identifier_candidates() {
        assert_eq!(
            extract_words("She said \"hello\" to my 42 friends, ok?"),
            words(&["She", "said", "hello", "friends"])
        );
        // Punctuation splits words and accented characters are separators.
        assert_eq!(extract_words("café"), words(&["caf"]));
        assert!(extract_words("a b c 1 2").is_empty());
    }

    #[test]
    fn the_line_hash_is_stable_and_bounded() {
        assert_eq!(line_hash(0, 0), 7);
        assert_eq!(line_hash(1, 1), 77);
        assert_eq!(line_hash(3, 4), 234);
        assert_eq!(line_hash(137, 2), line_hash(137, 2));
        assert!(line_hash(9_999, 9_999) <= 0xffff);
    }

    #[test]
    fn escaping_covers_quotes_and_backslashes() {
        assert_eq!(
            esc(r#"She said "hi" at C:\moon"#),
            r#"She said \"hi\" at C:\\moon"#
        );
    }

    #[test]
    fn identifier_helpers_respect_their_length_caps() {
        let source = words(&["Harbour", "Lantern", "Meridian"]);
        assert_eq!(to_var_name(&source, 0, ""), "harbour");
        assert_eq!(to_var_name(&source, 2, "Value"), "meridianVa");
        assert_eq!(to_type_name(&source, 1, ""), "Lantern");
        assert!(to_func_name(&source, 0).chars().count() <= 14);
        assert_eq!(to_snake_name(&source, 0, "id"), "harbour_id");
        assert!(to_snake_func_name(&source, 3).chars().count() <= 15);
    }

    #[test]
    fn identifier_helpers_fall_back_when_there_are_no_usable_words() {
        assert_eq!(to_var_name(&[], 0, ""), "data");
        assert_eq!(to_type_name(&[], 1, ""), "Config");
        assert_eq!(to_func_name(&[], 0), "handleContent");
        assert_eq!(to_snake_func_name(&[], 0), "handle_content");
    }
}

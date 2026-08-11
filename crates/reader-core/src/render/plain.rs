//! Plain reading mode, including dialogue highlighting.
//!
//! Dialogue detection finds quoted spans (straight, curly, and guillemet pairs)
//! plus dash-introduced speech, then colors those characters with the accent
//! while the rest stays in the foreground. Spans are tracked against the
//! original text so wrapping cannot shift them.

use crate::book::CanonicalBlock;
use crate::style::{Span, Style, StyledLine};
use crate::theme::Palette;

use super::text::wrap_text;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DialogueSpan {
    start: usize,
    end: usize,
}

struct QuotePair {
    open: char,
    close: char,
    /// Apostrophe-shaped pairs need boundary checks so `don't` is not dialogue.
    single: bool,
}

const QUOTE_PAIRS: [QuotePair; 5] = [
    QuotePair {
        open: '"',
        close: '"',
        single: false,
    },
    QuotePair {
        open: '\'',
        close: '\'',
        single: true,
    },
    QuotePair {
        open: '“',
        close: '”',
        single: false,
    },
    QuotePair {
        open: '‘',
        close: '’',
        single: true,
    },
    QuotePair {
        open: '«',
        close: '»',
        single: false,
    },
];

const DIALOGUE_DASHES: [char; 3] = ['—', '―', '–'];

fn is_word_char(value: Option<char>) -> bool {
    value.is_some_and(|char| char.is_alphanumeric() || char == '_')
}

/// A quote preceded by an odd number of backslashes is escaped.
fn is_escaped(chars: &[char], index: usize) -> bool {
    let mut slashes = 0usize;
    let mut cursor = index;
    while cursor > 0 && chars[cursor - 1] == '\\' {
        slashes += 1;
        cursor -= 1;
    }
    slashes % 2 == 1
}

fn is_single_quote_boundary(chars: &[char], open_index: usize, close_index: usize) -> bool {
    let before_open = open_index.checked_sub(1).map(|index| chars[index]);
    let after_open = chars.get(open_index + 1).copied();
    let before_close = close_index.checked_sub(1).map(|index| chars[index]);
    let after_close = chars.get(close_index + 1).copied();

    if is_word_char(before_open) || is_word_char(after_close) {
        return false;
    }
    match after_open {
        None => return false,
        Some(char) if char.is_whitespace() => return false,
        Some(_) => {}
    }
    match before_close {
        None => return false,
        Some(char) if char.is_whitespace() => return false,
        Some(_) => {}
    }
    let content = &chars[open_index + 1..close_index];
    !(content.len() == 1 && is_word_char(Some(content[0])))
}

fn find_closing_quote(
    chars: &[char],
    open_index: usize,
    close: char,
    single: bool,
) -> Option<usize> {
    ((open_index + 1)..chars.len()).find(|cursor| {
        chars[*cursor] == close
            && !is_escaped(chars, *cursor)
            && (!single || is_single_quote_boundary(chars, open_index, *cursor))
    })
}

fn collect_dialogue_spans(chars: &[char]) -> Vec<DialogueSpan> {
    let mut spans: Vec<DialogueSpan> = Vec::new();
    let mut cursor = 0usize;
    while cursor < chars.len() {
        if is_escaped(chars, cursor) {
            cursor += 1;
            continue;
        }
        let Some(pair) = QUOTE_PAIRS.iter().find(|pair| pair.open == chars[cursor]) else {
            cursor += 1;
            continue;
        };
        // An opening quote followed by nothing or by a space is punctuation,
        // except for guillemets which legitimately hug a space in some texts.
        let next = chars.get(cursor + 1).copied();
        if pair.open != '«' && next.is_none_or(char::is_whitespace) {
            cursor += 1;
            continue;
        }
        if let Some(close_index) = find_closing_quote(chars, cursor, pair.close, pair.single) {
            spans.push(DialogueSpan {
                start: cursor,
                end: close_index + 1,
            });
            cursor = close_index + 1;
            continue;
        }
        cursor += 1;
    }

    if let Some(first_non_space) = chars.iter().position(|char| !char.is_whitespace())
        && DIALOGUE_DASHES.contains(&chars[first_non_space])
    {
        spans.push(DialogueSpan {
            start: first_non_space,
            end: chars.len(),
        });
    }

    if spans.len() <= 1 {
        return spans;
    }
    spans.sort_by(|left, right| left.start.cmp(&right.start).then(left.end.cmp(&right.end)));
    let mut merged: Vec<DialogueSpan> = Vec::with_capacity(spans.len());
    for span in spans {
        match merged.last_mut() {
            Some(last) if span.start <= last.end => last.end = last.end.max(span.end),
            _ => merged.push(span),
        }
    }
    merged
}

fn in_dialogue(index: usize, spans: &[DialogueSpan]) -> bool {
    spans
        .iter()
        .any(|span| index >= span.start && index < span.end)
}

/// A character of output plus where it came from; inserted spaces have no source.
#[derive(Debug, Clone, Copy)]
struct IndexedChar {
    value: char,
    source: Option<usize>,
}

fn render_indexed(chars: &[IndexedChar], spans: &[DialogueSpan], palette: &Palette) -> StyledLine {
    let accent = Style::fg(palette.accent);
    let foreground = Style::fg(palette.foreground);
    let mut line = StyledLine::empty();
    for (position, item) in chars.iter().enumerate() {
        let dialogue = match item.source {
            Some(source) => in_dialogue(source, spans),
            // A space introduced by wrapping inherits dialogue only when both
            // neighbours sit inside the same span.
            None => {
                let previous = position
                    .checked_sub(1)
                    .and_then(|index| chars[index].source);
                let next = chars.get(position + 1).and_then(|item| item.source);
                match (previous, next) {
                    (Some(previous), Some(next)) => spans
                        .iter()
                        .any(|span| previous >= span.start && next < span.end),
                    _ => false,
                }
            }
        };
        line.push(Span::new(
            item.value.to_string(),
            if dialogue { accent } else { foreground },
        ));
    }
    if line.spans.is_empty() {
        return StyledLine::single("", foreground);
    }
    line
}

/// Highlight dialogue on one already-wrapped line.
#[must_use]
pub fn highlight_dialogue(line: &str, palette: &Palette) -> StyledLine {
    let chars: Vec<char> = line.chars().collect();
    let spans = collect_dialogue_spans(&chars);
    let indexed: Vec<IndexedChar> = chars
        .iter()
        .enumerate()
        .map(|(index, value)| IndexedChar {
            value: *value,
            source: Some(index),
        })
        .collect();
    render_indexed(&indexed, &spans, palette)
}

/// Wrap `text` to `width` and highlight dialogue against the unwrapped source.
#[must_use]
pub fn wrap_with_dialogue(text: &str, width: usize, palette: &Palette) -> Vec<StyledLine> {
    let width = if width == 0 { 20 } else { width };
    let chars: Vec<char> = text.chars().collect();
    let spans = collect_dialogue_spans(&chars);

    // Word boundaries in character offsets, matching v1's `/\S+/gu` scan.
    let mut words: Vec<(usize, usize)> = Vec::new();
    let mut index = 0usize;
    while index < chars.len() {
        if chars[index].is_whitespace() {
            index += 1;
            continue;
        }
        let start = index;
        while index < chars.len() && !chars[index].is_whitespace() {
            index += 1;
        }
        words.push((start, index));
    }
    if words.is_empty() {
        return vec![StyledLine::single("", Style::fg(palette.foreground))];
    }

    let mut rendered: Vec<StyledLine> = Vec::new();
    let mut current: Vec<IndexedChar> = Vec::new();
    let mut current_width = 0usize;

    for (start, end) in words {
        let word_width = end - start;
        let next_width = if current_width == 0 {
            word_width
        } else {
            current_width + 1 + word_width
        };
        if next_width > width && current_width > 0 {
            rendered.push(render_indexed(&current, &spans, palette));
            current.clear();
            current_width = 0;
        }
        if current_width > 0 {
            current.push(IndexedChar {
                value: ' ',
                source: None,
            });
            current_width += 1;
        }
        for (source, value) in chars.iter().enumerate().take(end).skip(start) {
            current.push(IndexedChar {
                value: *value,
                source: Some(source),
            });
        }
        current_width += word_width;
    }
    if !current.is_empty() {
        rendered.push(render_indexed(&current, &spans, palette));
    }
    if rendered.is_empty() {
        rendered.push(StyledLine::single("", Style::fg(palette.foreground)));
    }
    rendered
}

/// Render one block in plain mode.
#[must_use]
pub fn render_plain(
    block: &CanonicalBlock,
    width: usize,
    palette: &Palette,
    plain_highlight: bool,
) -> Vec<StyledLine> {
    match block {
        CanonicalBlock::Heading { text, .. } => wrap_text(&text.to_uppercase(), width)
            .into_iter()
            .map(|line| StyledLine::single(line, Style::fg(palette.accent).bold()))
            .collect(),
        CanonicalBlock::Blockquote { text, .. } => wrap_text(text, width.saturating_sub(2))
            .into_iter()
            .map(|line| StyledLine::single(format!("❝ {line}"), Style::fg(palette.subtle)))
            .collect(),
        CanonicalBlock::SceneBreak { .. } => {
            let mark = "· · · · ·";
            let mark_width = mark.chars().count();
            // v1 centred with `padStart(floor((width + mark.length) / 2))`.
            let target = (width + mark_width) / 2;
            let padding = target.saturating_sub(mark_width);
            vec![StyledLine::single(
                format!("{}{mark}", " ".repeat(padding)),
                Style::fg(palette.border),
            )]
        }
        CanonicalBlock::Image { text, .. } => {
            let label = if text.is_empty() {
                "[image]".to_owned()
            } else {
                format!("[image: {text}]")
            };
            vec![StyledLine::single(label, Style::fg(palette.subtle))]
        }
        CanonicalBlock::ListItem { text, .. } => {
            render_body(&format!("  · {text}"), width, palette, plain_highlight)
        }
        CanonicalBlock::Paragraph { text, .. } | CanonicalBlock::Anchor { text, .. } => {
            render_body(text, width, palette, plain_highlight)
        }
    }
}

fn render_body(
    text: &str,
    width: usize,
    palette: &Palette,
    plain_highlight: bool,
) -> Vec<StyledLine> {
    if plain_highlight {
        wrap_with_dialogue(text, width, palette)
    } else {
        wrap_text(text, width)
            .into_iter()
            .map(|line| StyledLine::single(line, Style::fg(palette.foreground)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{highlight_dialogue, render_plain, wrap_with_dialogue};
    use crate::book::CanonicalBlock;
    use crate::style::Style;
    use crate::theme::Theme;

    fn palette() -> crate::theme::Palette {
        Theme::default().palette
    }

    /// The substrings a line renders in the accent color.
    fn accented(line: &crate::style::StyledLine) -> Vec<String> {
        let accent = Style::fg(palette().accent);
        line.spans
            .iter()
            .filter(|span| span.style == accent)
            .map(|span| span.text.clone())
            .collect()
    }

    #[test]
    fn straight_and_curly_quotes_are_dialogue() {
        let line = highlight_dialogue(r#"She said "come in" softly."#, &palette());
        assert_eq!(accented(&line), vec![r#""come in""#]);

        let curly = highlight_dialogue("She said “come in” softly.", &palette());
        assert_eq!(accented(&curly), vec!["“come in”"]);
    }

    #[test]
    fn an_apostrophe_inside_a_word_is_not_dialogue() {
        let line = highlight_dialogue("He didn't answer at all.", &palette());
        assert!(accented(&line).is_empty());
    }

    #[test]
    fn an_escaped_quote_does_not_open_dialogue() {
        let line = highlight_dialogue(r#"The path was C:\"odd\" indeed"#, &palette());
        assert!(accented(&line).is_empty());
    }

    #[test]
    fn a_leading_dash_marks_the_whole_line_as_speech() {
        let line = highlight_dialogue("— Come in, she said.", &palette());
        assert_eq!(accented(&line), vec!["— Come in, she said."]);
    }

    #[test]
    fn an_unclosed_quote_is_left_alone() {
        let line = highlight_dialogue(r#"She said "come in and stayed"#, &palette());
        assert!(accented(&line).is_empty());
    }

    #[test]
    fn dialogue_survives_wrapping_across_lines() {
        let text = r#"She said "come in from the cold night air" and closed the door."#;
        let lines = wrap_with_dialogue(text, 20, &palette());
        assert!(lines.len() > 2);
        let joined: String = lines
            .iter()
            .map(crate::style::StyledLine::text)
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(joined, text);
        // Each wrapped line carries its own share of the quoted span; the
        // closing quote ends the highlight mid-line.
        let per_line: Vec<Vec<String>> = lines.iter().map(accented).collect();
        assert_eq!(per_line[0], vec!["\"come in"]);
        assert_eq!(per_line[1], vec!["from the cold night"]);
        assert_eq!(per_line[2], vec!["air\""]);
        assert!(per_line[3].is_empty());
    }

    #[test]
    fn structural_blocks_render_their_v1_decorations() {
        let width = 20;
        let heading = render_plain(
            &CanonicalBlock::Heading {
                id: "h".into(),
                text: "a quiet chapter".into(),
                level: Some(1),
            },
            width,
            &palette(),
            true,
        );
        assert_eq!(heading[0].text(), "A QUIET CHAPTER");

        let quote = render_plain(
            &CanonicalBlock::Blockquote {
                id: "q".into(),
                text: "remember this".into(),
            },
            width,
            &palette(),
            true,
        );
        assert_eq!(quote[0].text(), "❝ remember this");

        // The bullet indent is collapsed by wrapping, as in v1: leading
        // whitespace never survives the word split.
        let list = render_plain(
            &CanonicalBlock::ListItem {
                id: "l".into(),
                text: "first".into(),
            },
            width,
            &palette(),
            true,
        );
        assert_eq!(list[0].text(), "· first");

        let break_line = render_plain(
            &CanonicalBlock::SceneBreak {
                id: "s".into(),
                text: String::new(),
            },
            width,
            &palette(),
            true,
        );
        assert_eq!(break_line[0].text(), "     · · · · ·");

        let image = render_plain(
            &CanonicalBlock::Image {
                id: "i".into(),
                text: "Map of Arrakis".into(),
                image_source: None,
            },
            width,
            &palette(),
            true,
        );
        assert_eq!(image[0].text(), "[image: Map of Arrakis]");

        let bare_image = render_plain(
            &CanonicalBlock::Image {
                id: "i".into(),
                text: String::new(),
                image_source: None,
            },
            width,
            &palette(),
            true,
        );
        assert_eq!(bare_image[0].text(), "[image]");
    }

    #[test]
    fn highlighting_can_be_switched_off() {
        let block = CanonicalBlock::Paragraph {
            id: "p".into(),
            text: r#"She said "come in" softly."#.into(),
        };
        let plain = render_plain(&block, 40, &palette(), false);
        assert!(accented(&plain[0]).is_empty());
        assert_eq!(plain[0].text(), r#"She said "come in" softly."#);
    }
}

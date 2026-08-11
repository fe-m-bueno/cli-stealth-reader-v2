//! The frame's chrome: the rules that open and close the reading column.
//!
//! The reading area is not a box. A rounded rule sits above it carrying the
//! book and the render mode, another sits below it carrying the status and the
//! key hints, and nothing walls the text in between — that is what keeps a
//! terminal's own text selection usable across a whole paragraph.
//!
//! Everything here is a pure function of widths and strings, so the geometry can
//! be asserted without a terminal.

use ratatui::text::{Line, Span};
use reader_core::Palette;
use reader_core::style::Style;

use crate::style::to_tui_style;

/// Columns the opening and closing decorations of a rule take.
const RULE_PREFIX: usize = 3;
const RULE_SUFFIX: usize = 3;
/// Columns between the left text and the fill.
const RULE_SEPARATOR: usize = 2;
/// The rule always shows at least this much horizontal line, so the two ends
/// never look glued together.
const RULE_MIN_FILL: usize = 3;

/// Shorten `text` to `width` columns, marking the cut with an ellipsis.
///
/// Counting is by character rather than by byte, so a title with accents is not
/// cut mid-codepoint.
#[must_use]
pub fn truncate(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if text.chars().count() <= width {
        return text.to_owned();
    }
    let mut shortened: String = text.chars().take(width - 1).collect();
    shortened.push('…');
    shortened
}

/// Pad `text` with spaces so it occupies exactly `width` columns.
#[must_use]
pub fn pad(text: &str, width: usize) -> String {
    let mut padded = truncate(text, width);
    let length = padded.chars().count();
    padded.extend(std::iter::repeat_n(' ', width.saturating_sub(length)));
    padded
}

/// Which end of the reading column a rule closes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleSide {
    Top,
    Bottom,
}

impl RuleSide {
    const fn corners(self) -> (&'static str, &'static str) {
        match self {
            Self::Top => ("╭─ ", " ─╮"),
            Self::Bottom => ("╰─ ", " ─╯"),
        }
    }
}

/// One rounded rule: `╭─ left ─────── right ─╮`.
///
/// The left text yields first when the terminal is too narrow, because the right
/// side is the fixed-width state (mode, theme, key hints) and losing a character
/// of it changes what it means.
#[must_use]
pub fn rule(
    side: RuleSide,
    width: u16,
    left: &str,
    right: &str,
    palette: &Palette,
) -> Line<'static> {
    let border = to_tui_style(Style::fg(palette.border));
    let width = width as usize;
    let (open, close) = side.corners();

    // Below this there is no room for both ends and any text at all; a plain
    // line still reads as a frame.
    if width < RULE_PREFIX + RULE_SUFFIX + RULE_SEPARATOR + RULE_MIN_FILL {
        return Line::from(Span::styled("─".repeat(width), border));
    }

    let budget = width - RULE_PREFIX - RULE_SUFFIX - RULE_SEPARATOR - RULE_MIN_FILL;
    let right = truncate(right, budget);
    let left = truncate(left, budget - right.chars().count());

    let used =
        RULE_PREFIX + left.chars().count() + RULE_SEPARATOR + right.chars().count() + RULE_SUFFIX;
    let fill = width.saturating_sub(used).max(RULE_MIN_FILL);

    let mut spans = vec![Span::styled(open, border)];
    if !left.is_empty() {
        spans.push(Span::styled(
            left,
            to_tui_style(Style::fg(palette.foreground)),
        ));
    }
    spans.push(Span::styled(" ─", border));
    spans.push(Span::styled("─".repeat(fill), border));
    if !right.is_empty() {
        spans.push(Span::styled(right, to_tui_style(Style::fg(palette.dim))));
    }
    spans.push(Span::styled(close, border));
    Line::from(spans)
}

/// `Key:label` pairs joined the way both footers show them.
#[must_use]
pub fn hints(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(key, label)| format!("{key}:{label}"))
        .collect::<Vec<_>>()
        .join("  │  ")
}

/// Where a scrollbar's thumb sits, and how far the content can scroll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollbarMetrics {
    /// Largest offset that still fills the viewport.
    pub max_offset: usize,
    /// Rows the thumb covers.
    pub thumb_height: usize,
    /// Rows between the top of the track and the thumb.
    pub thumb_offset: usize,
}

/// Thumb geometry for `total_lines` of content in `body_height` rows.
#[must_use]
pub fn scrollbar_metrics(
    total_lines: usize,
    body_height: usize,
    offset: usize,
) -> ScrollbarMetrics {
    if body_height == 0 {
        return ScrollbarMetrics {
            max_offset: 0,
            thumb_height: 0,
            thumb_offset: 0,
        };
    }
    let max_offset = total_lines.saturating_sub(body_height);
    if max_offset == 0 {
        return ScrollbarMetrics {
            max_offset: 0,
            thumb_height: body_height,
            thumb_offset: 0,
        };
    }
    let thumb_height = ((body_height * body_height) / total_lines).clamp(1, body_height);
    let travel = body_height - thumb_height;
    let thumb_offset = offset.min(max_offset) * travel / max_offset;
    ScrollbarMetrics {
        max_offset,
        thumb_height,
        thumb_offset,
    }
}

/// The content offset that puts the thumb's top at `thumb_top_row`.
///
/// This is the inverse of [`scrollbar_metrics`], and it is what turns a click or
/// a drag on the track into a place in the chapter.
#[must_use]
pub fn offset_from_thumb_row(
    total_lines: usize,
    body_height: usize,
    thumb_top_row: isize,
) -> usize {
    let metrics = scrollbar_metrics(total_lines, body_height, 0);
    if metrics.max_offset == 0 || body_height <= metrics.thumb_height {
        return 0;
    }
    let travel = body_height - metrics.thumb_height;
    let clamped = thumb_top_row.clamp(0, travel as isize) as usize;
    // Rounded rather than truncated so dragging to the last row reaches the end.
    ((clamped * metrics.max_offset + travel / 2) / travel).min(metrics.max_offset)
}

/// Where a window of `visible` rows should start so `cursor` stays on screen.
#[must_use]
pub fn window_start(length: usize, visible: usize, cursor: usize) -> usize {
    if length <= visible {
        return 0;
    }
    cursor.saturating_sub(visible / 2).min(length - visible)
}

#[cfg(test)]
mod tests {
    use reader_core::AppSettings;

    use super::{
        RuleSide, hints, offset_from_thumb_row, pad, rule, scrollbar_metrics, truncate,
        window_start,
    };

    fn palette() -> reader_core::Palette {
        AppSettings::default().theme().palette
    }

    fn text(line: &ratatui::text::Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn truncation_counts_characters_and_marks_the_cut() {
        assert_eq!(truncate("harbour", 10), "harbour");
        assert_eq!(truncate("harbour", 7), "harbour");
        assert_eq!(truncate("harbour", 4), "har…");
        assert_eq!(truncate("harbour", 0), "");
        assert_eq!(
            truncate("Edição", 5),
            "Ediç…",
            "an accent is one character, not two bytes"
        );
    }

    #[test]
    fn padding_fills_to_exactly_the_requested_width() {
        assert_eq!(pad("ab", 5).chars().count(), 5);
        assert_eq!(pad("abcdef", 3).chars().count(), 3);
    }

    #[test]
    fn a_rule_is_exactly_as_wide_as_the_frame() {
        for width in 0..=120u16 {
            let line = rule(
                RuleSide::Top,
                width,
                "Quiet Harbour",
                "plain · Codex",
                &palette(),
            );
            assert_eq!(
                text(&line).chars().count(),
                width as usize,
                "width {width} produced {:?}",
                text(&line)
            );
        }
    }

    #[test]
    fn a_rule_keeps_its_rounded_corners_and_both_sides() {
        let line = text(&rule(
            RuleSide::Top,
            60,
            "Quiet Harbour",
            "plain · Codex",
            &palette(),
        ));
        assert!(line.starts_with("╭─ Quiet Harbour ─"), "{line:?}");
        assert!(line.ends_with("plain · Codex ─╮"), "{line:?}");

        let bottom = text(&rule(RuleSide::Bottom, 60, "Opened", "q:quit", &palette()));
        assert!(bottom.starts_with("╰─ Opened"), "{bottom:?}");
        assert!(bottom.ends_with("q:quit ─╯"), "{bottom:?}");
    }

    #[test]
    fn a_narrow_rule_sacrifices_the_left_side_first() {
        let line = text(&rule(
            RuleSide::Top,
            28,
            "A very long book title indeed",
            "plain",
            &palette(),
        ));
        assert!(
            line.contains("plain ─╮"),
            "the right side survives: {line:?}"
        );
        assert!(line.contains('…'), "the left side is cut: {line:?}");
    }

    #[test]
    fn hints_read_as_key_label_pairs() {
        assert_eq!(
            hints(&[("Esc", "close"), ("q", "quit")]),
            "Esc:close  │  q:quit"
        );
    }

    #[test]
    fn a_thumb_covers_the_whole_track_when_everything_fits() {
        let metrics = scrollbar_metrics(5, 10, 0);
        assert_eq!(metrics.max_offset, 0);
        assert_eq!(metrics.thumb_height, 10);
        assert_eq!(metrics.thumb_offset, 0);
    }

    #[test]
    fn a_thumb_shrinks_and_travels_with_the_offset() {
        let top = scrollbar_metrics(100, 10, 0);
        let bottom = scrollbar_metrics(100, 10, 90);
        assert_eq!(top.max_offset, 90);
        assert_eq!(top.thumb_offset, 0);
        assert_eq!(bottom.thumb_offset, 10 - bottom.thumb_height);
        assert!(bottom.thumb_height >= 1);
    }

    #[test]
    fn a_thumb_row_maps_back_to_the_offset_it_came_from() {
        let total = 100;
        let height = 10;
        for offset in [0, 17, 45, 90] {
            let metrics = scrollbar_metrics(total, height, offset);
            let round_trip = offset_from_thumb_row(total, height, metrics.thumb_offset as isize);
            let travel = height - metrics.thumb_height;
            let tolerance = metrics.max_offset.div_ceil(travel);
            assert!(
                round_trip.abs_diff(offset) <= tolerance,
                "offset {offset} came back as {round_trip}"
            );
        }
    }

    #[test]
    fn thumb_rows_outside_the_track_clamp_to_the_ends() {
        assert_eq!(offset_from_thumb_row(100, 10, -20), 0);
        assert_eq!(offset_from_thumb_row(100, 10, 999), 90);
        assert_eq!(offset_from_thumb_row(5, 10, 3), 0, "nothing to scroll");
    }

    #[test]
    fn a_window_follows_the_cursor_without_leaving_the_list() {
        assert_eq!(window_start(3, 10, 2), 0, "a short list never scrolls");
        assert_eq!(window_start(100, 10, 0), 0);
        assert_eq!(window_start(100, 10, 50), 45);
        assert_eq!(window_start(100, 10, 99), 90, "the end stays full");
    }
}

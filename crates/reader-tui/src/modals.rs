//! The centred modal every list overlay is drawn in.
//!
//! One geometry and one renderer serve the shortcuts page, the library, the file
//! picker, and the smaller lists. Sharing them is what keeps the close button,
//! the search row, and the footer hints in the same place everywhere — and it is
//! what lets a pointer be hit-tested against the same rectangle that was drawn.
//!
//! The reader stays visible behind the modal but is repainted in the theme's
//! subtle colour, so it reads as background rather than as something to act on.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style as TuiStyle;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use reader_app::{EntryStyle, OverlayEntry, ReaderState};
use reader_core::Palette;
use reader_core::style::Style;

use crate::chrome::{pad, truncate, window_start};
use crate::style::to_tui_style;

/// Rows the modal spends on its own chrome: border, search, two dividers,
/// footer hints, and the closing border.
const CHROME_ROWS: u16 = 6;

/// Where a modal sits inside the frame, and which part of it is the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModalGeometry {
    /// The modal rectangle, borders included.
    pub area: Rect,
    /// Rows available for entries.
    pub visible_rows: u16,
    /// Row of the first entry.
    pub entries_y: u16,
}

/// A centred modal, sized as a share of the frame and clamped to sane limits.
#[must_use]
pub fn modal_geometry(area: Rect) -> ModalGeometry {
    let max_width = area.width.saturating_sub(2).max(20);
    let max_height = area.height.saturating_sub(2).max(CHROME_ROWS + 2);
    let width = (area.width * 7 / 10).clamp(44.min(max_width), 80.min(max_width));
    let height = (area.height * 76 / 100).clamp(16.min(max_height), max_height);
    let modal = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    ModalGeometry {
        area: modal,
        visible_rows: height.saturating_sub(CHROME_ROWS).max(1),
        entries_y: modal.y + 3,
    }
}

/// What a pointer landed on inside a modal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalHit {
    /// The `[×]` on the title bar.
    Close,
    /// The search row.
    Search,
    /// An entry, by its index in the visible list.
    Row(usize),
}

/// Which part of the modal covers `(column, row)`, if any.
///
/// The window is computed from the same cursor the renderer used, so a click
/// lands on the row the reader actually sees.
#[must_use]
pub fn hit_test(
    area: Rect,
    row_count: usize,
    cursor: usize,
    column: u16,
    row: u16,
) -> Option<ModalHit> {
    let geometry = modal_geometry(area);
    let modal = geometry.area;
    if column < modal.x || column >= modal.x + modal.width {
        return None;
    }
    if row == modal.y {
        // The close button occupies the last few cells of the title bar.
        return (column + 6 >= modal.x + modal.width).then_some(ModalHit::Close);
    }
    if row == modal.y + 1 {
        return Some(ModalHit::Search);
    }
    if row < geometry.entries_y || row >= geometry.entries_y + geometry.visible_rows {
        return None;
    }
    let cursor = cursor.min(row_count.saturating_sub(1));
    let start = window_start(row_count, geometry.visible_rows as usize, cursor);
    let index = start + (row - geometry.entries_y) as usize;
    (index < row_count).then_some(ModalHit::Row(index))
}

/// Everything a modal needs beyond its entries.
#[derive(Debug, Clone)]
pub struct ModalSpec {
    pub title: String,
    /// Shown in the search row before anything is typed.
    pub placeholder: &'static str,
    /// `Key:label` pairs for the footer.
    pub hints: String,
    /// Shown in place of the list when there is nothing to list.
    pub empty: String,
}

/// Repaint the reader behind a modal so it reads as background.
pub fn dim_background(frame: &mut Frame<'_>, area: Rect, palette: &Palette) {
    frame
        .buffer_mut()
        .set_style(area, to_tui_style(Style::fg(palette.subtle)));
}

/// Draw a modal and its entries.
pub fn draw_modal(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &ReaderState,
    entries: &[OverlayEntry],
    spec: &ModalSpec,
) {
    let palette = &state.theme.palette;
    dim_background(frame, area, palette);

    let geometry = modal_geometry(area);
    let modal = geometry.area;
    if modal.width < 8 || modal.height < 4 {
        return;
    }
    frame.render_widget(Clear, modal);

    let border = to_tui_style(Style::fg(palette.accent_muted));
    // An opaque surface: the reader behind the modal must not bleed through.
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border)
        .style(TuiStyle::default().bg(crate::style::to_tui_color(palette.background)))
        .title(Span::styled(
            format!(
                " {} ",
                truncate(&spec.title, modal.width.saturating_sub(10) as usize)
            ),
            to_tui_style(Style::fg(palette.accent).bold()),
        ))
        .title_top(Line::from(Span::styled("[×]", border)).right_aligned());
    frame.render_widget(block, modal);

    let inner_width = modal.width.saturating_sub(4);
    let content_x = modal.x + 2;

    draw_search_row(
        frame,
        Rect {
            x: content_x,
            y: modal.y + 1,
            width: inner_width,
            height: 1,
        },
        state,
        spec,
    );
    draw_divider(frame, modal, modal.y + 2, palette);
    draw_divider(frame, modal, modal.y + modal.height - 3, palette);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            truncate(&spec.hints, inner_width as usize),
            to_tui_style(Style::fg(palette.dim)),
        ))),
        Rect {
            x: content_x,
            y: modal.y + modal.height - 2,
            width: inner_width,
            height: 1,
        },
    );

    draw_entries(
        frame,
        Rect {
            x: content_x,
            y: geometry.entries_y,
            width: inner_width,
            height: geometry
                .visible_rows
                .min(modal.y + modal.height - 3 - geometry.entries_y),
        },
        state,
        entries,
        spec,
    );
}

fn draw_search_row(frame: &mut Frame<'_>, area: Rect, state: &ReaderState, spec: &ModalSpec) {
    let palette = &state.theme.palette;
    let search = &state.overlay_search;
    let spans = if search.active || !search.buffer.is_empty() {
        vec![
            Span::styled("/ ", to_tui_style(Style::fg(palette.accent).bold())),
            Span::styled(
                search.buffer.clone(),
                to_tui_style(Style::fg(palette.foreground)),
            ),
            Span::styled(
                if search.active { "▏" } else { "" },
                to_tui_style(Style::fg(palette.accent)),
            ),
        ]
    } else {
        // `subtle` is the colour of text meant to recede behind the modal; a
        // prompt the reader has to read needs `dim`, which every palette keeps
        // legible against its own background.
        vec![Span::styled(
            spec.placeholder.to_owned(),
            to_tui_style(Style::fg(palette.dim)),
        )]
    };
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_divider(frame: &mut Frame<'_>, modal: Rect, y: u16, palette: &Palette) {
    if y <= modal.y || y >= modal.y + modal.height {
        return;
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("├{}┤", "─".repeat(modal.width.saturating_sub(2) as usize)),
            to_tui_style(Style::fg(palette.accent_muted)),
        ))),
        Rect {
            x: modal.x,
            y,
            width: modal.width,
            height: 1,
        },
    );
}

fn draw_entries(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &ReaderState,
    entries: &[OverlayEntry],
    spec: &ModalSpec,
) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let palette = &state.theme.palette;
    if entries.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                spec.empty.clone(),
                to_tui_style(Style::fg(palette.dim)),
            ))),
            area,
        );
        return;
    }

    // One column of the row belongs to the scrollbar, so the text never runs
    // under it.
    let row_width = area.width.saturating_sub(2) as usize;
    let visible = area.height as usize;
    let cursor = state.overlay_cursor.min(entries.len() - 1);
    let start = window_start(entries.len(), visible, cursor);
    let metrics = crate::chrome::scrollbar_metrics(entries.len(), visible, start);

    for (offset, entry) in entries.iter().skip(start).take(visible).enumerate() {
        let index = start + offset;
        let row = Rect {
            x: area.x,
            y: area.y + offset as u16,
            width: area.width,
            height: 1,
        };
        let selected = index == cursor;
        frame.render_widget(
            Paragraph::new(Line::from(entry_spans(entry, row_width, selected, palette))).style(
                if selected {
                    to_tui_style(Style::fg(palette.foreground).with_bg(palette.border))
                } else {
                    TuiStyle::default()
                },
            ),
            Rect {
                width: row_width as u16,
                ..row
            },
        );

        if entries.len() > visible {
            let inside = offset >= metrics.thumb_offset
                && offset < metrics.thumb_offset + metrics.thumb_height;
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    if inside { "█" } else { "│" },
                    to_tui_style(Style::fg(if inside {
                        palette.foreground
                    } else {
                        palette.border
                    })),
                ))),
                Rect {
                    x: area.x + area.width - 1,
                    y: row.y,
                    width: 1,
                    height: 1,
                },
            );
        }
    }
}

/// One row: the label on the left, the detail column right-aligned.
fn entry_spans(
    entry: &OverlayEntry,
    width: usize,
    selected: bool,
    palette: &Palette,
) -> Vec<Span<'static>> {
    let header = entry.style == EntryStyle::Header;
    let indent = if header { "" } else { "  " };
    let detail_width = entry.detail.chars().count().min(width.saturating_sub(4));
    let detail = truncate(&entry.detail, detail_width);
    let label_width = width
        .saturating_sub(indent.len())
        .saturating_sub(detail.chars().count())
        .saturating_sub(usize::from(!detail.is_empty()));
    let label = pad(&entry.display, label_width);

    let label_style = if selected {
        Style::fg(palette.foreground).bold()
    } else {
        match entry.style {
            EntryStyle::Header => Style::fg(palette.foreground).bold(),
            EntryStyle::Normal => Style::fg(palette.foreground),
            EntryStyle::Muted => Style::fg(palette.dim),
        }
    };

    let mut spans = vec![
        Span::raw(indent),
        Span::styled(label, to_tui_style(label_style)),
    ];
    if !detail.is_empty() {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            detail,
            to_tui_style(if selected {
                Style::fg(palette.foreground).bold()
            } else {
                Style::fg(palette.accent_muted).bold()
            }),
        ));
    }
    spans
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    use super::{ModalHit, hit_test, modal_geometry};

    fn frame(width: u16, height: u16) -> Rect {
        Rect {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    #[test]
    fn a_modal_is_centred_and_fits_inside_the_frame() {
        for (width, height) in [(80, 24), (120, 40), (40, 12), (20, 8)] {
            let geometry = modal_geometry(frame(width, height));
            let modal = geometry.area;
            assert!(
                modal.x + modal.width <= width,
                "{width}x{height}: {modal:?}"
            );
            assert!(
                modal.y + modal.height <= height,
                "{width}x{height}: {modal:?}"
            );
            assert!(geometry.visible_rows >= 1);
        }
    }

    #[test]
    fn the_title_bar_close_button_and_search_row_are_hit_testable() {
        let area = frame(100, 30);
        let geometry = modal_geometry(area);
        let modal = geometry.area;

        assert_eq!(
            hit_test(area, 10, 0, modal.x + modal.width - 2, modal.y),
            Some(ModalHit::Close)
        );
        assert_eq!(
            hit_test(area, 10, 0, modal.x + 4, modal.y),
            None,
            "the title itself is not a button"
        );
        assert_eq!(
            hit_test(area, 10, 0, modal.x + 4, modal.y + 1),
            Some(ModalHit::Search)
        );
    }

    #[test]
    fn a_click_lands_on_the_row_the_reader_can_see() {
        let area = frame(100, 30);
        let geometry = modal_geometry(area);

        assert_eq!(
            hit_test(area, 5, 0, geometry.area.x + 4, geometry.entries_y),
            Some(ModalHit::Row(0))
        );
        assert_eq!(
            hit_test(area, 5, 0, geometry.area.x + 4, geometry.entries_y + 2),
            Some(ModalHit::Row(2))
        );
        assert_eq!(
            hit_test(area, 5, 0, geometry.area.x + 4, geometry.entries_y + 9),
            None,
            "past the last entry there is nothing to act on"
        );
    }

    #[test]
    fn a_click_on_a_scrolled_list_follows_the_window() {
        let area = frame(100, 30);
        let geometry = modal_geometry(area);
        let rows = 200;
        let cursor = 120;
        let start = crate::chrome::window_start(rows, geometry.visible_rows as usize, cursor);

        assert_eq!(
            hit_test(area, rows, cursor, geometry.area.x + 4, geometry.entries_y),
            Some(ModalHit::Row(start))
        );
    }

    #[test]
    fn clicks_outside_the_modal_hit_nothing() {
        let area = frame(100, 30);
        let geometry = modal_geometry(area);
        assert_eq!(hit_test(area, 5, 0, 0, 0), None);
        assert_eq!(
            hit_test(
                area,
                5,
                0,
                geometry.area.x + geometry.area.width,
                geometry.entries_y
            ),
            None
        );
    }
}

//! The command palette: what `/` opens.
//!
//! Typing a command is a choice, not a guess, so the bar shows the choices while
//! they are still being made — the matching commands, their category, their
//! usage, and what they do — with the typed line underneath, inside the same box.
//! One geometry serves the list, the cursor, and the scrollbar, which is what
//! keeps the highlighted row and the row Tab completes the same row.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style as TuiStyle;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use reader_app::ReaderState;
use reader_core::Palette;
use reader_core::command::Suggestion;
use reader_core::style::Style;

use crate::chrome::{pad, scrollbar_metrics, truncate, window_start};
use crate::frame::CommandBar;
use crate::style::{to_tui_color, to_tui_style};

/// Suggestions shown at once. Beyond this the list scrolls with the cursor.
pub const MAX_ROWS: usize = 8;

/// Rows the box spends on its own chrome: two borders, the divider, the input.
const CHROME_ROWS: u16 = 4;

/// Columns the category column occupies.
const CATEGORY_WIDTH: usize = 12;

/// Where the palette sits inside the reading area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteGeometry {
    /// The bordered box, when the frame has room for one.
    pub area: Option<Rect>,
    /// Rows the suggestion list occupies; height 0 when nothing fits.
    pub list: Rect,
    /// The row the typed line is drawn on.
    pub input: Rect,
}

/// The commands matching what has been typed so far.
#[must_use]
pub fn suggestions(command_bar: &CommandBar) -> Vec<Suggestion> {
    reader_core::command::list_command_suggestions(&command_bar.buffer, command_bar.cursor, None)
}

/// Lay the palette out against the reading area, anchored to its bottom.
///
/// `rows` is how many suggestions there are; the box grows with the list up to
/// [`MAX_ROWS`] and shrinks with the frame, and gives the list up before it
/// gives up the typed line.
#[must_use]
pub fn geometry(body: Rect, rows: usize) -> Option<PaletteGeometry> {
    if body.height == 0 || body.width < 12 {
        return None;
    }

    // Without room for a box, the typed line still gets the last row.
    if body.height < CHROME_ROWS + 1 {
        let input = Rect {
            x: body.x,
            y: body.y + body.height - 1,
            width: body.width,
            height: 1,
        };
        return Some(PaletteGeometry {
            area: None,
            list: Rect { height: 0, ..input },
            input,
        });
    }

    let width = body.width - 2;
    let list_rows = rows
        .clamp(1, MAX_ROWS)
        .min((body.height - CHROME_ROWS) as usize) as u16;
    let height = list_rows + CHROME_ROWS;
    let area = Rect {
        x: body.x + 1,
        y: body.y + body.height - height,
        width,
        height,
    };

    Some(PaletteGeometry {
        area: Some(area),
        // A column of margin on the left, and the scrollbar against the border
        // on the right.
        list: Rect {
            x: area.x + 2,
            y: area.y + 1,
            width: width - 3,
            height: list_rows,
        },
        input: Rect {
            x: area.x + 2,
            y: area.y + list_rows + 2,
            width: width - 4,
            height: 1,
        },
    })
}

/// Draw the palette over the reading area.
pub fn draw(
    frame: &mut Frame<'_>,
    body: Rect,
    state: &ReaderState,
    command_bar: &CommandBar,
    suggestions: &[Suggestion],
) -> Option<PaletteGeometry> {
    let geometry = geometry(body, suggestions.len())?;
    let palette = &state.theme.palette;

    if let Some(area) = geometry.area {
        // The box is inset by a column, and the reading text behind it would
        // otherwise show through that margin as loose letters.
        let band = Rect {
            x: body.x,
            y: area.y,
            width: body.width,
            height: area.height,
        };
        frame.render_widget(Clear, band);
        frame.render_widget(
            Block::default().style(TuiStyle::default().bg(to_tui_color(palette.background))),
            band,
        );
        frame.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(to_tui_style(Style::fg(palette.accent_muted)))
                .style(TuiStyle::default().bg(to_tui_color(palette.background))),
            area,
        );
        draw_divider(frame, area, area.y + geometry.list.height + 1, palette);
    }

    draw_list(frame, geometry.list, command_bar, suggestions, palette);
    draw_input(frame, geometry.input, command_bar, palette);
    Some(geometry)
}

fn draw_divider(frame: &mut Frame<'_>, area: Rect, y: u16, palette: &Palette) {
    if y <= area.y || y + 1 >= area.y + area.height {
        return;
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("├{}┤", "─".repeat(area.width.saturating_sub(2) as usize)),
            to_tui_style(Style::fg(palette.accent_muted)),
        ))),
        Rect {
            x: area.x,
            y,
            width: area.width,
            height: 1,
        },
    );
}

fn draw_list(
    frame: &mut Frame<'_>,
    area: Rect,
    command_bar: &CommandBar,
    suggestions: &[Suggestion],
    palette: &Palette,
) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    if suggestions.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                " No command matches that.",
                to_tui_style(Style::fg(palette.dim)),
            ))),
            area,
        );
        return;
    }

    // The last column belongs to the scrollbar, so no row runs under it.
    let row_width = area.width.saturating_sub(1) as usize;
    let visible = area.height as usize;
    let cursor = command_bar.selected.min(suggestions.len() - 1);
    let start = window_start(suggestions.len(), visible, cursor);
    let metrics = scrollbar_metrics(suggestions.len(), visible, start);

    for (offset, suggestion) in suggestions.iter().skip(start).take(visible).enumerate() {
        let selected = start + offset == cursor;
        let y = area.y + offset as u16;
        frame.render_widget(
            Paragraph::new(Line::from(row_spans(
                suggestion, row_width, selected, palette,
            )))
            .style(if selected {
                to_tui_style(Style::fg(palette.foreground).with_bg(palette.border))
            } else {
                TuiStyle::default()
            }),
            Rect {
                x: area.x,
                y,
                width: row_width as u16,
                height: 1,
            },
        );

        if suggestions.len() > visible {
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
                    y,
                    width: 1,
                    height: 1,
                },
            );
        }
    }
}

/// One row: category, usage, and what the command does.
///
/// Only the selected row carries the trailing detail — the aliases, the flags,
/// an example — because that is the row the reader is deciding about.
fn row_spans(
    suggestion: &Suggestion,
    width: usize,
    selected: bool,
    palette: &Palette,
) -> Vec<Span<'static>> {
    let category_width = CATEGORY_WIDTH.min(width);
    let usage_width = (width.saturating_sub(category_width) / 3).clamp(0, 40);
    let description_width = width
        .saturating_sub(category_width)
        .saturating_sub(usage_width)
        .saturating_sub(1);

    let usage = match &suggestion.matched_alias {
        // What was typed is what should be shown: an alias completes to itself.
        Some(alias) => {
            suggestion
                .usage
                .replacen(&format!("/{}", suggestion.name), &format!("/{alias}"), 1)
        }
        None => suggestion.usage.clone(),
    };
    let description = if selected && !suggestion.detail.is_empty() {
        format!("{} ({})", suggestion.description, suggestion.detail)
    } else {
        suggestion.description.clone()
    };

    vec![
        Span::styled(
            pad(suggestion.category, category_width),
            to_tui_style(Style::fg(palette.dim)),
        ),
        Span::styled(
            pad(&usage, usage_width),
            to_tui_style(if selected {
                Style::fg(palette.accent).bold()
            } else {
                Style::fg(palette.foreground)
            }),
        ),
        Span::raw(" "),
        Span::styled(
            truncate(&description, description_width),
            to_tui_style(if selected {
                Style::fg(palette.foreground)
            } else {
                Style::fg(palette.dim)
            }),
        ),
    ]
}

/// The typed line, with the caret sitting on the character it would replace.
fn draw_input(frame: &mut Frame<'_>, area: Rect, command_bar: &CommandBar, palette: &Palette) {
    if area.width == 0 {
        return;
    }
    let characters: Vec<char> = command_bar.buffer.chars().collect();
    let cursor = command_bar.cursor.min(characters.len());
    let before: String = characters[..cursor].iter().collect();
    let under: String = characters.get(cursor).copied().into_iter().collect();
    let after: String = characters[cursor.saturating_add(1).min(characters.len())..]
        .iter()
        .collect();

    let mut spans = vec![
        Span::styled("/", to_tui_style(Style::fg(palette.accent).bold())),
        Span::styled(before, to_tui_style(Style::fg(palette.foreground))),
    ];
    if under.is_empty() {
        spans.push(Span::styled("█", to_tui_style(Style::fg(palette.accent))));
    } else {
        spans.push(Span::styled(
            under,
            to_tui_style(Style::fg(palette.background).with_bg(palette.accent)),
        ));
        spans.push(Span::styled(
            after,
            to_tui_style(Style::fg(palette.foreground)),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    use super::{MAX_ROWS, geometry};

    fn body(width: u16, height: u16) -> Rect {
        Rect {
            x: 0,
            y: 1,
            width,
            height,
        }
    }

    #[test]
    fn the_palette_sits_at_the_bottom_of_the_reading_area() {
        let area = body(80, 20);
        let geometry = geometry(area, 5).expect("a palette");
        let palette = geometry.area.expect("a box");

        assert_eq!(
            palette.y + palette.height,
            area.y + area.height,
            "the box closes where the reading area does"
        );
        assert_eq!(
            palette.height,
            5 + 4,
            "five rows, two borders, divider, input"
        );
        assert_eq!(geometry.list.height, 5);
        assert_eq!(
            geometry.input.y,
            palette.y + palette.height - 2,
            "the typed line sits just above the closing border"
        );
    }

    #[test]
    fn a_long_list_stops_growing_and_a_short_frame_shrinks_it() {
        let tall = geometry(body(80, 40), 60).expect("a palette");
        assert_eq!(tall.list.height as usize, MAX_ROWS);

        let short = geometry(body(80, 7), 60).expect("a palette");
        assert_eq!(short.list.height, 3, "only what is left after the chrome");
        let area = short.area.expect("a box");
        assert!(area.y >= 1 && area.y + area.height <= 8, "{area:?}");
    }

    #[test]
    fn an_empty_list_still_reserves_a_row_to_say_so() {
        let geometry = geometry(body(80, 20), 0).expect("a palette");
        assert_eq!(geometry.list.height, 1);
    }

    #[test]
    fn a_frame_with_no_room_for_a_box_still_takes_the_typed_line() {
        let cramped = geometry(body(80, 3), 5).expect("a palette");
        assert!(cramped.area.is_none());
        assert_eq!(cramped.list.height, 0);
        assert_eq!(cramped.input.y, 3, "the last row of the reading area");

        assert!(geometry(body(8, 20), 5).is_none(), "too narrow to draw");
        assert!(geometry(body(80, 0), 5).is_none(), "no reading area at all");
    }
}

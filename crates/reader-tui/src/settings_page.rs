//! The tabbed settings page.
//!
//! Settings are not a list: they are four tabs, a search, the controls of the
//! open tab, and a live preview of what the draft would look like. The preview
//! is the point — no description of "relaxed line spacing" is as useful as
//! seeing it — so it is rendered with the same renderer the reading column uses,
//! against the draft rather than against what is saved.
//!
//! Nothing here mutates state: the draft already lives in
//! [`reader_app::ReaderState::settings`], and Enter or Esc decide its fate.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style as TuiStyle;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use reader_app::{OverlayEntry, ReaderState};
use reader_core::render::{RenderOptions, render_blocks};
use reader_core::style::Style;
use reader_core::{CanonicalBlock, LineSpacing, Palette, SettingsTab};

use crate::chrome::{pad, truncate};
use crate::style::{to_tui_color, to_tui_line, to_tui_style};

/// The sample the preview renders, so every mode shows the same sentence.
const PREVIEW_TEXT: &str = "A quiet chapter begins here.";
const PREVIEW_TEXT_SECOND: &str = "The next sentence follows softly.";

/// Rows the page needs before it will show a preview at all.
const PREVIEW_MIN_ROOM: u16 = 8;

/// Column the page's content starts at, inside `area`.
const CONTENT_X: u16 = 1;
/// Row of the tab strip, relative to the top of the page.
const TAB_ROW: u16 = 2;
/// Row of the search input, relative to the top of the page.
const SEARCH_ROW: u16 = 5;
/// Row of the first setting, relative to the top of the page.
const FIRST_SETTING_ROW: u16 = 8;

/// What a pointer landed on inside the settings page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsHit {
    /// A tab in the strip.
    Tab(SettingsTab),
    /// The search input.
    Search,
    /// A setting, by its index in the visible list.
    Row(usize),
}

/// Which part of the page covers `(column, row)`, if any.
#[must_use]
pub fn hit_test(area: Rect, row_count: usize, column: u16, row: u16) -> Option<SettingsHit> {
    if column < area.x + CONTENT_X || column >= area.x + area.width {
        return None;
    }
    let offset = row.checked_sub(area.y)?;
    if offset == TAB_ROW {
        return tab_at(area.x + CONTENT_X, column);
    }
    if offset == SEARCH_ROW {
        return Some(SettingsHit::Search);
    }
    let index = usize::from(offset.checked_sub(FIRST_SETTING_ROW)?);
    (index < row_count).then_some(SettingsHit::Row(index))
}

/// The tab whose label covers `column`, given where the strip starts.
fn tab_at(start: u16, column: u16) -> Option<SettingsHit> {
    let mut cursor = start;
    for tab in SettingsTab::ALL {
        // Each label is drawn as " Label ", with two spaces between tabs.
        let width = tab.label().len() as u16 + 2;
        if column >= cursor && column < cursor + width {
            return Some(SettingsHit::Tab(tab));
        }
        cursor += width + 2;
    }
    None
}

/// Draw the settings page over `area`.
pub fn draw(frame: &mut Frame<'_>, area: Rect, state: &ReaderState, entries: &[OverlayEntry]) {
    if area.width < 8 || area.height < 4 {
        return;
    }
    let palette = &state.theme.palette;
    // An opaque surface: the reader behind must not show through the page.
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default().style(TuiStyle::default().bg(to_tui_color(palette.background))),
        area,
    );

    let width = area.width as usize;
    // The page is drawn one column in, and the boxes add two glyphs either side,
    // so the usable interior is six columns narrower than the frame.
    let inner = width.saturating_sub(6).max(10);
    let mut lines: Vec<Line<'static>> = vec![
        Line::from(vec![
            Span::styled(
                " Aa ",
                to_tui_style(Style::fg(palette.background).with_bg(palette.accent).bold()),
            ),
            Span::raw(" "),
            Span::styled(
                "Reader settings",
                to_tui_style(Style::fg(palette.foreground).bold()),
            ),
        ]),
        Line::default(),
        tab_bar(state, palette),
        Line::default(),
    ];
    lines.extend(search_box(state, palette, inner));
    lines.push(Line::default());

    let cursor = state.overlay_cursor.min(entries.len().saturating_sub(1));
    if entries.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No settings match your search.",
            to_tui_style(Style::fg(palette.dim)),
        )));
    } else {
        let label_width = (inner * 34 / 100).clamp(18, 28);
        for (index, entry) in entries.iter().enumerate() {
            let selected = index == cursor;
            let colour = if selected {
                palette.accent
            } else {
                palette.foreground
            };
            lines.push(Line::from(vec![
                Span::styled(
                    if selected { "› " } else { "  " },
                    to_tui_style(Style::fg(palette.accent).bold()),
                ),
                Span::styled(
                    pad(&entry.display, label_width),
                    to_tui_style(Style::fg(colour)),
                ),
                Span::raw(" "),
                Span::styled(
                    truncate(&entry.detail, inner.saturating_sub(label_width + 3)),
                    to_tui_style(Style::fg(colour).bold()),
                ),
            ]));
        }
        // Only the selected setting explains itself; eleven descriptions at once
        // would bury the values they belong to.
        if let Some(description) = selected_description(state, cursor) {
            lines.push(Line::from(Span::styled(
                format!("    {}", truncate(description, inner.saturating_sub(4))),
                to_tui_style(Style::fg(palette.dim)),
            )));
        }
    }

    let hint = Line::from(Span::styled(
        truncate(
            "←/→ tab · ↑/↓ select · Space change · Enter save · / search · Esc cancel",
            width,
        ),
        to_tui_style(Style::fg(palette.dim)),
    ));

    let preview = preview_lines(state, inner);
    let room = (area.height as usize).saturating_sub(lines.len() + 2);
    if room > preview.len() && area.height >= PREVIEW_MIN_ROOM {
        lines.push(Line::default());
        lines.extend(preview);
    }
    lines.push(Line::default());
    lines.push(hint);
    lines.truncate(area.height as usize);

    frame.render_widget(
        Paragraph::new(lines),
        Rect {
            x: area.x + 1,
            y: area.y,
            width: area.width.saturating_sub(1),
            height: area.height,
        },
    );
}

/// The description of the setting under the cursor, matched by label.
fn selected_description(state: &ReaderState, cursor: usize) -> Option<&'static str> {
    reader_app::settings_panel::rows(state)
        .get(cursor)
        .map(|row| row.description)
}

fn tab_bar(state: &ReaderState, palette: &Palette) -> Line<'static> {
    let mut spans = Vec::new();
    for tab in SettingsTab::ALL {
        if !spans.is_empty() {
            spans.push(Span::raw("  "));
        }
        let active = tab == state.settings_tab;
        spans.push(Span::styled(
            format!(" {} ", tab.label()),
            to_tui_style(if active {
                Style::fg(palette.background).with_bg(palette.accent).bold()
            } else {
                Style::fg(palette.dim)
            }),
        ));
    }
    Line::from(spans)
}

fn search_box(state: &ReaderState, palette: &Palette, inner: usize) -> Vec<Line<'static>> {
    let border = to_tui_style(Style::fg(palette.border));
    let search = &state.overlay_search;
    let body = if search.active || !search.buffer.is_empty() {
        vec![
            Span::styled("⌕ ", to_tui_style(Style::fg(palette.accent))),
            Span::styled(
                pad(&search.buffer, inner.saturating_sub(2)),
                to_tui_style(Style::fg(palette.foreground)),
            ),
        ]
    } else {
        vec![
            Span::styled("⌕ ", to_tui_style(Style::fg(palette.dim))),
            // `dim`, not `subtle`: a prompt has to be readable on every palette.
            Span::styled(
                pad("Search settings...", inner.saturating_sub(2)),
                to_tui_style(Style::fg(palette.dim)),
            ),
        ]
    };
    let rule = |left: &str, right: &str| {
        Line::from(Span::styled(
            format!("{left}{}{right}", "─".repeat(inner + 2)),
            border,
        ))
    };
    let mut middle = vec![Span::styled("│ ", border)];
    middle.extend(body);
    middle.push(Span::styled(" │", border));
    vec![rule("╭", "╮"), Line::from(middle), rule("╰", "╯")]
}

/// The draft, rendered by the reading renderer inside a small box.
fn preview_lines(state: &ReaderState, inner: usize) -> Vec<Line<'static>> {
    let settings = &state.settings;
    let palette = &state.theme.palette;
    let border = to_tui_style(Style::fg(palette.border));

    // Margins and font scale narrow the preview exactly as they narrow the page.
    let margin = usize::from(settings.margin_size).min(inner.saturating_sub(12) / 2);
    let inside_margins = inner.saturating_sub(margin * 2).max(1);
    let text_width =
        ((inside_margins as f64 / settings.font_scale.clamp(1.0, 2.0)).floor() as usize).max(1);
    let padding = (inner.saturating_sub(text_width)) / 2;

    let blocks = [
        CanonicalBlock::Paragraph {
            id: "preview-1".into(),
            text: PREVIEW_TEXT.into(),
        },
        CanonicalBlock::Paragraph {
            id: "preview-2".into(),
            text: PREVIEW_TEXT_SECOND.into(),
        },
    ];
    let options = RenderOptions {
        mode: settings.render_mode,
        width: text_width,
        palette,
        code_language: settings.code_language,
        code_density: settings.code_density,
        plain_highlight: settings.plain_highlight,
        line_spacing: settings.line_spacing,
        block_index_offset: 0,
        include_trailing_spacing: false,
        search_query: None,
    };

    let meta = format!(
        "{} text · {} · {} spacing",
        font_scale_label(settings.font_scale),
        if settings.margin_size == 0 {
            "No margins".to_owned()
        } else {
            format!("{}-column margins", settings.margin_size)
        },
        line_spacing_label(settings.line_spacing)
    );

    let mut rows: Vec<Line<'static>> = Vec::new();
    for rendered in render_blocks(&blocks, &options) {
        let line = to_tui_line(&rendered);
        let text_length: usize = line
            .spans
            .iter()
            .map(|span| span.content.chars().count())
            .sum();
        let mut spans = vec![Span::styled("│ ", border), Span::raw(" ".repeat(padding))];
        spans.extend(
            line.spans
                .into_iter()
                .map(|span| Span::styled(span.content.into_owned(), span.style)),
        );
        spans.push(Span::raw(
            " ".repeat(inner.saturating_sub(padding + text_length)),
        ));
        spans.push(Span::styled(" │", border));
        rows.push(Line::from(spans));
    }
    rows.push(Line::from(vec![
        Span::styled("│ ", border),
        Span::styled(pad(&meta, inner), to_tui_style(Style::fg(palette.dim))),
        Span::styled(" │", border),
    ]));

    let rule = |left: &str, right: &str| {
        Line::from(Span::styled(
            format!("{left}{}{right}", "─".repeat(inner + 2)),
            border,
        ))
    };
    let mut lines = vec![
        Line::from(Span::styled(
            "Preview",
            to_tui_style(Style::fg(palette.accent).bold()),
        )),
        rule("╭", "╮"),
    ];
    lines.extend(rows);
    lines.push(rule("╰", "╯"));
    lines
}

fn font_scale_label(scale: f64) -> &'static str {
    if (scale - 1.0).abs() < f64::EPSILON {
        "Standard"
    } else if (scale - 1.15).abs() < f64::EPSILON {
        "Medium"
    } else if (scale - 1.3).abs() < f64::EPSILON {
        "Large"
    } else {
        "Extra large"
    }
}

fn line_spacing_label(spacing: LineSpacing) -> &'static str {
    match spacing {
        LineSpacing::Compact => "Compact",
        LineSpacing::Normal => "Normal",
        LineSpacing::Relaxed => "Relaxed",
    }
}

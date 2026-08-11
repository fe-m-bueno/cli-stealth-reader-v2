//! Composing one frame.
//!
//! Drawing reads state and never changes it, so a frame can be rendered into a
//! test buffer and asserted line by line. The reading column, the footer, and any
//! overlay are laid out from the same geometry the executor used, which is what
//! keeps scroll bounds and what is on screen in agreement.

use std::borrow::Cow;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style as TuiStyle;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use reader_app::{Overlay, OverlayEntry, ReaderState};
use reader_core::pace::{EstimateScope, format_time_left};
use reader_core::style::Style;
use reader_core::{ProgressVisibility, RenderMode};

use crate::chrome::{RuleSide, hints, rule, scrollbar_metrics, truncate};
use crate::style::{to_tui_line, to_tui_style};

/// Columns a progress bar occupies in the metadata row.
const PROGRESS_BAR_WIDTH: usize = 12;

/// What the command bar is doing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandBar {
    pub active: bool,
    pub buffer: String,
    /// Cursor position in characters.
    pub cursor: usize,
    /// Which suggestion the palette highlights, and which one Tab completes.
    pub selected: usize,
    /// A running Toggl timer, when one is known. The integration owns the text;
    /// the footer only places it.
    pub timer: Option<String>,
}

/// Rows below the closing rule: the reading metadata whenever a book is open.
///
/// The command palette is not counted: it is drawn over the reading area rather
/// than under the rule, so opening it does not reflow the text behind it.
///
/// The two rules themselves are not counted here — [`reader_app::compute_layout`]
/// already reserves them — so this stays the number the layout needs.
#[must_use]
pub fn footer_height(state: &ReaderState, _command_bar: &CommandBar) -> u16 {
    u16::from(state.current_book.is_some())
}

/// Where each part of the frame ends up.
///
/// Drawing and pointer handling both work from this, so a click is tested
/// against the rectangle that was actually painted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameGeometry {
    pub header: Option<Rect>,
    pub body: Rect,
    pub status: Option<Rect>,
    /// The typed line of the command palette, while it is open.
    pub command: Option<Rect>,
    pub metadata: Option<Rect>,
    /// Column the scrollbar occupies, when one is drawn.
    pub scrollbar_column: Option<u16>,
}

/// Lay the frame out for `area`.
#[must_use]
pub fn geometry(area: Rect, state: &ReaderState, command_bar: &CommandBar) -> FrameGeometry {
    let footer_rows = footer_height(state, command_bar);
    let layout = state.layout(footer_rows);

    // The rules take a row each; whatever is left between them is reading area.
    let body_rows = area.height.saturating_sub(footer_rows.saturating_add(2));
    let mut cursor = area.y;

    let header = take_row(area, &mut cursor);
    let body = Rect {
        x: area.x,
        y: cursor,
        width: area.width,
        height: body_rows.min(area.height.saturating_sub(cursor - area.y)),
    };
    cursor += body.height;
    let status = take_row(area, &mut cursor);
    // The palette is drawn over the reading area, so its typed line comes from
    // the body rather than from a row of the footer.
    let command = command_bar.active.then(|| {
        crate::palette::geometry(body, crate::palette::suggestions(command_bar).len())
            .map(|palette| palette.input)
    });
    let command = command.flatten();
    let metadata = state
        .current_book
        .is_some()
        .then(|| take_row(area, &mut cursor))
        .flatten();

    FrameGeometry {
        header,
        body,
        status,
        command,
        metadata,
        scrollbar_column: (layout.scrollbar_width > 0 && body.width > 0)
            .then(|| body.x + body.width - 1),
    }
}

/// Draw the whole frame.
///
/// `overlay_entries` are supplied by the caller because building them needs the
/// database, and drawing must not touch it: the same list the frame shows is the
/// one the cursor indexes and confirming acts on.
pub fn draw(
    frame: &mut Frame<'_>,
    state: &mut ReaderState,
    command_bar: &CommandBar,
    overlay_entries: &[OverlayEntry],
) {
    let area = frame.area();
    state.viewport = reader_app::Viewport::new(area.width, area.height);
    let layout = state.layout(footer_height(state, command_bar));
    let FrameGeometry {
        header: header_area,
        body: body_area,
        status: status_area,
        metadata: metadata_area,
        ..
    } = geometry(area, state, command_bar);

    let visible = draw_body(frame, body_area, state, &layout);

    if let Some(row) = header_area {
        let left = header_left(state);
        let right = header_right(state);
        frame.render_widget(
            Paragraph::new(rule(
                RuleSide::Top,
                row.width,
                &left,
                &right,
                &state.theme.palette,
            )),
            row,
        );
    }
    if let Some(row) = status_area {
        draw_status_rule(frame, row, state, command_bar);
    }
    if let Some(row) = metadata_area {
        draw_metadata(frame, row, state, &layout, visible);
    }

    if state.overlay != Overlay::None {
        draw_overlay(frame, body_area, state, overlay_entries);
    }

    // The palette is the thing being typed into, so nothing draws over it.
    if command_bar.active {
        let suggestions = crate::palette::suggestions(command_bar);
        crate::palette::draw(frame, body_area, state, command_bar, &suggestions);
    }
}

/// Take the next single row, or nothing when the frame has run out.
fn take_row(area: Rect, cursor: &mut u16) -> Option<Rect> {
    if *cursor >= area.y + area.height {
        return None;
    }
    let row = Rect {
        x: area.x,
        y: *cursor,
        width: area.width,
        height: 1,
    };
    *cursor += 1;
    Some(row)
}

/// What the reading column currently shows, so the metadata row can say so.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct VisibleWindow {
    offset: usize,
    rows: usize,
    total: usize,
}

/// The book, chapter, and search context, for the opening rule.
fn header_left(state: &ReaderState) -> String {
    let Some(book) = state.current_book.as_ref() else {
        return "stealth-reader".to_owned();
    };
    let mut left = format!(
        "{} · Ch {}/{}",
        truncate(&book.title, 34),
        state.chapter_index + 1,
        book.chapters.len()
    );
    if let Some(chapter) = state.chapter() {
        left.push_str(" · ");
        left.push_str(&truncate(&chapter.title, 28));
    }
    if let Some(search) = state.search.as_ref() {
        left.push_str(&format!(
            " · [{}/{}] \"{}\"",
            search.cursor + 1,
            search.results.len(),
            truncate(&search.query, 20)
        ));
    }
    left
}

/// The render mode, density, focus state, and theme, for the opening rule.
fn header_right(state: &ReaderState) -> String {
    let settings = &state.settings;
    let mut right = match settings.render_mode {
        RenderMode::Plain => "plain".to_owned(),
        RenderMode::Code => settings.code_language.as_str().to_owned(),
    };
    // Density only means something while code is on screen.
    if settings.render_mode == RenderMode::Code {
        right.push_str(&format!(" · density:{}", settings.code_density.get()));
    }
    if state.focus_mode {
        right.push_str(&format!(" · focus §{}", state.focus_block_index + 1));
    }
    right.push_str(&format!(
        " · {} · {}",
        settings.theme_id.label(),
        settings.appearance_theme_id.label()
    ));
    right
}

/// The keys that apply right now, for the closing rule.
fn footer_hints(state: &ReaderState, command_bar: &CommandBar) -> String {
    // While the palette is open, the keys that matter are the palette's own.
    if command_bar.active {
        return hints(&[
            ("↑/↓", "nav"),
            ("Tab", "complete"),
            ("Enter", "run"),
            ("Esc", "close"),
        ]);
    }
    match state.overlay {
        Overlay::Keys if state.overlay_search.active => {
            hints(&[("Esc", "exit search"), ("Esc Esc", "close")])
        }
        Overlay::Keys => hints(&[
            ("Enter/click", "run"),
            ("/", "search"),
            ("z", "fold all"),
            ("Esc", "close"),
        ]),
        Overlay::None if state.focus_mode => hints(&[
            ("j/k", "block"),
            ("Esc", "exit focus"),
            ("/", "commands"),
            ("q", "quit"),
        ]),
        Overlay::None => hints(&[("/", "commands"), ("Ctrl+.", "shortcuts"), ("q", "quit")]),
        _ => hints(&[
            ("Esc", "close"),
            ("/", "commands"),
            ("Ctrl+.", "shortcuts"),
            ("q", "quit"),
        ]),
    }
}

fn draw_body(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &mut ReaderState,
    layout: &reader_app::ViewportLayout,
) -> VisibleWindow {
    if area.height == 0 || area.width == 0 {
        return VisibleWindow::default();
    }

    let Some(book) = state.current_book.as_ref() else {
        let hint = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "  No book open. Press / then type add to import one, or resume to continue.",
                to_tui_style(Style::fg(state.theme.palette.dim)),
            )),
        ]);
        frame.render_widget(hint, area);
        return VisibleWindow::default();
    };

    if book.chapters.get(state.chapter_index).is_none() {
        return VisibleWindow::default();
    }

    // The chapter is rendered once per change, not once per frame; only the
    // visible slice is converted to ratatui lines.
    let body_height = area.height as usize;
    let block_offset = state.block_offset;
    // Both are read before the chapter is borrowed for rendering.
    let focus_span = if state.focus_mode {
        state.focus_block_span(layout.content_width)
    } else {
        None
    };
    let dim_style = to_tui_style(Style::fg(state.theme.palette.dim));

    let lines = state.chapter_lines(layout.content_width);
    let line_count = lines.len();
    let max_offset = line_count.saturating_sub(body_height);
    // Focus mode puts the block it is on in the middle of the column, so the
    // reader's eye stays in one place as blocks step past it.
    let offset = match focus_span {
        Some((start, length)) => centred_offset(start, length, body_height).min(max_offset),
        None => block_offset.min(max_offset),
    };
    let visible: Vec<Line<'_>> = lines
        .iter()
        .enumerate()
        .skip(offset)
        .take(body_height)
        .map(|(index, line)| match focus_span {
            // Everything but the focused block is dimmed rather than hidden, so
            // the block keeps the context it belongs to.
            Some((start, length)) if index < start || index >= start + length => {
                dimmed_line(line, dim_style)
            }
            _ => to_tui_line(line),
        })
        .collect();

    let padding = layout.content_padding;
    let text_area = Rect {
        x: area.x + padding,
        y: area.y,
        width: area.width.saturating_sub(padding + layout.scrollbar_width),
        height: area.height,
    };
    frame.render_widget(Paragraph::new(visible), text_area);

    if layout.scrollbar_width > 0 {
        draw_scrollbar(frame, area, state, line_count, offset);
    }

    VisibleWindow {
        offset,
        rows: body_height.min(line_count),
        total: line_count,
    }
}

/// The scroll offset that puts a block of `length` lines in the middle.
///
/// A block taller than the column starts at its top instead: showing its middle
/// would cut off the beginning of what the reader is meant to be reading.
const fn centred_offset(start: usize, length: usize, body_height: usize) -> usize {
    start.saturating_sub(body_height.saturating_sub(length) / 2)
}

/// The same line, drawn in the palette's dim colour.
fn dimmed_line(line: &reader_core::style::StyledLine, dim: TuiStyle) -> Line<'_> {
    Line::from(
        line.spans
            .iter()
            .map(|span| Span::styled(span.text.as_str(), dim))
            .collect::<Vec<_>>(),
    )
}

/// A one-column scrollbar whose thumb length reflects how much is visible.
///
/// It sits in the reading column's last cell rather than on a border, so the
/// text edge and the bar stay aligned however wide the margins are.
fn draw_scrollbar(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &ReaderState,
    total_lines: usize,
    offset: usize,
) {
    let height = area.height as usize;
    if height == 0 || total_lines <= height {
        return;
    }
    let track_style = to_tui_style(Style::fg(state.theme.palette.border));
    let thumb_style = to_tui_style(Style::fg(state.theme.palette.accent_muted));
    let metrics = scrollbar_metrics(total_lines, height, offset);

    let column = area.x + area.width - 1;
    let rows: Vec<Line<'static>> = (0..height)
        .map(|row| {
            let inside =
                row >= metrics.thumb_offset && row < metrics.thumb_offset + metrics.thumb_height;
            Line::from(Span::styled(
                if inside { "█" } else { "│" },
                if inside { thumb_style } else { track_style },
            ))
        })
        .collect();
    frame.render_widget(
        Paragraph::new(rows),
        Rect {
            x: column,
            y: area.y,
            width: 1,
            height: area.height,
        },
    );
}

/// The reading progress text for the footer.
#[must_use]
pub fn progress_text(state: &mut ReaderState, content_width: u16, body_height: u16) -> String {
    let visibility = state.settings.progress_visibility;
    if visibility == ProgressVisibility::Hidden {
        return String::new();
    }
    let Some(book) = state.current_book.as_ref() else {
        return String::new();
    };
    let chapter_index = state.chapter_index;
    let chapter_count = book.chapters.len();
    // Time estimates only need the current chapter and, for book scope, a sum
    // of later chapters. Keep those as scalars instead of allocating a shadow
    // chapter vector on every repaint.
    let (chapter_words, later_words) = if matches!(
        visibility,
        ProgressVisibility::TimeChapter | ProgressVisibility::TimeBook
    ) {
        let current = book
            .chapters
            .get(chapter_index)
            .map_or(0, |chapter| chapter.word_count);
        let later = book
            .chapters
            .get(chapter_index + 1..)
            .unwrap_or_default()
            .iter()
            .map(|chapter| chapter.word_count)
            .sum::<usize>();
        (current, later)
    } else {
        (0, 0)
    };
    let wpm = state.pace.effective_wpm();

    match visibility {
        ProgressVisibility::Hidden => String::new(),
        ProgressVisibility::TimeChapter => {
            let progress = state.chapter_progress(content_width, body_height);
            format_time_left(
                chapter_words as f64 * (1.0 - progress),
                wpm,
                EstimateScope::Chapter,
            )
        }
        ProgressVisibility::TimeBook => {
            let progress = state.chapter_progress(content_width, body_height);
            format_time_left(
                chapter_words as f64 * (1.0 - progress) + later_words as f64,
                wpm,
                EstimateScope::Book,
            )
        }
        ProgressVisibility::Chapter => {
            let progress = state.chapter_progress(content_width, body_height);
            format!("Ch {:.0}%", progress * 100.0)
        }
        ProgressVisibility::Book => {
            let progress = state.book_progress(content_width, body_height);
            format!("Book {:.0}%", progress * 100.0)
        }
        ProgressVisibility::Both => {
            let chapter_progress = state.chapter_progress(content_width, body_height);
            let book_progress = state.book_progress(content_width, body_height);
            format!(
                "Ch {}/{} {:.0}% · Book {:.0}%",
                chapter_index + 1,
                chapter_count,
                chapter_progress * 100.0,
                book_progress * 100.0
            )
        }
    }
}

/// The closing rule: what just happened on the left, what the keys do on the right.
fn draw_status_rule(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &ReaderState,
    command_bar: &CommandBar,
) {
    let status = if state.status.is_empty() {
        Cow::Borrowed("Ready")
    } else {
        Cow::Borrowed(state.status.as_str())
    };
    // A running timer belongs with the status: both say what the session is doing.
    let status = match &command_bar.timer {
        Some(timer) => Cow::Owned(format!("{timer} · {status}")),
        None => status,
    };
    frame.render_widget(
        Paragraph::new(rule(
            RuleSide::Bottom,
            area.width,
            &status,
            &footer_hints(state, command_bar),
            &state.theme.palette,
        )),
        area,
    );
}

/// Where the reader is: chapter, rendered lines on screen, focus section, search
/// match — and, on the right, how much is left.
fn draw_metadata(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &mut ReaderState,
    layout: &reader_app::ViewportLayout,
    visible: VisibleWindow,
) {
    let left = position_text(state, visible);
    let progress = progress_text(state, layout.content_width, layout.body_height);
    let bar = progress_bar_value(state, layout.content_width, layout.body_height);

    let palette = &state.theme.palette;
    let mut right: Vec<Span<'static>> = Vec::new();
    if let Some(value) = bar {
        right.extend(progress_bar(value, PROGRESS_BAR_WIDTH, palette));
        right.push(Span::raw(" "));
    }
    if !progress.is_empty() {
        right.push(Span::styled(
            progress.clone(),
            to_tui_style(Style::fg(palette.accent_muted)),
        ));
    }

    let width = area.width as usize;
    let right_width: usize = right
        .iter()
        .map(|span| span.content.chars().count())
        .sum::<usize>();
    let left = truncate(&left, width.saturating_sub(right_width + 1));
    let gap = width
        .saturating_sub(left.chars().count())
        .saturating_sub(right_width);

    let mut spans = vec![Span::styled(left, to_tui_style(Style::fg(palette.dim)))];
    spans.push(Span::raw(" ".repeat(gap)));
    spans.extend(right);
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// `Ch 2/12 · ln 31–52/480 · §4/9 · match 2/7`, as far as it fits.
fn position_text(state: &mut ReaderState, visible: VisibleWindow) -> String {
    let Some(book) = state.current_book.as_ref() else {
        return String::new();
    };
    let chapters = book.chapters.len();
    let first = if visible.total == 0 {
        0
    } else {
        visible.offset + 1
    };
    let last = (visible.offset + visible.rows).min(visible.total);
    let mut text = format!(
        "Ch {}/{} · ln {first}–{last}/{}",
        state.chapter_index + 1,
        chapters,
        visible.total
    );
    if state.focus_mode {
        text.push_str(&format!(
            " · §{}/{}",
            state.focus_block_index + 1,
            state.chapter_block_count()
        ));
    }
    if let Some(search) = state.search.as_ref() {
        text.push_str(&format!(
            " · match {}/{}",
            search.cursor + 1,
            search.results.len()
        ));
    }
    text
}

/// The fraction a bar should show, or `None` when the setting asks for words.
fn progress_bar_value(
    state: &mut ReaderState,
    content_width: u16,
    body_height: u16,
) -> Option<f64> {
    state.current_book.as_ref()?;
    match state.settings.progress_visibility {
        ProgressVisibility::Chapter => Some(state.chapter_progress(content_width, body_height)),
        ProgressVisibility::Book | ProgressVisibility::Both => {
            Some(state.book_progress(content_width, body_height))
        }
        ProgressVisibility::Hidden
        | ProgressVisibility::TimeChapter
        | ProgressVisibility::TimeBook => None,
    }
}

fn progress_bar(value: f64, width: usize, palette: &reader_core::Palette) -> Vec<Span<'static>> {
    let filled = ((value.clamp(0.0, 1.0) * width as f64).round() as usize).min(width);
    vec![
        Span::styled("█".repeat(filled), to_tui_style(Style::fg(palette.accent))),
        Span::styled(
            "░".repeat(width - filled),
            to_tui_style(Style::fg(palette.border)),
        ),
    ]
}

/// The overlay's heading, including what is filtering it.
fn overlay_title(state: &ReaderState) -> String {
    let name = match state.overlay {
        Overlay::Chapters => "Chapters",
        Overlay::Books => "Library",
        Overlay::Bookmarks => "Bookmarks",
        Overlay::Notes => "Notes",
        Overlay::ColorSchemes => "Colorschemes",
        Overlay::Themes => "Themes",
        Overlay::Settings => "Reader settings",
        Overlay::Keys => "Keyboard Shortcuts",
        Overlay::Diagnostics if !state.integration_report.is_empty() => "Toggl",
        Overlay::Diagnostics => "Import diagnostics",
        Overlay::FilePicker => "Add Books",
        Overlay::Help => "Manual",
        Overlay::None => "",
    };

    // The library says how it is sorted; the modal's own search row shows the
    // query, so the title only carries what the list is otherwise scoped by.
    let sort = if state.overlay == Overlay::Books {
        let arrow = match state.library_sort_direction {
            reader_core::SortDirection::Ascending => '↑',
            reader_core::SortDirection::Descending => '↓',
        };
        let tag = match state.books_tag_filter.as_deref() {
            Some(tag) => format!(" · #{tag}"),
            None => String::new(),
        };
        format!(" · Sort: {} {arrow}{tag}", state.library_sort_key.label())
    } else {
        String::new()
    };
    // A modal shows its query in its own search row; a side list has no room
    // for one, so its title carries it instead.
    let search = if state.overlay.is_modal() {
        String::new()
    } else if state.overlay_search.active {
        format!(" · /{}", state.overlay_search.buffer)
    } else if !state.overlay_search.query().is_empty() {
        format!(" · {}", state.overlay_search.query())
    } else {
        String::new()
    };
    format!("{name}{sort}{search}")
}

/// The keys the open modal responds to.
fn overlay_hints(state: &ReaderState) -> String {
    if state.overlay_search.active {
        return hints(&[("Esc", "exit search"), ("Enter", "confirm")]);
    }
    match state.overlay {
        Overlay::Keys => hints(&[
            ("↑/↓", "nav"),
            ("Enter/Space", "expand"),
            ("/", "search"),
            ("Esc", "close"),
        ]),
        Overlay::Books => hints(&[
            ("Enter", "open"),
            ("s/r", "sort"),
            ("/", "search"),
            ("Esc", "close"),
        ]),
        Overlay::FilePicker => hints(&[
            ("Space", "select"),
            ("Enter", "import"),
            ("/", "search"),
            ("Esc", "close"),
        ]),
        _ => hints(&[("↑/↓", "nav"), ("Enter", "confirm"), ("Esc", "close")]),
    }
}

/// What an empty modal should say, which is rarely "nothing here".
fn overlay_empty_message(state: &ReaderState) -> String {
    if !state.overlay_search.query().is_empty() {
        return "Nothing matches.".to_owned();
    }
    match state.overlay {
        Overlay::Keys => "No shortcuts match.".to_owned(),
        Overlay::Books => {
            "The library is empty. Press Esc, then / and type add to import a book.".to_owned()
        }
        Overlay::FilePicker => format!(
            "No EPUB, CBZ, or PDF files under {}.",
            state.library_directory.display()
        ),
        Overlay::Diagnostics => "Nothing to report.".to_owned(),
        _ => "Nothing here yet.".to_owned(),
    }
}

fn draw_overlay(frame: &mut Frame<'_>, area: Rect, state: &ReaderState, entries: &[OverlayEntry]) {
    if state.overlay == Overlay::Settings {
        crate::settings_page::draw(frame, area, state, entries);
        return;
    }
    if state.overlay.is_modal() {
        crate::modals::draw_modal(
            frame,
            area,
            state,
            entries,
            &crate::modals::ModalSpec {
                title: overlay_title(state),
                placeholder: "/ to search",
                hints: overlay_hints(state),
                empty: overlay_empty_message(state),
            },
        );
        return;
    }

    let title = overlay_title(state);
    let palette = &state.theme.palette;

    let overlay_area = if state.overlay == Overlay::Help {
        centred(area, 80, 90)
    } else {
        side_overlay_area(area)
    };

    frame.render_widget(Clear, overlay_area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(to_tui_style(Style::fg(palette.accent_muted)))
        .title(Span::styled(
            format!(" {title} "),
            to_tui_style(Style::fg(palette.accent).bold()),
        ));
    let inner = block.inner(overlay_area);
    frame.render_widget(block, overlay_area);

    let height = inner.height as usize;
    if entries.is_empty() {
        let message = if state.overlay_search.query().is_empty() {
            "Nothing here yet."
        } else {
            "Nothing matches."
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                message,
                to_tui_style(Style::fg(palette.dim)),
            ))),
            inner,
        );
        return;
    }

    // Keep the cursor in view by scrolling the window around it.
    let start = crate::chrome::window_start(entries.len(), height, state.overlay_cursor);
    let rows: Vec<Line<'static>> = entries
        .iter()
        .enumerate()
        .skip(start)
        .take(height)
        .map(|(index, entry)| {
            let selected = index == state.overlay_cursor;
            let style = if selected {
                to_tui_style(Style::fg(palette.background).with_bg(palette.accent))
            } else {
                to_tui_style(Style::fg(palette.foreground))
            };
            let text: String = entry.display.chars().take(inner.width as usize).collect();
            Line::from(Span::styled(text, style))
        })
        .collect();
    frame.render_widget(Paragraph::new(rows).style(TuiStyle::default()), inner);
}

/// The column a side overlay occupies, on the right of the reading area.
#[must_use]
pub fn side_overlay_area(area: Rect) -> Rect {
    let width = (area.width / 3).clamp(20, 46).min(area.width);
    Rect {
        x: area.x + area.width.saturating_sub(width),
        y: area.y,
        width,
        height: area.height,
    }
}

/// A centred rectangle taking `width_percent` × `height_percent` of `area`.
fn centred(area: Rect, width_percent: u16, height_percent: u16) -> Rect {
    let width = (area.width * width_percent / 100).max(1);
    let height = (area.height * height_percent / 100).max(1);
    Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use reader_app::{Overlay, ReaderState};
    use reader_core::{
        AppSettings, CanonicalBlock, CanonicalBook, CanonicalChapter, ProgressVisibility,
        RenderMode,
    };

    use super::{CommandBar, draw, footer_height, progress_text};

    fn book() -> CanonicalBook {
        CanonicalBook {
            id: "book".into(),
            title: "Quiet Harbour".into(),
            author: "Author".into(),
            source_path: "/books/quiet.epub".into(),
            import_hash: "hash".into(),
            parser_version: Some(3),
            diagnostics: Vec::new(),
            chapters: (0..3)
                .map(|index| CanonicalChapter {
                    id: format!("ch{index}"),
                    index,
                    title: format!("Chapter {}", index + 1),
                    href: format!("ch{index}.xhtml"),
                    depth: usize::from(index > 0),
                    blocks: (0..8)
                        .map(|block| CanonicalBlock::Paragraph {
                            id: format!("b{index}-{block}"),
                            text: "The lantern swung once over the quiet harbour at dawn.".into(),
                        })
                        .collect(),
                    word_count: 80,
                })
                .collect(),
            cover_path: None,
        }
    }

    fn reader() -> ReaderState {
        let mut state = ReaderState::new(AppSettings {
            render_mode: RenderMode::Plain,
            ..AppSettings::default()
        });
        state.current_book = Some(book());
        state.status = "Opened Quiet Harbour".to_owned();
        state
    }

    /// Render one frame and say, for each body row that has text on it, whether
    /// it was dimmed. Blank rows carry no colour, so they are left out.
    ///
    /// Dimming is what focus mode does to everything outside the block it is on,
    /// so this is how a test sees which block the column is showing.
    fn dimmed_rows(state: &mut ReaderState, width: u16, height: u16) -> Vec<(u16, bool)> {
        let storage = reader_storage::Storage::open_in_memory().expect("database");
        let entries = reader_app::visible_entries(state, &storage, 0);
        let dim = crate::style::to_tui_color(state.theme.palette.dim);
        let mut terminal =
            Terminal::new(TestBackend::new(width, height)).expect("test backend should build");
        terminal
            .draw(|frame| draw(frame, state, &CommandBar::default(), &entries))
            .expect("drawing should succeed");
        let buffer = terminal.backend().buffer().clone();
        // Row 0 is the opening rule; the last two are the status and metadata.
        (1..buffer.area.height - 2)
            .filter_map(|row| {
                let written: Vec<_> = (0..buffer.area.width)
                    .map(|column| &buffer[(column, row)])
                    // The scrollbar is chrome, not text.
                    .filter(|cell| cell.symbol().trim() != "" && cell.symbol() != "│")
                    .collect();
                (!written.is_empty())
                    .then(|| (row, written.iter().all(|cell| cell.style().fg == Some(dim))))
            })
            .collect()
    }

    #[test]
    fn focus_mode_centres_one_block_and_dims_the_rest() {
        let mut state = reader();
        let plain = dimmed_rows(&mut state, 78, 14);
        assert!(
            plain.iter().all(|(_, dimmed)| !dimmed),
            "nothing is dimmed while reading normally: {plain:?}"
        );

        state.focus_mode = true;
        state.focus_block_index = 3;
        state.block_offset = state.focus_index_to_offset(60, 3);
        let focused = dimmed_rows(&mut state, 78, 14);

        let lit: Vec<u16> = focused
            .iter()
            .filter(|(_, dimmed)| !dimmed)
            .map(|(row, _)| *row)
            .collect();
        assert!(!lit.is_empty(), "the focused block stays lit: {focused:?}");
        assert!(
            focused.iter().any(|(_, dimmed)| *dimmed),
            "its neighbours are dimmed: {focused:?}"
        );
        // Lit rows are one run: the block is whole, not a scatter of lines.
        let first = focused
            .iter()
            .position(|(_, dimmed)| !dimmed)
            .expect("a lit row");
        let last = focused
            .iter()
            .rposition(|(_, dimmed)| !dimmed)
            .expect("a lit row");
        assert!(
            focused[first..=last].iter().all(|(_, dimmed)| !dimmed),
            "the lit rows are contiguous: {focused:?}"
        );
        assert!(
            first.abs_diff(focused.len() - 1 - last) <= 1,
            "the block sits in the middle of the column: lit {lit:?} of {focused:?}"
        );
    }

    /// Render one frame and return its rows as plain strings.
    ///
    /// Overlay rows are built from an empty in-memory library, which is enough
    /// for the overlays whose contents do not come from the database.
    fn render(
        state: &mut ReaderState,
        command_bar: &CommandBar,
        width: u16,
        height: u16,
    ) -> Vec<String> {
        let storage = reader_storage::Storage::open_in_memory().expect("database");
        let entries = reader_app::visible_entries(state, &storage, 0);
        let mut terminal =
            Terminal::new(TestBackend::new(width, height)).expect("test backend should build");
        terminal
            .draw(|frame| draw(frame, state, command_bar, &entries))
            .expect("drawing should succeed");
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|row| {
                (0..buffer.area.width)
                    .map(|column| buffer[(column, row)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect()
    }

    #[test]
    fn the_frame_shows_the_book_the_chapter_and_the_status() {
        let mut state = reader();
        let rows = render(&mut state, &CommandBar::default(), 78, 12);

        assert!(
            rows[0].contains("Quiet Harbour · Ch 1/3 · Chapter 1"),
            "header row was {:?}",
            rows[0]
        );
        assert!(
            rows[1..].iter().any(|row| row.contains("lantern")),
            "the body should show prose: {rows:?}"
        );
        assert!(
            rows.iter().any(|row| row.contains("Opened Quiet Harbour")),
            "the status belongs on the closing rule: {rows:?}"
        );
    }

    #[test]
    fn the_reading_frame_is_ruled_top_and_bottom_and_never_walled() {
        let mut state = reader();
        let rows = render(&mut state, &CommandBar::default(), 78, 12);

        assert!(rows[0].starts_with('╭'), "{:?}", rows[0]);
        assert!(rows[0].ends_with('╮'), "{:?}", rows[0]);
        let closing = rows
            .iter()
            .find(|row| row.starts_with('╰'))
            .expect("a closing rule");
        assert!(closing.ends_with('╯'), "{closing:?}");

        // Everything between the rules is text, a margin, or the scrollbar.
        let body = &rows[1..rows.len() - 2];
        for row in body {
            assert!(!row.starts_with('│'), "a body row was walled: {row:?}");
            assert!(!row.starts_with('╭') && !row.starts_with('╰'), "{row:?}");
        }
    }

    #[test]
    fn the_header_carries_the_mode_density_and_theme() {
        let mut state = reader();
        let plain = render(&mut state, &CommandBar::default(), 100, 12);
        assert!(plain[0].contains("plain"), "{:?}", plain[0]);
        assert!(
            plain[0].contains("Codex"),
            "the colorscheme: {:?}",
            plain[0]
        );
        assert!(plain[0].contains("Dark"), "the appearance: {:?}", plain[0]);
        assert!(
            !plain[0].contains("density"),
            "plain mode has no density: {:?}",
            plain[0]
        );

        state.settings.render_mode = RenderMode::Code;
        state.settings.code_language = reader_core::CodeLanguage::Rust;
        let code = render(&mut state, &CommandBar::default(), 100, 12);
        assert!(code[0].contains("rust · density:"), "{:?}", code[0]);
    }

    #[test]
    fn the_header_shows_focus_and_search_context() {
        let mut state = reader();
        state.focus_mode = true;
        state.focus_block_index = 2;
        state.search = Some(reader_app::SearchState {
            query: "lantern".into(),
            global: false,
            results: vec![reader_app::SearchHit {
                chapter_index: 0,
                block_index: 1,
                line_index: 0,
            }],
            cursor: 0,
        });

        let rows = render(&mut state, &CommandBar::default(), 110, 12);

        assert!(rows[0].contains("focus §3"), "{:?}", rows[0]);
        assert!(rows[0].contains("[1/1] \"lantern\""), "{:?}", rows[0]);
    }

    #[test]
    fn the_metadata_row_reports_the_lines_on_screen_and_follows_scrolling() {
        let mut state = reader();
        let top = render(&mut state, &CommandBar::default(), 78, 14);
        let first = top.last().expect("a metadata row").clone();
        assert!(first.contains("Ch 1/3 · ln 1–"), "{first:?}");

        state.block_offset = 5;
        let scrolled = render(&mut state, &CommandBar::default(), 78, 14);
        let second = scrolled.last().expect("a metadata row");
        assert!(second.contains("ln 6–"), "{second:?}");
        assert_ne!(&first, second, "scrolling changes the metadata");
    }

    #[test]
    fn the_metadata_row_reports_focus_sections_and_search_matches() {
        let mut state = reader();
        state.focus_mode = true;
        state.focus_block_index = 1;
        state.search = Some(reader_app::SearchState {
            query: "harbour".into(),
            global: true,
            results: vec![
                reader_app::SearchHit {
                    chapter_index: 0,
                    block_index: 0,
                    line_index: 0,
                },
                reader_app::SearchHit {
                    chapter_index: 1,
                    block_index: 0,
                    line_index: 0,
                },
            ],
            cursor: 1,
        });

        let rows = render(&mut state, &CommandBar::default(), 110, 14);
        let metadata = rows.last().expect("a metadata row");

        assert!(metadata.contains("§2/8"), "{metadata:?}");
        assert!(metadata.contains("match 2/2"), "{metadata:?}");
    }

    #[test]
    fn hiding_the_progress_keeps_the_position_metadata() {
        let mut state = reader();
        state.settings.progress_visibility = ProgressVisibility::Hidden;

        let rows = render(&mut state, &CommandBar::default(), 78, 14);
        let metadata = rows.last().expect("a metadata row");

        assert!(metadata.contains("ln 1–"), "{metadata:?}");
        assert!(!metadata.contains('█'), "no bar when hidden: {metadata:?}");
        assert!(!metadata.contains('%'), "{metadata:?}");
    }

    #[test]
    fn an_empty_reader_explains_what_to_do() {
        let mut state = ReaderState::new(AppSettings::default());
        let rows = render(&mut state, &CommandBar::default(), 70, 8);
        assert!(rows[0].contains("stealth-reader"));
        assert!(
            rows.iter().any(|row| row.contains("No book open")),
            "{rows:?}"
        );
    }

    #[test]
    fn scrolling_moves_the_visible_window() {
        let mut state = reader();
        let top = render(&mut state, &CommandBar::default(), 60, 12);
        state.block_offset = 6;
        let scrolled = render(&mut state, &CommandBar::default(), 60, 12);
        assert_ne!(top[1], scrolled[1], "the first body row should change");
    }

    #[test]
    fn the_offset_is_clamped_so_the_last_screen_stays_full() {
        let mut state = reader();
        state.block_offset = 10_000;
        let rows = render(&mut state, &CommandBar::default(), 60, 12);
        assert!(
            rows[1..rows.len() - 2].iter().any(|row| !row.is_empty()),
            "an over-scrolled chapter should still show text: {rows:?}"
        );
    }

    #[test]
    fn the_palette_draws_over_the_reading_area_without_reflowing_it() {
        let inactive = CommandBar::default();
        let active = CommandBar {
            active: true,
            buffer: "goto 2".to_owned(),
            cursor: 6,
            selected: 0,
            timer: None,
        };
        let mut state = reader();
        assert_eq!(
            footer_height(&state, &active),
            footer_height(&state, &inactive),
            "opening the palette must not move the text behind it"
        );
        assert_eq!(footer_height(&state, &inactive), 1);

        let empty = ReaderState::new(AppSettings::default());
        assert_eq!(
            footer_height(&empty, &inactive),
            0,
            "without a book there is nothing to report"
        );

        let rows = render(&mut state, &active, 78, 20);
        assert!(
            rows.iter().any(|row| row.contains("/goto 2█")),
            "the typed line belongs inside the palette: {rows:?}"
        );
    }

    #[test]
    fn the_palette_lists_the_matching_commands_and_says_what_they_do() {
        let mut state = reader();
        let command_bar = CommandBar {
            active: true,
            buffer: "boo".to_owned(),
            cursor: 3,
            selected: 0,
            timer: None,
        };
        let rows = render(&mut state, &command_bar, 100, 24);

        let listed: Vec<&String> = rows.iter().filter(|row| row.contains("/book")).collect();
        assert!(
            !listed.is_empty(),
            "typing an alias should list its command: {rows:?}"
        );
        assert!(
            rows.iter().any(|row| row.contains("Switch books")),
            "a row says what the command does: {rows:?}"
        );
        assert!(
            rows.iter().any(|row| row.contains("Library")),
            "and which category it belongs to: {rows:?}"
        );
        assert!(
            rows.iter()
                .any(|row| row.contains("Tab:complete") && row.contains("Esc:close")),
            "the footer switches to the palette's own keys: {rows:?}"
        );
    }

    #[test]
    fn only_the_highlighted_row_carries_its_aliases_and_flags() {
        let mut state = reader();
        let command_bar = CommandBar {
            active: true,
            buffer: "highlight".to_owned(),
            cursor: 9,
            selected: 0,
            timer: None,
        };
        let rows = render(&mut state, &command_bar, 120, 24);
        assert!(
            rows.iter().any(|row| row.contains("(try /highlight on)")),
            "the selected row explains itself: {rows:?}"
        );
    }

    #[test]
    fn a_query_that_matches_nothing_says_so_instead_of_showing_an_empty_box() {
        let mut state = reader();
        let command_bar = CommandBar {
            active: true,
            buffer: "zzz".to_owned(),
            cursor: 3,
            selected: 0,
            timer: None,
        };
        let rows = render(&mut state, &command_bar, 60, 14);
        assert!(
            rows.iter().any(|row| row.contains("No command matches")),
            "{rows:?}"
        );
        assert!(
            rows.iter().any(|row| row.contains("/zzz█")),
            "the typed line stays visible: {rows:?}"
        );
    }

    #[test]
    fn the_highlight_moves_through_the_list() {
        let mut state = reader();
        let mut command_bar = CommandBar {
            active: true,
            selected: 0,
            ..CommandBar::default()
        };
        command_bar.active = true;
        let first = render(&mut state, &command_bar, 120, 24);

        command_bar.selected = 3;
        let moved = render(&mut state, &command_bar, 120, 24);
        assert_ne!(first, moved, "a different row is highlighted");

        command_bar.selected = 200;
        let clamped = render(&mut state, &command_bar, 120, 24);
        assert!(
            clamped.iter().any(|row| row.contains('█')),
            "past the end the list still draws: {clamped:?}"
        );
    }

    #[test]
    fn a_scrollbar_appears_only_when_the_chapter_is_taller_than_the_screen() {
        let mut state = reader();
        let short = render(&mut state, &CommandBar::default(), 60, 40);
        let tall = render(&mut state, &CommandBar::default(), 60, 10);
        let has_thumb = |rows: &[String]| rows.iter().any(|row| row.contains('█'));
        assert!(
            has_thumb(&tall),
            "a long chapter needs a scrollbar: {tall:?}"
        );
        assert!(
            !has_thumb(&short),
            "a chapter that fits needs no scrollbar: {short:?}"
        );
    }

    #[test]
    fn progress_text_follows_the_configured_visibility() {
        let mut state = reader();

        state.settings.progress_visibility = ProgressVisibility::Hidden;
        assert_eq!(progress_text(&mut state, 60, 10), "");

        state.settings.progress_visibility = ProgressVisibility::Chapter;
        assert!(progress_text(&mut state, 60, 10).starts_with("Ch "));

        state.settings.progress_visibility = ProgressVisibility::Book;
        assert!(progress_text(&mut state, 60, 10).starts_with("Book "));

        state.settings.progress_visibility = ProgressVisibility::Both;
        let both = progress_text(&mut state, 60, 10);
        assert!(both.contains("Ch 1/3") && both.contains("Book"), "{both}");

        state.settings.progress_visibility = ProgressVisibility::TimeChapter;
        assert!(
            progress_text(&mut state, 60, 10).ends_with("left in chapter"),
            "{}",
            progress_text(&mut state, 60, 10)
        );

        state.settings.progress_visibility = ProgressVisibility::TimeBook;
        assert!(progress_text(&mut state, 60, 10).ends_with("left in book"));
    }

    #[test]
    fn progress_is_empty_without_a_book() {
        let mut state = ReaderState::new(AppSettings::default());
        assert_eq!(progress_text(&mut state, 60, 10), "");
    }

    #[test]
    fn side_overlays_list_their_entries_and_mark_the_selection() {
        let mut state = reader();
        state.overlay = Overlay::Chapters;
        state.overlay_cursor = 1;

        let rows = render(&mut state, &CommandBar::default(), 70, 12);

        assert!(
            rows.iter().any(|row| row.contains("Chapters")),
            "the overlay names itself: {rows:?}"
        );
        assert!(
            rows.iter().any(|row| row.contains("Chapter 2")),
            "the overlay should list chapters: {rows:?}"
        );
    }

    #[test]
    fn the_manual_opens_as_a_centred_page() {
        let mut state = reader();
        state.overlay = Overlay::Help;
        state.help_command = Some("goto".to_owned());

        let rows = render(&mut state, &CommandBar::default(), 80, 20);

        assert!(
            rows.iter().any(|row| row.contains("/GOTO(1)")),
            "the manual page should be visible: {rows:?}"
        );
    }

    #[test]
    fn the_shortcut_modal_names_itself_groups_its_rows_and_offers_a_search() {
        let mut state = reader();
        state.overlay = Overlay::Keys;
        reader_app::shortcuts_panel::open(&mut state);

        let rows = render(&mut state, &CommandBar::default(), 80, 22);
        let screen = rows.join("\n");

        assert!(screen.contains("Keyboard Shortcuts"), "{screen}");
        assert!(screen.contains("[×]"), "the close control: {screen}");
        assert!(screen.contains("/ to search"), "{screen}");
        assert!(screen.contains("◆ Essentials"), "{screen}");
        assert!(screen.contains("› Navigation ("), "folded: {screen}");
        assert!(
            screen.contains("Enter/Space:expand"),
            "the footer hints: {screen}"
        );

        // Key and description are separate columns, so the keys line up.
        let quit = rows
            .iter()
            .find(|row| row.contains("Quit the reader"))
            .expect("the quit binding");
        assert!(quit.trim_end().ends_with('│'), "{quit:?}");
        assert!(quit.contains("Quit the reader"), "{quit:?}");
    }

    #[test]
    fn the_shortcut_modal_darkens_the_reader_behind_it() {
        let mut state = reader();
        state.overlay = Overlay::Keys;

        let storage = reader_storage::Storage::open_in_memory().expect("database");
        let entries = reader_app::visible_entries(&state, &storage, 0);
        let mut terminal =
            Terminal::new(TestBackend::new(90, 24)).expect("test backend should build");
        terminal
            .draw(|frame| draw(frame, &mut state, &CommandBar::default(), &entries))
            .expect("drawing should succeed");
        let buffer = terminal.backend().buffer().clone();

        let subtle = crate::style::to_tui_color(state.theme.palette.subtle);
        let modal = crate::modals::modal_geometry(ratatui::layout::Rect {
            x: 0,
            y: 1,
            width: 90,
            height: 22,
        });
        // A cell of the reading column, well outside the modal.
        let cell = &buffer[(1, modal.area.y)];
        assert_eq!(
            cell.style().fg,
            Some(subtle),
            "the page behind should read as background"
        );
    }

    #[test]
    fn a_shortcut_search_shows_the_query_and_only_what_matches() {
        let mut state = reader();
        state.overlay = Overlay::Keys;
        reader_app::shortcuts_panel::open(&mut state);
        state.overlay_search.active = true;
        state.overlay_search.buffer = "bookmark".into();

        let screen = render(&mut state, &CommandBar::default(), 84, 22).join("\n");

        assert!(
            screen.contains("/ bookmark"),
            "the query is visible: {screen}"
        );
        assert!(screen.contains("Open bookmarks"), "{screen}");
        assert!(
            !screen.contains("Jump to top"),
            "non-matching bindings stay out: {screen}"
        );
        assert!(
            screen.contains("Esc:exit search"),
            "the footer distinguishes leaving search from closing: {screen}"
        );
    }

    #[test]
    fn the_shortcut_modal_scrolls_to_keep_the_cursor_in_view() {
        let mut state = reader();
        state.overlay = Overlay::Keys;
        let rows = reader_app::shortcuts_panel::rows(&state).len();
        state.overlay_cursor = rows - 1;

        let screen = render(&mut state, &CommandBar::default(), 84, 22).join("\n");

        assert!(
            screen.contains("Quit the reader") || screen.contains("Autocomplete"),
            "the window follows the cursor to the end: {screen}"
        );
    }

    #[test]
    fn every_settings_tab_shows_its_own_controls_and_marks_itself_active() {
        let mut state = reader();
        state.overlay = Overlay::Settings;
        reader_app::settings_panel::open(&mut state);

        let expected = [
            (reader_core::SettingsTab::Themes, "Colorscheme", "Margin"),
            (
                reader_core::SettingsTab::Reading,
                "Code density",
                "Colorscheme",
            ),
            (reader_core::SettingsTab::Layout, "Margin", "Code density"),
            (reader_core::SettingsTab::More, "Mouse capture", "Margin"),
        ];
        for (tab, present, absent) in expected {
            state.settings_tab = tab;
            let screen = render(&mut state, &CommandBar::default(), 100, 30).join("\n");
            assert!(screen.contains("Reader settings"), "{screen}");
            assert!(
                screen.contains(present),
                "{tab:?} should show {present}:\n{screen}"
            );
            assert!(
                !screen.contains(&format!("› {absent}"))
                    && !screen.contains(&format!("  {absent} ")),
                "{tab:?} should not show {absent}:\n{screen}"
            );
            assert!(screen.contains(tab.label()), "the tab strip: {screen}");
        }
    }

    #[test]
    fn the_settings_page_shows_a_search_row_a_description_and_a_preview() {
        let mut state = reader();
        state.overlay = Overlay::Settings;
        reader_app::settings_panel::open(&mut state);

        let screen = render(&mut state, &CommandBar::default(), 100, 30).join("\n");

        assert!(screen.contains("Search settings..."), "{screen}");
        assert!(
            screen.contains("Accent color palette"),
            "the selected row explains itself:\n{screen}"
        );
        assert!(screen.contains("Preview"), "{screen}");
        assert!(
            screen.contains("A quiet chapter begins here."),
            "the preview renders the draft:\n{screen}"
        );
        assert!(
            screen.contains("←/→ tab")
                && screen.contains("Enter save")
                && screen.contains("Esc cancel"),
            "the footer explains every key:\n{screen}"
        );
    }

    #[test]
    fn the_settings_preview_follows_the_draft_without_persisting_it() {
        let mut state = reader();
        state.overlay = Overlay::Settings;
        reader_app::settings_panel::open(&mut state);

        let plain = render(&mut state, &CommandBar::default(), 100, 30).join("\n");
        assert!(plain.contains("A quiet chapter begins here."), "{plain}");

        state.settings.render_mode = RenderMode::Code;
        state.settings.code_language = reader_core::CodeLanguage::TypeScript;
        let disguised = render(&mut state, &CommandBar::default(), 100, 30).join("\n");

        assert!(
            disguised.contains("const") || disguised.contains('='),
            "the preview follows the draft mode:\n{disguised}"
        );
        assert!(
            state.settings_backup.is_some(),
            "the pre-open settings are still held for a cancel"
        );
    }

    #[test]
    fn a_settings_search_narrows_the_rows_and_says_when_nothing_matches() {
        let mut state = reader();
        state.overlay = Overlay::Settings;
        reader_app::settings_panel::open(&mut state);
        state.overlay_search.active = true;
        state.overlay_search.buffer = "appearance".into();

        let found = render(&mut state, &CommandBar::default(), 100, 30).join("\n");
        assert!(found.contains("Appearance"), "{found}");
        assert!(!found.contains("› Colorscheme"), "{found}");

        state.overlay_search.buffer = "zzz".into();
        let empty = render(&mut state, &CommandBar::default(), 100, 30).join("\n");
        assert!(empty.contains("No settings match your search."), "{empty}");
    }

    #[test]
    fn the_settings_page_survives_a_narrow_terminal() {
        let mut state = reader();
        state.overlay = Overlay::Settings;
        reader_app::settings_panel::open(&mut state);

        for (width, height) in [(30, 12), (44, 10), (24, 20)] {
            let rows = render(&mut state, &CommandBar::default(), width, height);
            assert_eq!(rows.len(), height as usize, "{width}x{height}");
            assert!(
                rows.iter().all(|row| row.chars().count() <= width as usize),
                "nothing overflows at {width}x{height}: {rows:?}"
            );
        }
    }

    #[test]
    fn a_narrow_terminal_keeps_the_header_and_the_footer_apart() {
        let mut state = reader();
        for width in [20, 30, 44] {
            let rows = render(&mut state, &CommandBar::default(), width, 10);
            assert_eq!(rows.len(), 10);
            assert!(
                rows[0].starts_with('╭'),
                "width {width} header was {:?}",
                rows[0]
            );
            assert!(
                rows[rows.len() - 2].starts_with('╰'),
                "width {width} closing rule was {:?}",
                rows[rows.len() - 2]
            );
            let metadata = rows.last().expect("a metadata row");
            assert!(
                !metadata.starts_with('╰'),
                "the metadata row must not overwrite the rule: {metadata:?}"
            );
        }
    }

    #[test]
    fn a_tiny_terminal_still_renders_without_panicking() {
        let mut state = reader();
        for (width, height) in [(1, 1), (4, 3), (20, 2), (200, 60)] {
            let rows = render(&mut state, &CommandBar::default(), width, height);
            assert_eq!(rows.len(), height as usize);
        }
    }

    #[test]
    fn overlays_render_at_every_size_and_kind() {
        let mut state = reader();
        for overlay in [
            Overlay::Chapters,
            Overlay::Books,
            Overlay::Bookmarks,
            Overlay::Notes,
            Overlay::ColorSchemes,
            Overlay::Themes,
            Overlay::Settings,
            Overlay::Keys,
            Overlay::Diagnostics,
            Overlay::FilePicker,
            Overlay::Help,
        ] {
            state.overlay = overlay;
            state.overlay_cursor = 0;
            let rows = render(&mut state, &CommandBar::default(), 40, 10);
            assert_eq!(rows.len(), 10, "{overlay:?} broke the frame");
        }
    }
}

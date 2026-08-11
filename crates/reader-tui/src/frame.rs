//! Composing one frame.
//!
//! Drawing reads state and never changes it, so a frame can be rendered into a
//! test buffer and asserted line by line. The reading column, the footer, and any
//! overlay are laid out from the same geometry the executor used, which is what
//! keeps scroll bounds and what is on screen in agreement.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style as TuiStyle;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use reader_app::{Overlay, ReaderState};
use reader_core::pace::{
    EstimateScope, format_time_left, remaining_words_in_book, remaining_words_in_chapter,
};
use reader_core::render::render_blocks;
use reader_core::style::Style;
use reader_core::{ChapterWords, ProgressVisibility};

use crate::style::{to_tui_line, to_tui_style};

/// What the command bar is doing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandBar {
    pub active: bool,
    pub buffer: String,
    /// Cursor position in characters.
    pub cursor: usize,
}

/// Rows the footer needs: the status line, plus the command bar when active.
#[must_use]
pub fn footer_height(command_bar: &CommandBar) -> u16 {
    if command_bar.active { 2 } else { 1 }
}

/// Draw the whole frame.
pub fn draw(frame: &mut Frame<'_>, state: &mut ReaderState, command_bar: &CommandBar) {
    let area = frame.area();
    state.viewport = reader_app::Viewport::new(area.width, area.height);
    let layout = state.layout(footer_height(command_bar));

    let footer_rows = footer_height(command_bar);
    let body_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: area.height.saturating_sub(footer_rows),
    };
    let footer_area = Rect {
        x: area.x,
        y: area.y + body_area.height,
        width: area.width,
        height: footer_rows,
    };

    draw_body(frame, body_area, state, &layout);
    draw_footer(frame, footer_area, state, command_bar);
    if state.overlay != Overlay::None {
        draw_overlay(frame, body_area, state);
    }
}

fn draw_body(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &mut ReaderState,
    layout: &reader_app::ViewportLayout,
) {
    let border_style = to_tui_style(Style::fg(state.theme.palette.border));
    let title = match state.current_book.as_ref() {
        Some(book) => match state.chapter() {
            Some(chapter) => format!(" {} · {} ", book.title, chapter.title),
            None => format!(" {} ", book.title),
        },
        None => " stealth-reader ".to_owned(),
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Span::styled(
            title,
            to_tui_style(Style::fg(state.theme.palette.accent).bold()),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(book) = state.current_book.as_ref() else {
        let hint = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "  No book open. Press / then type add to import one, or resume to continue.",
                to_tui_style(Style::fg(state.theme.palette.dim)),
            )),
        ]);
        frame.render_widget(hint, inner);
        return;
    };

    let Some(chapter) = book.chapters.get(state.chapter_index) else {
        return;
    };
    let options = state.render_options(layout.content_width);
    let lines = render_blocks(&chapter.blocks, &options);

    let body_height = inner.height as usize;
    let max_offset = lines.len().saturating_sub(body_height);
    let offset = state.block_offset.min(max_offset);
    let visible: Vec<Line<'static>> = lines
        .iter()
        .skip(offset)
        .take(body_height)
        .map(to_tui_line)
        .collect();

    let padding = layout.content_padding;
    let text_area = Rect {
        x: inner.x + padding,
        y: inner.y,
        width: inner.width.saturating_sub(padding + layout.scrollbar_width),
        height: inner.height,
    };
    frame.render_widget(Paragraph::new(visible), text_area);

    if layout.scrollbar_width > 0 && inner.width > 0 {
        draw_scrollbar(frame, inner, state, lines.len(), offset);
    }
}

/// A one-column scrollbar whose thumb length reflects how much is visible.
fn draw_scrollbar(
    frame: &mut Frame<'_>,
    inner: Rect,
    state: &ReaderState,
    total_lines: usize,
    offset: usize,
) {
    let height = inner.height as usize;
    if height == 0 || total_lines <= height {
        return;
    }
    let track_style = to_tui_style(Style::fg(state.theme.palette.border));
    let thumb_style = to_tui_style(Style::fg(state.theme.palette.accent_muted));
    let thumb_height = ((height * height) / total_lines).max(1);
    let max_offset = total_lines - height;
    let thumb_start = (offset * (height - thumb_height))
        .checked_div(max_offset)
        .unwrap_or(0);

    let column = inner.x + inner.width - 1;
    let rows: Vec<Line<'static>> = (0..height)
        .map(|row| {
            let inside = row >= thumb_start && row < thumb_start + thumb_height;
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
            y: inner.y,
            width: 1,
            height: inner.height,
        },
    );
}

/// The reading progress text for the footer.
#[must_use]
pub fn progress_text(state: &mut ReaderState, content_width: u16, body_height: u16) -> String {
    if state.settings.progress_visibility == ProgressVisibility::Hidden {
        return String::new();
    }
    let Some(book) = state.current_book.as_ref() else {
        return String::new();
    };
    let chapters: Vec<ChapterWords> = book
        .chapters
        .iter()
        .map(|chapter| ChapterWords::new(chapter.word_count))
        .collect();
    let chapter_index = state.chapter_index;
    let chapter_count = chapters.len();
    let chapter_progress = state.chapter_progress(content_width, body_height);
    let book_progress = state.book_progress(content_width, body_height);
    let wpm = state.pace.effective_wpm();

    match state.settings.progress_visibility {
        ProgressVisibility::Hidden => String::new(),
        ProgressVisibility::TimeChapter => format_time_left(
            remaining_words_in_chapter(&chapters, chapter_index, chapter_progress),
            wpm,
            EstimateScope::Chapter,
        ),
        ProgressVisibility::TimeBook => format_time_left(
            remaining_words_in_book(&chapters, chapter_index, chapter_progress),
            wpm,
            EstimateScope::Book,
        ),
        ProgressVisibility::Chapter => format!("Ch {:.0}%", chapter_progress * 100.0),
        ProgressVisibility::Book => format!("Book {:.0}%", book_progress * 100.0),
        ProgressVisibility::Both => format!(
            "Ch {}/{} {:.0}% · Book {:.0}%",
            chapter_index + 1,
            chapter_count,
            chapter_progress * 100.0,
            book_progress * 100.0
        ),
    }
}

fn draw_footer(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &mut ReaderState,
    command_bar: &CommandBar,
) {
    let layout = state.layout(footer_height(command_bar));
    let progress = progress_text(state, layout.content_width, layout.body_height);
    let status = state.status.clone();

    let mut rows: Vec<Line<'static>> = Vec::new();
    if command_bar.active {
        rows.push(Line::from(vec![
            Span::styled(
                "/",
                to_tui_style(Style::fg(state.theme.palette.accent).bold()),
            ),
            Span::styled(
                command_bar.buffer.clone(),
                to_tui_style(Style::fg(state.theme.palette.foreground)),
            ),
        ]));
    }

    // Progress is right-aligned; the status keeps whatever is left.
    let width = area.width as usize;
    let progress_width = progress.chars().count();
    let status_width = width.saturating_sub(progress_width + 1);
    let trimmed: String = status.chars().take(status_width).collect();
    let gap = width
        .saturating_sub(trimmed.chars().count())
        .saturating_sub(progress_width);
    rows.push(Line::from(vec![
        Span::styled(trimmed, to_tui_style(Style::fg(state.theme.palette.dim))),
        Span::raw(" ".repeat(gap)),
        Span::styled(
            progress,
            to_tui_style(Style::fg(state.theme.palette.accent_muted)),
        ),
    ]));

    frame.render_widget(Paragraph::new(rows), area);
}

/// Entries an overlay lists, and which one is selected.
fn overlay_entries(state: &ReaderState) -> (String, Vec<String>) {
    match state.overlay {
        Overlay::Chapters => (
            "Chapters".to_owned(),
            state
                .current_book
                .as_ref()
                .map(|book| {
                    book.chapters
                        .iter()
                        .map(|chapter| format!("{}{}", "  ".repeat(chapter.depth), chapter.title))
                        .collect()
                })
                .unwrap_or_default(),
        ),
        Overlay::ColorSchemes => (
            "Colorschemes".to_owned(),
            reader_core::ColorSchemeId::ALL
                .iter()
                .map(|scheme| scheme.label().to_owned())
                .collect(),
        ),
        Overlay::Themes => (
            "Themes".to_owned(),
            reader_core::theme::AppearanceThemeId::ALL
                .iter()
                .map(|theme| theme.label().to_owned())
                .collect(),
        ),
        Overlay::Keys => (
            "Keyboard shortcuts".to_owned(),
            reader_core::KEYBOARD_SHORTCUTS
                .iter()
                .map(|shortcut| format!("{:<22} {}", shortcut.key, shortcut.description))
                .collect(),
        ),
        Overlay::Help => (
            "Manual".to_owned(),
            reader_core::command::command_help(state.help_command.as_deref(), None),
        ),
        Overlay::Diagnostics => (
            "Import diagnostics".to_owned(),
            state
                .current_book
                .as_ref()
                .map(|book| {
                    book.diagnostics
                        .iter()
                        .map(|diagnostic| diagnostic.message.clone())
                        .collect()
                })
                .unwrap_or_default(),
        ),
        Overlay::FilePicker => (
            "Add a book".to_owned(),
            state
                .discoveries
                .iter()
                .map(|item| item.file_name.clone())
                .collect(),
        ),
        // Overlays whose contents come from the database are filled in by the
        // caller before drawing; an empty list is the honest default.
        Overlay::Books => ("Library".to_owned(), Vec::new()),
        Overlay::Bookmarks => ("Bookmarks".to_owned(), Vec::new()),
        Overlay::Notes => ("Notes".to_owned(), Vec::new()),
        Overlay::Settings => (
            "Settings".to_owned(),
            reader_core::SettingsTab::ALL
                .iter()
                .map(|tab| tab.label().to_owned())
                .collect(),
        ),
        Overlay::None => (String::new(), Vec::new()),
    }
}

fn draw_overlay(frame: &mut Frame<'_>, area: Rect, state: &ReaderState) {
    let (title, entries) = overlay_entries(state);
    let palette = &state.theme.palette;

    let overlay_area = if state.overlay.is_modal() || state.overlay == Overlay::Help {
        centred(area, 80, 90)
    } else {
        let width = (area.width / 3).clamp(20, 46);
        Rect {
            x: area.x + area.width.saturating_sub(width),
            y: area.y,
            width,
            height: area.height,
        }
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
    // Keep the cursor in view by scrolling the window around it.
    let start = state
        .overlay_cursor
        .saturating_sub(height / 2)
        .min(entries.len().saturating_sub(height));
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
            let text: String = entry.chars().take(inner.width as usize).collect();
            Line::from(Span::styled(text, style))
        })
        .collect();
    frame.render_widget(Paragraph::new(rows).style(TuiStyle::default()), inner);
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

    /// Render one frame and return its rows as plain strings.
    fn render(
        state: &mut ReaderState,
        command_bar: &CommandBar,
        width: u16,
        height: u16,
    ) -> Vec<String> {
        let mut terminal =
            Terminal::new(TestBackend::new(width, height)).expect("test backend should build");
        terminal
            .draw(|frame| draw(frame, state, command_bar))
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
        let rows = render(&mut state, &CommandBar::default(), 60, 12);

        assert!(
            rows[0].contains("Quiet Harbour · Chapter 1"),
            "title row was {:?}",
            rows[0]
        );
        assert!(
            rows[1..].iter().any(|row| row.contains("lantern")),
            "the body should show prose: {rows:?}"
        );
        assert!(
            rows.last()
                .expect("a footer row")
                .contains("Opened Quiet Harbour"),
            "footer was {:?}",
            rows.last()
        );
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
    fn the_command_bar_takes_a_row_only_while_it_is_active() {
        let inactive = CommandBar::default();
        let active = CommandBar {
            active: true,
            buffer: "goto 2".to_owned(),
            cursor: 6,
        };
        assert_eq!(footer_height(&inactive), 1);
        assert_eq!(footer_height(&active), 2);

        let mut state = reader();
        let rows = render(&mut state, &active, 60, 12);
        assert!(
            rows[rows.len() - 2].starts_with("/goto 2"),
            "command row was {:?}",
            rows[rows.len() - 2]
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

        assert!(rows[0].contains("Chapters"), "{:?}", rows[0]);
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
    fn the_shortcut_overlay_lists_key_bindings_and_scrolls_to_the_cursor() {
        let mut state = reader();
        state.overlay = Overlay::Keys;

        let from_top = render(&mut state, &CommandBar::default(), 80, 20);
        assert!(
            from_top.iter().any(|row| row.contains("Scroll up")),
            "the first bindings should be visible: {from_top:?}"
        );

        // The last binding is off-screen until the cursor reaches it.
        state.overlay_cursor = reader_core::KEYBOARD_SHORTCUTS.len() - 1;
        let from_bottom = render(&mut state, &CommandBar::default(), 80, 20);
        assert!(
            from_bottom
                .iter()
                .any(|row| row.contains("Quit the reader")),
            "the window should follow the cursor: {from_bottom:?}"
        );
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

//! Applying actions to the reader.
//!
//! Handling is separated from mapping so it can be tested without a terminal:
//! every function here takes state, storage, and geometry, and returns nothing
//! the caller has to interpret.

use reader_app::{
    CommandContext, Overlay, ReaderState, apply_search_hit, execute_command, visible_entries,
};
use reader_core::{CodeLanguage, RenderMode};
use reader_storage::Storage;

use crate::frame::CommandBar;
use crate::input::{Action, InputMode};

/// Which input map applies right now.
#[must_use]
pub fn current_mode(state: &ReaderState, command_bar: &CommandBar) -> InputMode {
    if command_bar.active {
        InputMode::Command
    } else if state.overlay == Overlay::None {
        InputMode::Reading
    } else if state.overlay_search.active {
        InputMode::OverlaySearch(state.overlay)
    } else {
        InputMode::Overlay(state.overlay)
    }
}

/// Apply one action.
pub fn apply(
    action: Action,
    state: &mut ReaderState,
    storage: &mut Storage,
    command_bar: &mut CommandBar,
    context: CommandContext,
) -> Result<(), reader_app::ExecutionError> {
    let page = (context.body_height as usize).saturating_sub(1).max(1);
    let clamp_offset_after = matches!(
        &action,
        Action::HistoryBack
            | Action::HistoryForward
            | Action::Dismiss
            | Action::SubmitCommand
            | Action::OverlayConfirm
            | Action::ChangeSetting
            | Action::CycleRenderMode
            | Action::CycleCodeDensity
            | Action::RunCommand(_)
    );
    let clamp_overlay_after = matches!(
        &action,
        Action::OverlayUp
            | Action::OverlayDown
            | Action::OverlayPageUp
            | Action::OverlayPageDown
            | Action::OverlayHome
            | Action::OverlayEnd
            | Action::OverlayConfirm
            | Action::OverlayDelete
            | Action::CycleLibrarySort
            | Action::ReverseLibrarySort
            | Action::BeginOverlaySearch
            | Action::EndOverlaySearch
            | Action::OverlaySearchChar(_)
            | Action::OverlaySearchBackspace
            | Action::ToggleAllGroups
            | Action::ChangeSetting
            | Action::NextTab
            | Action::PreviousTab
            | Action::SubmitCommand
            | Action::OpenColorSchemes
            | Action::OpenThemes
            | Action::OpenSettings
            | Action::OpenChapters
            | Action::OpenBookmarks
            | Action::OpenShortcuts
            | Action::RunCommand(_)
    );

    match action {
        Action::Ignore => {}
        Action::Quit => state.should_quit = true,

        Action::ScrollDown(step) => {
            let max_offset = state.chapter_max_offset(context.content_width, context.body_height);
            state.block_offset = (state.block_offset + step).min(max_offset);
        }
        Action::ScrollUp(step) => {
            state.block_offset = state.block_offset.saturating_sub(step);
        }
        Action::PageDown => {
            state.push_nav_history();
            let max_offset = state.chapter_max_offset(context.content_width, context.body_height);
            state.block_offset = (state.block_offset + page).min(max_offset);
        }
        Action::PageUp => {
            state.push_nav_history();
            state.block_offset = state.block_offset.saturating_sub(page);
        }
        Action::ChapterStart | Action::JumpToTop => {
            state.push_nav_history();
            state.block_offset = 0;
        }
        Action::ChapterEnd | Action::JumpToBottom => {
            state.push_nav_history();
            state.block_offset =
                state.chapter_max_offset(context.content_width, context.body_height);
        }
        Action::PreviousChapter => execute_command(state, storage, "/prev", context)?,
        Action::NextChapter => execute_command(state, storage, "/next", context)?,

        Action::HistoryBack => {
            if state.history_back() {
                state.status = "Went back".to_owned();
            } else {
                state.status = "No earlier position in history.".to_owned();
            }
        }
        Action::HistoryForward => {
            if state.history_forward() {
                state.status = "Went forward".to_owned();
            } else {
                state.status = "No later position in history.".to_owned();
            }
        }

        Action::NextSearchMatch => advance_search(state, true, context),
        Action::PreviousSearchMatch => advance_search(state, false, context),

        Action::FocusCommandBar => {
            command_bar.active = true;
            command_bar.buffer.clear();
            command_bar.cursor = 0;
        }
        Action::Dismiss => {
            if command_bar.active {
                command_bar.active = false;
                command_bar.buffer.clear();
                command_bar.cursor = 0;
            } else if state.overlay != Overlay::None {
                // Leaving the settings panel discards its preview.
                if state.overlay == Overlay::Settings {
                    reader_app::settings_panel::cancel(state);
                    state.status = "Settings unchanged.".to_owned();
                }
                close_overlay(state);
            } else if state.search.is_some() {
                state.search = None;
                state.status = "Search cleared.".to_owned();
            }
        }
        Action::SubmitCommand => {
            let line = std::mem::take(&mut command_bar.buffer);
            command_bar.active = false;
            command_bar.cursor = 0;
            if !line.trim().is_empty() {
                let command = if line.starts_with('/') {
                    line
                } else {
                    format!("/{line}")
                };
                execute_command(state, storage, &command, context)?;
            }
        }
        Action::CompleteCommand => {
            let suggestions = reader_core::command::list_command_suggestions(
                &command_bar.buffer,
                command_bar.cursor,
                None,
            );
            if let Some(suggestion) = suggestions.first() {
                command_bar.buffer =
                    reader_core::command::apply_completion(&command_bar.buffer, suggestion);
                command_bar.cursor = command_bar.buffer.chars().count();
            }
        }
        Action::InsertChar(character) => {
            let byte = char_to_byte(&command_bar.buffer, command_bar.cursor);
            command_bar.buffer.insert(byte, character);
            command_bar.cursor += 1;
        }
        Action::DeleteBackward => {
            if command_bar.cursor > 0 {
                let byte = char_to_byte(&command_bar.buffer, command_bar.cursor - 1);
                command_bar.buffer.remove(byte);
                command_bar.cursor -= 1;
            }
        }
        Action::DeleteForward => {
            if command_bar.cursor < command_bar.buffer.chars().count() {
                let byte = char_to_byte(&command_bar.buffer, command_bar.cursor);
                command_bar.buffer.remove(byte);
            }
        }
        Action::MoveCursorLeft => command_bar.cursor = command_bar.cursor.saturating_sub(1),
        Action::MoveCursorRight => {
            command_bar.cursor = (command_bar.cursor + 1).min(command_bar.buffer.chars().count());
        }
        Action::MoveCursorHome => command_bar.cursor = 0,
        Action::MoveCursorEnd => command_bar.cursor = command_bar.buffer.chars().count(),

        Action::OverlayUp => state.overlay_cursor = state.overlay_cursor.saturating_sub(1),
        Action::OverlayDown => state.overlay_cursor += 1,
        Action::OverlayPageUp => state.overlay_cursor = state.overlay_cursor.saturating_sub(page),
        Action::OverlayPageDown => state.overlay_cursor += page,
        Action::OverlayHome => state.overlay_cursor = 0,
        Action::OverlayEnd => state.overlay_cursor = usize::MAX,
        Action::OverlayConfirm => confirm_overlay(state, storage, context)?,
        Action::OverlayDelete => delete_overlay_entry(state, storage, context)?,
        Action::BeginOverlaySearch => {
            state.overlay_search.active = true;
            state.overlay_search.buffer.clear();
            state.overlay_cursor = 0;
        }
        Action::EndOverlaySearch => {
            // Leaving search clears the query, so the list the reader returns to
            // is the whole one rather than a filtered remnant.
            state.overlay_search.reset();
            state.overlay_cursor = 0;
        }
        Action::OverlaySearchChar(character) => {
            state.overlay_search.buffer.push(character);
            state.overlay_cursor = 0;
        }
        Action::OverlaySearchBackspace => {
            state.overlay_search.buffer.pop();
            state.overlay_cursor = 0;
        }
        Action::ToggleAllGroups => {
            if state.collapsed_shortcut_categories.is_empty() {
                reader_app::shortcuts_panel::collapse_all(state);
                state.status = "Folded every group.".to_owned();
            } else {
                reader_app::shortcuts_panel::expand_all(state);
                state.status = "Unfolded every group.".to_owned();
            }
            state.overlay_cursor = 0;
        }
        Action::NextTab | Action::PreviousTab => {
            if state.overlay == Overlay::Settings {
                reader_app::settings_panel::cycle_tab(state, action == Action::NextTab);
                state.overlay_cursor = 0;
            }
        }
        Action::ChangeSetting => {
            if state.overlay == Overlay::Settings {
                let cursor = state.overlay_cursor;
                if let Some(field) = reader_app::settings_panel::change(state, cursor) {
                    state.status = format!("{}: {}", field.label(), field.value(&state.settings));
                }
            }
        }
        Action::CycleLibrarySort => {
            state.library_sort_key = state.library_sort_key.next();
            state.status = format!("Sorting by {}", state.library_sort_key.label());
        }
        Action::ReverseLibrarySort => {
            state.library_sort_direction = state.library_sort_direction.reversed();
            state.status = format!("Sort direction: {}", state.library_sort_direction.as_str());
        }

        Action::ToggleFocusMode => {
            state.focus_mode = !state.focus_mode;
            if state.focus_mode {
                state.focus_block_index =
                    state.offset_to_focus_index(context.content_width, state.block_offset);
                state.status = "Focus mode on".to_owned();
            } else {
                state.status = "Focus mode off".to_owned();
            }
        }
        Action::CycleRenderMode => {
            // plain → typescript → python → rust → plain
            let next = match (state.settings.render_mode, state.settings.code_language) {
                (RenderMode::Plain, _) => "typescript",
                (RenderMode::Code, CodeLanguage::TypeScript) => "python",
                (RenderMode::Code, CodeLanguage::Python) => "rust",
                (RenderMode::Code, CodeLanguage::Rust) => "plain",
            };
            execute_command(state, storage, &format!("/mode {next}"), context)?;
        }
        Action::CycleCodeDensity => {
            // The key cycles the useful extremes, 1 → 3 → 5.
            let next = match state.settings.code_density.get() {
                1 => 3,
                2 | 3 => 5,
                _ => 1,
            };
            execute_command(state, storage, &format!("/density {next}"), context)?;
        }
        Action::CycleProgress => execute_command(state, storage, "/toggleprogress", context)?,
        Action::OpenColorSchemes => execute_command(state, storage, "/colorscheme", context)?,
        Action::OpenThemes => execute_command(state, storage, "/theme", context)?,
        Action::OpenSettings => execute_command(state, storage, "/settings", context)?,
        Action::OpenChapters => execute_command(state, storage, "/chapters", context)?,
        Action::OpenBookmarks => execute_command(state, storage, "/marks", context)?,
        Action::OpenShortcuts => execute_command(state, storage, "/keys", context)?,
        Action::RunCommand(command) => execute_command(state, storage, command, context)?,
    }

    // A command can ask for text to be typed for the reader — Toggl setup needs
    // a workspace URL pasted after the prefix it supplies.
    if let Some(prefill) = state.command_prefill.take() {
        command_bar.active = true;
        command_bar.cursor = prefill.chars().count();
        command_bar.buffer = prefill;
    }

    clamp_cursors(
        state,
        storage,
        context,
        clamp_overlay_after,
        clamp_offset_after,
    );
    Ok(())
}

/// Byte offset of a character index, so multi-byte input edits correctly.
fn char_to_byte(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map_or(text.len(), |(byte, _)| byte)
}

fn advance_search(state: &mut ReaderState, forward: bool, context: CommandContext) {
    let Some(hit) = state
        .search
        .as_mut()
        .and_then(|search| search.advance(forward))
    else {
        state.status = "No active search.".to_owned();
        return;
    };
    let position = state.search.as_ref().map_or(0, |search| search.cursor + 1);
    let total = state
        .search
        .as_ref()
        .map_or(0, |search| search.results.len());
    apply_search_hit(state, hit, context);
    state.status = format!("Match {position} of {total}");
}

/// Keep the overlay selection and the scroll offset inside their bounds.
fn clamp_cursors(
    state: &mut ReaderState,
    storage: &Storage,
    context: CommandContext,
    clamp_overlay: bool,
    clamp_offset: bool,
) {
    if clamp_overlay {
        // A closed or empty overlay has nothing to select.
        let entries = visible_entries(state, storage, context.now).len();
        state.overlay_cursor = match entries {
            0 => 0,
            count => state.overlay_cursor.min(count - 1),
        };
    }
    if clamp_offset {
        state.clamp_offset(context.content_width, context.body_height);
    }
}

/// Act on the selected overlay entry.
///
/// The row is taken from the same filtered list the frame draws, so acting on a
/// narrowed list cannot hit the item that used to be at that position.
fn confirm_overlay(
    state: &mut ReaderState,
    storage: &mut Storage,
    context: CommandContext,
) -> Result<(), reader_app::ExecutionError> {
    let Some(entry) = visible_entries(state, storage, context.now)
        .into_iter()
        .nth(state.overlay_cursor)
    else {
        state.overlay = Overlay::None;
        return Ok(());
    };

    match state.overlay {
        Overlay::Chapters => {
            let Some(index) = entry.index() else {
                return Ok(());
            };
            close_overlay(state);
            execute_command(state, storage, &format!("/goto {}", index + 1), context)?;
        }
        Overlay::Books => {
            if let Some(id) = entry.id()
                && let Some(book) = storage.book(id)?
            {
                close_overlay(state);
                reader_app::open_book(state, storage, book, context)?;
            }
        }
        Overlay::Bookmarks => {
            if let Some(book_id) = state.book_id().map(str::to_owned)
                && let Some(bookmark) = storage
                    .bookmarks(&book_id)?
                    .into_iter()
                    .find(|bookmark| Some(bookmark.id.as_str()) == entry.id())
            {
                close_overlay(state);
                state.push_nav_history();
                state.chapter_index = bookmark
                    .chapter_index
                    .min(state.chapter_count().saturating_sub(1));
                state.block_offset = bookmark.block_offset;
                state.push_nav_history();
                state.status = match bookmark.label {
                    Some(label) => format!("Jumped to {label}"),
                    None => "Jumped to bookmark".to_owned(),
                };
            }
        }
        Overlay::Notes => {
            if let Some(book_id) = state.book_id().map(str::to_owned)
                && let Some(note) = storage
                    .notes(&book_id)?
                    .into_iter()
                    .find(|note| Some(note.id.as_str()) == entry.id())
            {
                close_overlay(state);
                state.push_nav_history();
                if let Some(chapter_index) = note.chapter_index {
                    state.chapter_index =
                        chapter_index.min(state.chapter_count().saturating_sub(1));
                }
                state.block_offset = note.block_offset.unwrap_or(0);
                state.push_nav_history();
                state.status = "Jumped to note".to_owned();
            }
        }
        Overlay::ColorSchemes => {
            if let Some(scheme) = entry
                .index()
                .and_then(|index| reader_core::ColorSchemeId::ALL.get(index).copied())
            {
                close_overlay(state);
                execute_command(
                    state,
                    storage,
                    &format!("/colorscheme {}", scheme.as_str()),
                    context,
                )?;
            }
        }
        Overlay::Themes => {
            if let Some(theme) = entry.index().and_then(|index| {
                reader_core::theme::AppearanceThemeId::ALL
                    .get(index)
                    .copied()
            }) {
                close_overlay(state);
                execute_command(
                    state,
                    storage,
                    &format!("/theme {}", theme.as_str()),
                    context,
                )?;
            }
        }
        Overlay::FilePicker => {
            if let Some(discovery) = entry
                .index()
                .and_then(|index| state.discoveries.get(index).cloned())
            {
                close_overlay(state);
                let _ = reader_app::import_and_open(state, storage, &discovery.path, context);
            }
        }
        // Folding a group keeps the panel open; that is the whole interaction.
        Overlay::Keys => {
            let rows = reader_app::shortcuts_panel::rows(state);
            if let Some(row) = entry.index().and_then(|index| rows.get(index).cloned()) {
                if !reader_app::shortcuts_panel::toggle(state, &row) {
                    close_overlay(state);
                }
            } else {
                close_overlay(state);
            }
        }
        // Space changes a setting and keeps previewing; Enter accepts the whole
        // page, which is the only point the database hears about it.
        Overlay::Settings => {
            let settings = reader_app::settings_panel::save(state);
            storage
                .save_settings(&settings)
                .map_err(reader_app::ExecutionError::Storage)?;
            close_overlay(state);
            state.status = "Settings saved.".to_owned();
        }
        Overlay::Diagnostics | Overlay::Help | Overlay::None => close_overlay(state),
    }
    Ok(())
}

/// Close the overlay and forget what was filtering it.
fn close_overlay(state: &mut ReaderState) {
    state.overlay = Overlay::None;
    state.overlay_cursor = 0;
    state.overlay_search.reset();
}

fn delete_overlay_entry(
    state: &mut ReaderState,
    storage: &Storage,
    context: CommandContext,
) -> Result<(), reader_app::ExecutionError> {
    let Some(book_id) = state.book_id().map(str::to_owned) else {
        return Ok(());
    };
    let Some(id) = visible_entries(state, storage, context.now)
        .into_iter()
        .nth(state.overlay_cursor)
        .and_then(|entry| entry.id().map(str::to_owned))
    else {
        return Ok(());
    };

    // Only an entry that belongs to the open book can be deleted from here; the
    // cursor is checked against storage rather than trusted.
    match state.overlay {
        Overlay::Bookmarks => {
            let exists = storage
                .bookmarks(&book_id)?
                .iter()
                .any(|bookmark| bookmark.id == id);
            if exists {
                storage.delete_bookmark(&id)?;
                state.status = "Bookmark deleted.".to_owned();
            }
        }
        Overlay::Notes => {
            let exists = storage.notes(&book_id)?.iter().any(|note| note.id == id);
            if exists {
                storage.delete_note(&id)?;
                state.status = "Note deleted.".to_owned();
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use reader_app::{CommandContext, Overlay, ReaderState, open_book};
    use reader_core::{
        AppSettings, CanonicalBlock, CanonicalBook, CanonicalChapter, CodeLanguage, RenderMode,
    };
    use reader_storage::Storage;

    use super::{Action, CommandBar, InputMode, apply, current_mode};

    const CONTEXT: CommandContext = CommandContext {
        now: 1_700_000_000_000,
        content_width: 60,
        body_height: 10,
    };

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
                    depth: 0,
                    blocks: (0..10)
                        .map(|block| CanonicalBlock::Paragraph {
                            id: format!("b{index}-{block}"),
                            text: "The lantern swung once over the quiet harbour at dawn.".into(),
                        })
                        .collect(),
                    word_count: 100,
                })
                .collect(),
            cover_path: None,
        }
    }

    fn reader() -> (ReaderState, Storage, CommandBar) {
        let mut storage = Storage::open_in_memory().expect("database");
        let book = book();
        storage
            .save_book(&book, RenderMode::Plain, CONTEXT.now)
            .expect("save");
        let mut state = ReaderState::new(AppSettings {
            render_mode: RenderMode::Plain,
            ..AppSettings::default()
        });
        open_book(&mut state, &storage, book, CONTEXT).expect("open");
        (state, storage, CommandBar::default())
    }

    fn act(
        action: Action,
        state: &mut ReaderState,
        storage: &mut Storage,
        command_bar: &mut CommandBar,
    ) {
        apply(action, state, storage, command_bar, CONTEXT).expect("actions must not fail");
    }

    #[test]
    fn the_input_mode_follows_the_command_bar_and_overlays() {
        let (mut state, _storage, mut command_bar) = reader();
        assert_eq!(current_mode(&state, &command_bar), InputMode::Reading);

        state.overlay = Overlay::Chapters;
        assert_eq!(
            current_mode(&state, &command_bar),
            InputMode::Overlay(Overlay::Chapters)
        );

        // The command bar wins, so typing never triggers navigation.
        command_bar.active = true;
        assert_eq!(current_mode(&state, &command_bar), InputMode::Command);
    }

    #[test]
    fn scrolling_is_bounded_at_both_ends() {
        let (mut state, mut storage, mut bar) = reader();
        act(Action::ScrollUp(5), &mut state, &mut storage, &mut bar);
        assert_eq!(state.block_offset, 0, "cannot scroll above the top");

        for _ in 0..200 {
            act(Action::ScrollDown(5), &mut state, &mut storage, &mut bar);
        }
        let max = state.chapter_max_offset(CONTEXT.content_width, CONTEXT.body_height);
        assert_eq!(state.block_offset, max, "cannot scroll past the end");
    }

    #[test]
    fn non_navigation_actions_do_not_touch_layout_metrics() {
        let (mut state, mut storage, mut bar) = reader();
        let before = state.layout_metrics_cache_stats();

        act(Action::Ignore, &mut state, &mut storage, &mut bar);

        assert_eq!(state.layout_metrics_cache_stats(), before);
    }

    #[test]
    fn paging_and_jumping_record_history() {
        let (mut state, mut storage, mut bar) = reader();
        act(Action::PageDown, &mut state, &mut storage, &mut bar);
        assert!(state.block_offset > 0);
        act(Action::JumpToTop, &mut state, &mut storage, &mut bar);
        assert_eq!(state.block_offset, 0);
        act(Action::JumpToBottom, &mut state, &mut storage, &mut bar);
        let max = state.chapter_max_offset(CONTEXT.content_width, CONTEXT.body_height);
        assert_eq!(state.block_offset, max);
        assert!(state.nav_history.len() >= 3);

        act(Action::HistoryBack, &mut state, &mut storage, &mut bar);
        assert_eq!(state.status, "Went back");
        act(Action::HistoryForward, &mut state, &mut storage, &mut bar);
        assert_eq!(state.status, "Went forward");
    }

    #[test]
    fn history_at_its_edges_says_so_instead_of_moving() {
        let (mut state, mut storage, mut bar) = reader();
        act(Action::HistoryBack, &mut state, &mut storage, &mut bar);
        assert_eq!(state.status, "No earlier position in history.");
        act(Action::HistoryForward, &mut state, &mut storage, &mut bar);
        assert_eq!(state.status, "No later position in history.");
    }

    #[test]
    fn the_command_bar_edits_text_and_runs_it() {
        let (mut state, mut storage, mut bar) = reader();
        act(Action::FocusCommandBar, &mut state, &mut storage, &mut bar);
        assert!(bar.active);

        for character in "goto 2".chars() {
            act(
                Action::InsertChar(character),
                &mut state,
                &mut storage,
                &mut bar,
            );
        }
        assert_eq!(bar.buffer, "goto 2");
        assert_eq!(bar.cursor, 6);

        act(Action::MoveCursorHome, &mut state, &mut storage, &mut bar);
        act(Action::DeleteForward, &mut state, &mut storage, &mut bar);
        assert_eq!(bar.buffer, "oto 2");
        act(Action::InsertChar('g'), &mut state, &mut storage, &mut bar);
        assert_eq!(bar.buffer, "goto 2");
        act(Action::MoveCursorEnd, &mut state, &mut storage, &mut bar);
        act(Action::DeleteBackward, &mut state, &mut storage, &mut bar);
        assert_eq!(bar.buffer, "goto ");

        act(Action::InsertChar('3'), &mut state, &mut storage, &mut bar);
        act(Action::SubmitCommand, &mut state, &mut storage, &mut bar);
        assert!(!bar.active, "submitting closes the bar");
        assert_eq!(state.chapter_index, 2);
        assert_eq!(state.status, "Jumped to chapter 3");
    }

    #[test]
    fn a_submitted_line_works_with_or_without_the_leading_slash() {
        let (mut state, mut storage, mut bar) = reader();
        bar.active = true;
        bar.buffer = "/goto 2".to_owned();
        act(Action::SubmitCommand, &mut state, &mut storage, &mut bar);
        assert_eq!(state.chapter_index, 1);
    }

    #[test]
    fn an_empty_command_line_does_nothing() {
        let (mut state, mut storage, mut bar) = reader();
        bar.active = true;
        bar.buffer = "   ".to_owned();
        act(Action::SubmitCommand, &mut state, &mut storage, &mut bar);
        assert!(state.status.starts_with("Opened"), "{}", state.status);
    }

    #[test]
    fn multibyte_input_edits_by_character() {
        let (mut state, mut storage, mut bar) = reader();
        bar.active = true;
        for character in "não".chars() {
            act(
                Action::InsertChar(character),
                &mut state,
                &mut storage,
                &mut bar,
            );
        }
        assert_eq!(bar.buffer, "não");
        assert_eq!(bar.cursor, 3);
        act(Action::DeleteBackward, &mut state, &mut storage, &mut bar);
        assert_eq!(bar.buffer, "nã");
        act(Action::MoveCursorLeft, &mut state, &mut storage, &mut bar);
        act(Action::DeleteForward, &mut state, &mut storage, &mut bar);
        assert_eq!(bar.buffer, "n");
    }

    #[test]
    fn completion_fills_in_the_command_name() {
        let (mut state, mut storage, mut bar) = reader();
        bar.active = true;
        bar.buffer = "cha".to_owned();
        bar.cursor = 3;
        act(Action::CompleteCommand, &mut state, &mut storage, &mut bar);
        assert_eq!(bar.buffer, "changebook");
        assert_eq!(bar.cursor, 10);
    }

    #[test]
    fn dismiss_closes_the_command_bar_then_the_overlay_then_the_search() {
        let (mut state, mut storage, mut bar) = reader();
        state.overlay = Overlay::Chapters;
        bar.active = true;
        bar.buffer = "goto".to_owned();

        act(Action::Dismiss, &mut state, &mut storage, &mut bar);
        assert!(!bar.active);
        assert!(bar.buffer.is_empty());
        assert_eq!(
            state.overlay,
            Overlay::Chapters,
            "the overlay stays for now"
        );

        act(Action::Dismiss, &mut state, &mut storage, &mut bar);
        assert_eq!(state.overlay, Overlay::None);

        state.search = Some(reader_app::SearchState {
            query: "harbour".into(),
            global: false,
            results: Vec::new(),
            cursor: 0,
        });
        act(Action::Dismiss, &mut state, &mut storage, &mut bar);
        assert!(state.search.is_none());
        assert_eq!(state.status, "Search cleared.");
    }

    #[test]
    fn search_navigation_reports_its_position_or_says_there_is_none() {
        let (mut state, mut storage, mut bar) = reader();
        act(Action::NextSearchMatch, &mut state, &mut storage, &mut bar);
        assert_eq!(state.status, "No active search.");

        bar.active = true;
        bar.buffer = "search -g harbour".to_owned();
        act(Action::SubmitCommand, &mut state, &mut storage, &mut bar);
        let total = state.search.as_ref().expect("a search").results.len();
        assert!(total > 1, "the fixture should match repeatedly");

        act(Action::NextSearchMatch, &mut state, &mut storage, &mut bar);
        assert_eq!(state.status, format!("Match 2 of {total}"));
        act(
            Action::PreviousSearchMatch,
            &mut state,
            &mut storage,
            &mut bar,
        );
        assert_eq!(state.status, format!("Match 1 of {total}"));
    }

    #[test]
    fn the_overlay_cursor_stays_inside_the_list() {
        let (mut state, mut storage, mut bar) = reader();
        state.overlay = Overlay::Chapters;

        act(Action::OverlayUp, &mut state, &mut storage, &mut bar);
        assert_eq!(state.overlay_cursor, 0);

        act(Action::OverlayEnd, &mut state, &mut storage, &mut bar);
        assert_eq!(state.overlay_cursor, state.chapter_count() - 1);

        for _ in 0..10 {
            act(Action::OverlayDown, &mut state, &mut storage, &mut bar);
        }
        assert_eq!(state.overlay_cursor, state.chapter_count() - 1);

        act(Action::OverlayHome, &mut state, &mut storage, &mut bar);
        assert_eq!(state.overlay_cursor, 0);
    }

    #[test]
    fn confirming_a_chapter_jumps_to_it_and_closes_the_overlay() {
        let (mut state, mut storage, mut bar) = reader();
        state.overlay = Overlay::Chapters;
        state.overlay_cursor = 2;

        act(Action::OverlayConfirm, &mut state, &mut storage, &mut bar);

        assert_eq!(state.overlay, Overlay::None);
        assert_eq!(state.chapter_index, 2);
    }

    #[test]
    fn confirming_a_bookmark_jumps_to_its_place() {
        let (mut state, mut storage, mut bar) = reader();
        storage
            .add_bookmark("book", 1, 4, Some("halfway"), CONTEXT.now)
            .expect("bookmark");
        state.overlay = Overlay::Bookmarks;
        state.overlay_cursor = 0;

        act(Action::OverlayConfirm, &mut state, &mut storage, &mut bar);

        assert_eq!(state.overlay, Overlay::None);
        assert_eq!(state.chapter_index, 1);
        assert_eq!(state.block_offset, 4);
        assert_eq!(state.status, "Jumped to halfway");
    }

    #[test]
    fn deleting_from_an_overlay_removes_the_selected_entry() {
        let (mut state, mut storage, mut bar) = reader();
        storage
            .add_bookmark("book", 0, 0, Some("first"), CONTEXT.now)
            .expect("bookmark");
        storage
            .add_note("book", "a note", Some(0), Some(0), CONTEXT.now)
            .expect("note");

        state.overlay = Overlay::Bookmarks;
        act(Action::OverlayDelete, &mut state, &mut storage, &mut bar);
        assert!(storage.bookmarks("book").expect("list").is_empty());
        assert_eq!(state.status, "Bookmark deleted.");

        state.overlay = Overlay::Notes;
        act(Action::OverlayDelete, &mut state, &mut storage, &mut bar);
        assert!(storage.notes("book").expect("list").is_empty());
    }

    #[test]
    fn confirming_a_colorscheme_or_theme_applies_it() {
        let (mut state, mut storage, mut bar) = reader();
        state.overlay = Overlay::ColorSchemes;
        state.overlay_cursor = 1;
        act(Action::OverlayConfirm, &mut state, &mut storage, &mut bar);
        assert_eq!(state.settings.theme_id.as_str(), "claude");
        assert_eq!(state.overlay, Overlay::None);

        state.overlay = Overlay::Themes;
        state.overlay_cursor = 1;
        act(Action::OverlayConfirm, &mut state, &mut storage, &mut bar);
        assert_eq!(state.settings.appearance_theme_id.as_str(), "light");
    }

    #[test]
    fn the_mode_key_cycles_plain_and_the_three_languages() {
        let (mut state, mut storage, mut bar) = reader();
        let mut seen = Vec::new();
        for _ in 0..4 {
            act(Action::CycleRenderMode, &mut state, &mut storage, &mut bar);
            seen.push(match state.settings.render_mode {
                RenderMode::Plain => "plain".to_owned(),
                RenderMode::Code => state.settings.code_language.as_str().to_owned(),
            });
        }
        assert_eq!(seen, vec!["typescript", "python", "rust", "plain"]);
        assert_eq!(state.settings.code_language, CodeLanguage::Rust, "kept");
    }

    #[test]
    fn the_density_key_cycles_the_useful_extremes() {
        let (mut state, mut storage, mut bar) = reader();
        let mut seen = Vec::new();
        for _ in 0..3 {
            act(Action::CycleCodeDensity, &mut state, &mut storage, &mut bar);
            seen.push(state.settings.code_density.get());
        }
        assert_eq!(seen, vec![5, 1, 3]);
    }

    #[test]
    fn focus_mode_tracks_the_block_under_the_viewport() {
        let (mut state, mut storage, mut bar) = reader();
        state.block_offset = 6;

        act(Action::ToggleFocusMode, &mut state, &mut storage, &mut bar);
        assert!(state.focus_mode);
        assert_eq!(state.status, "Focus mode on");
        let expected_focus_index = state.offset_to_focus_index(CONTEXT.content_width, 6);
        assert_eq!(state.focus_block_index, expected_focus_index);

        act(Action::ToggleFocusMode, &mut state, &mut storage, &mut bar);
        assert!(!state.focus_mode);
        assert_eq!(state.status, "Focus mode off");
    }

    #[test]
    fn library_sorting_cycles_and_reverses() {
        let (mut state, mut storage, mut bar) = reader();
        state.overlay = Overlay::Books;

        act(Action::CycleLibrarySort, &mut state, &mut storage, &mut bar);
        assert_eq!(state.library_sort_key, reader_core::LibrarySortKey::Title);
        assert_eq!(state.status, "Sorting by Title");

        act(
            Action::ReverseLibrarySort,
            &mut state,
            &mut storage,
            &mut bar,
        );
        assert_eq!(
            state.library_sort_direction,
            reader_core::SortDirection::Ascending
        );
    }

    #[test]
    fn quitting_sets_the_flag_the_loop_watches() {
        let (mut state, mut storage, mut bar) = reader();
        act(Action::Quit, &mut state, &mut storage, &mut bar);
        assert!(state.should_quit);
    }

    #[test]
    fn every_view_key_opens_its_overlay() {
        let (mut state, mut storage, mut bar) = reader();
        for (action, expected) in [
            (Action::OpenChapters, Overlay::Chapters),
            (Action::OpenColorSchemes, Overlay::ColorSchemes),
            (Action::OpenThemes, Overlay::Themes),
            (Action::OpenSettings, Overlay::Settings),
            (Action::OpenShortcuts, Overlay::Keys),
            (Action::OpenBookmarks, Overlay::Bookmarks),
        ] {
            state.overlay = Overlay::None;
            act(action, &mut state, &mut storage, &mut bar);
            assert_eq!(state.overlay, expected);
        }
    }
}

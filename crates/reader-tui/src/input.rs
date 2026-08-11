//! Turning key presses into actions.
//!
//! Mapping is a pure function of the key and the current mode, so every binding
//! is a table test. The same key means different things while typing a command,
//! while an overlay is open, and while reading — which is exactly why the mapping
//! is separated from the handling.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use reader_app::Overlay;

/// What the reader should do next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Nothing is bound to this key here.
    Ignore,
    Quit,
    ScrollUp(usize),
    ScrollDown(usize),
    PageUp,
    PageDown,
    ChapterStart,
    ChapterEnd,
    PreviousChapter,
    NextChapter,
    JumpToTop,
    JumpToBottom,
    HistoryBack,
    HistoryForward,
    NextSearchMatch,
    PreviousSearchMatch,
    /// Open the command bar.
    FocusCommandBar,
    /// Close the overlay, or blur the command bar.
    Dismiss,
    /// Run the typed command line.
    SubmitCommand,
    /// Complete the current command line.
    CompleteCommand,
    /// Highlight the suggestion above the current one.
    CommandPrevious,
    /// Highlight the suggestion below the current one.
    CommandNext,
    /// Insert a character into the command line.
    InsertChar(char),
    /// Delete the character before the cursor.
    DeleteBackward,
    /// Delete the character under the cursor.
    DeleteForward,
    MoveCursorLeft,
    MoveCursorRight,
    MoveCursorHome,
    MoveCursorEnd,
    /// Move the overlay selection.
    OverlayUp,
    OverlayDown,
    OverlayPageUp,
    OverlayPageDown,
    OverlayHome,
    OverlayEnd,
    /// Act on the selected overlay entry.
    OverlayConfirm,
    /// Move the overlay selection to a row a pointer landed on.
    ///
    /// Clicking selects rather than confirms — except on a foldable group, where
    /// the fold is the whole interaction.
    PointerSelect(usize),
    /// Scroll to an exact offset, as a scrollbar drag asks for.
    ScrollTo(usize),
    /// Delete the selected overlay entry, where that is meaningful.
    OverlayDelete,
    /// Cycle the library sort key, in the library overlay.
    CycleLibrarySort,
    /// Reverse the library sort direction.
    ReverseLibrarySort,
    /// Start narrowing the overlay by typing.
    BeginOverlaySearch,
    /// Stop narrowing, keeping the list as it was before the query.
    EndOverlaySearch,
    /// Add a character to the overlay's query.
    OverlaySearchChar(char),
    /// Remove the last character of the overlay's query.
    OverlaySearchBackspace,
    /// Fold or unfold every group, in overlays that have them.
    ToggleAllGroups,
    /// Change the setting under the cursor, previewing it.
    ChangeSetting,
    /// Tick or untick the file under the cursor in the picker.
    TogglePickerSelection,
    /// Move between tabs, in overlays that have them.
    NextTab,
    PreviousTab,
    /// Open a tab directly, as clicking one asks for.
    SelectSettingsTab(reader_core::SettingsTab),
    ToggleFocusMode,
    CycleRenderMode,
    CycleCodeDensity,
    CycleProgress,
    OpenColorSchemes,
    OpenThemes,
    OpenSettings,
    OpenChapters,
    OpenBookmarks,
    OpenShortcuts,
    /// Run a command line directly, for keys bound to commands.
    RunCommand(&'static str),
}

/// Which key map applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// Reading, with no overlay and no command bar.
    Reading,
    /// Typing in the command bar.
    Command,
    /// An overlay has the keyboard.
    Overlay(Overlay),
    /// An overlay has the keyboard and is being narrowed by a query.
    OverlaySearch(Overlay),
}

/// How many lines a wheel notch or `j`/`k` scrolls.
pub const SCROLL_STEP: usize = 1;

/// Map a key press to an action.
#[must_use]
pub fn map_key(key: KeyEvent, mode: InputMode) -> Action {
    match mode {
        InputMode::Command => map_command_key(key),
        InputMode::Overlay(overlay) => map_overlay_key(key, overlay, false),
        InputMode::OverlaySearch(overlay) => map_overlay_key(key, overlay, true),
        InputMode::Reading => map_reading_key(key),
    }
}

fn map_command_key(key: KeyEvent) -> Action {
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Esc => Action::Dismiss,
        KeyCode::Enter => Action::SubmitCommand,
        KeyCode::Tab => Action::CompleteCommand,
        KeyCode::Up => Action::CommandPrevious,
        KeyCode::Down => Action::CommandNext,
        KeyCode::BackTab => Action::CommandPrevious,
        KeyCode::Backspace => Action::DeleteBackward,
        KeyCode::Delete => Action::DeleteForward,
        KeyCode::Left => Action::MoveCursorLeft,
        KeyCode::Right => Action::MoveCursorRight,
        KeyCode::Home => Action::MoveCursorHome,
        KeyCode::End => Action::MoveCursorEnd,
        // Ctrl+C leaves the reader even mid-command.
        KeyCode::Char('c') if control => Action::Quit,
        KeyCode::Char(character) if !control => Action::InsertChar(character),
        _ => Action::Ignore,
    }
}

fn map_overlay_key(key: KeyEvent, overlay: Overlay, searching: bool) -> Action {
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    if control && matches!(key.code, KeyCode::Char('c')) {
        return Action::Quit;
    }

    // While searching, letters narrow the list instead of navigating it.
    if searching {
        return match key.code {
            KeyCode::Esc => Action::EndOverlaySearch,
            KeyCode::Enter => Action::OverlayConfirm,
            KeyCode::Backspace => Action::OverlaySearchBackspace,
            KeyCode::Up => Action::OverlayUp,
            KeyCode::Down => Action::OverlayDown,
            KeyCode::PageUp => Action::OverlayPageUp,
            KeyCode::PageDown => Action::OverlayPageDown,
            KeyCode::Char(character) if !control => Action::OverlaySearchChar(character),
            _ => Action::Ignore,
        };
    }

    match key.code {
        KeyCode::Char('/') => Action::BeginOverlaySearch,
        KeyCode::Esc | KeyCode::Char('q') => Action::Dismiss,
        KeyCode::Enter => Action::OverlayConfirm,
        KeyCode::Up | KeyCode::Char('k') => Action::OverlayUp,
        KeyCode::Down | KeyCode::Char('j') => Action::OverlayDown,
        KeyCode::PageUp | KeyCode::Char('b') => Action::OverlayPageUp,
        // Space acts on the row wherever there is something to act on: it
        // changes a setting, ticks a file, folds a group. Only where the row has
        // nothing to toggle does it fall back to paging.
        KeyCode::Char(' ') if overlay == Overlay::Settings => Action::ChangeSetting,
        KeyCode::Char(' ') if overlay == Overlay::FilePicker => Action::TogglePickerSelection,
        KeyCode::Char(' ') if overlay == Overlay::Keys => Action::OverlayConfirm,
        KeyCode::PageDown | KeyCode::Char(' ') => Action::OverlayPageDown,
        KeyCode::Home | KeyCode::Char('g') => Action::OverlayHome,
        KeyCode::End | KeyCode::Char('G') => Action::OverlayEnd,
        // Deletion only means something where entries are the reader's own.
        KeyCode::Char('d') if matches!(overlay, Overlay::Bookmarks | Overlay::Notes) => {
            Action::OverlayDelete
        }
        KeyCode::Char('s') if overlay == Overlay::Books => Action::CycleLibrarySort,
        KeyCode::Char('r') if overlay == Overlay::Books => Action::ReverseLibrarySort,
        // Folding only means something where the list has groups.
        KeyCode::Char('z') if overlay == Overlay::Keys => Action::ToggleAllGroups,
        KeyCode::Left if overlay == Overlay::Settings => Action::PreviousTab,
        KeyCode::Right if overlay == Overlay::Settings => Action::NextTab,
        KeyCode::Char('h') if overlay == Overlay::Settings => Action::PreviousTab,
        KeyCode::Char('l') if overlay == Overlay::Settings => Action::NextTab,
        _ => Action::Ignore,
    }
}

fn map_reading_key(key: KeyEvent) -> Action {
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    if control {
        return match key.code {
            KeyCode::Char('c') => Action::Quit,
            // Ctrl+. is the documented shortcut; Ctrl+X is the fallback for
            // terminals that swallow it.
            KeyCode::Char('.' | 'x') => Action::OpenShortcuts,
            _ => Action::Ignore,
        };
    }
    match key.code {
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Char('/') => Action::FocusCommandBar,
        KeyCode::Esc => Action::Dismiss,

        KeyCode::Char('j') | KeyCode::Down => Action::ScrollDown(SCROLL_STEP),
        KeyCode::Char('k') | KeyCode::Up => Action::ScrollUp(SCROLL_STEP),
        KeyCode::Char(' ') | KeyCode::PageDown => Action::PageDown,
        KeyCode::Char('b') | KeyCode::PageUp => Action::PageUp,
        KeyCode::Home => Action::ChapterStart,
        KeyCode::End => Action::ChapterEnd,
        KeyCode::Left => Action::PreviousChapter,
        KeyCode::Right => Action::NextChapter,
        KeyCode::Char('g') => Action::JumpToTop,
        KeyCode::Char('G') => Action::JumpToBottom,
        KeyCode::Char('[') => Action::HistoryBack,
        KeyCode::Char(']') => Action::HistoryForward,
        KeyCode::Char('n') => Action::NextSearchMatch,
        KeyCode::Char('N') => Action::PreviousSearchMatch,

        KeyCode::Char('m') => Action::CycleRenderMode,
        KeyCode::Char('d') => Action::CycleCodeDensity,
        KeyCode::Char('f') => Action::ToggleFocusMode,
        KeyCode::Char('p') => Action::CycleProgress,
        KeyCode::Char('c') => Action::OpenColorSchemes,
        KeyCode::Char('C') => Action::OpenThemes,
        KeyCode::Char('S') => Action::OpenSettings,
        KeyCode::Char('T') => Action::OpenChapters,
        KeyCode::Char('B') => Action::OpenBookmarks,
        KeyCode::Char('?') => Action::OpenShortcuts,
        _ => Action::Ignore,
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use reader_app::Overlay;

    use super::{Action, InputMode, map_key};

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn control(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    #[test]
    fn reading_keys_scroll_navigate_and_quit() {
        let cases = [
            (KeyCode::Char('j'), Action::ScrollDown(1)),
            (KeyCode::Down, Action::ScrollDown(1)),
            (KeyCode::Char('k'), Action::ScrollUp(1)),
            (KeyCode::Up, Action::ScrollUp(1)),
            (KeyCode::Char(' '), Action::PageDown),
            (KeyCode::PageDown, Action::PageDown),
            (KeyCode::Char('b'), Action::PageUp),
            (KeyCode::Home, Action::ChapterStart),
            (KeyCode::End, Action::ChapterEnd),
            (KeyCode::Left, Action::PreviousChapter),
            (KeyCode::Right, Action::NextChapter),
            (KeyCode::Char('g'), Action::JumpToTop),
            (KeyCode::Char('G'), Action::JumpToBottom),
            (KeyCode::Char('['), Action::HistoryBack),
            (KeyCode::Char(']'), Action::HistoryForward),
            (KeyCode::Char('n'), Action::NextSearchMatch),
            (KeyCode::Char('N'), Action::PreviousSearchMatch),
            (KeyCode::Char('q'), Action::Quit),
            (KeyCode::Char('/'), Action::FocusCommandBar),
        ];
        for (code, expected) in cases {
            assert_eq!(
                map_key(press(code), InputMode::Reading),
                expected,
                "{code:?} in reading mode"
            );
        }
    }

    #[test]
    fn reading_keys_open_the_view_controls() {
        let cases = [
            (KeyCode::Char('m'), Action::CycleRenderMode),
            (KeyCode::Char('d'), Action::CycleCodeDensity),
            (KeyCode::Char('f'), Action::ToggleFocusMode),
            (KeyCode::Char('p'), Action::CycleProgress),
            (KeyCode::Char('c'), Action::OpenColorSchemes),
            (KeyCode::Char('C'), Action::OpenThemes),
            (KeyCode::Char('S'), Action::OpenSettings),
            (KeyCode::Char('T'), Action::OpenChapters),
            (KeyCode::Char('B'), Action::OpenBookmarks),
            (KeyCode::Char('?'), Action::OpenShortcuts),
        ];
        for (code, expected) in cases {
            assert_eq!(
                map_key(press(code), InputMode::Reading),
                expected,
                "{code:?}"
            );
        }
    }

    #[test]
    fn control_shortcuts_work_in_every_mode() {
        for mode in [
            InputMode::Reading,
            InputMode::Command,
            InputMode::Overlay(Overlay::Chapters),
        ] {
            assert_eq!(
                map_key(control(KeyCode::Char('c')), mode),
                Action::Quit,
                "{mode:?} should still allow Ctrl+C"
            );
        }
        assert_eq!(
            map_key(control(KeyCode::Char('.')), InputMode::Reading),
            Action::OpenShortcuts
        );
        assert_eq!(
            map_key(control(KeyCode::Char('x')), InputMode::Reading),
            Action::OpenShortcuts,
            "Ctrl+X is the fallback where Ctrl+. is swallowed"
        );
    }

    #[test]
    fn command_mode_types_text_instead_of_navigating() {
        assert_eq!(
            map_key(press(KeyCode::Char('j')), InputMode::Command),
            Action::InsertChar('j'),
            "letters are text while typing a command"
        );
        assert_eq!(
            map_key(press(KeyCode::Char(' ')), InputMode::Command),
            Action::InsertChar(' ')
        );
        assert_eq!(
            map_key(press(KeyCode::Enter), InputMode::Command),
            Action::SubmitCommand
        );
        assert_eq!(
            map_key(press(KeyCode::Tab), InputMode::Command),
            Action::CompleteCommand
        );
        assert_eq!(
            map_key(press(KeyCode::Esc), InputMode::Command),
            Action::Dismiss
        );
    }

    #[test]
    fn command_mode_edits_the_line() {
        let cases = [
            (KeyCode::Backspace, Action::DeleteBackward),
            (KeyCode::Delete, Action::DeleteForward),
            (KeyCode::Left, Action::MoveCursorLeft),
            (KeyCode::Right, Action::MoveCursorRight),
            (KeyCode::Home, Action::MoveCursorHome),
            (KeyCode::End, Action::MoveCursorEnd),
        ];
        for (code, expected) in cases {
            assert_eq!(
                map_key(press(code), InputMode::Command),
                expected,
                "{code:?}"
            );
        }
    }

    #[test]
    fn overlay_keys_move_the_selection_and_close() {
        let mode = InputMode::Overlay(Overlay::Chapters);
        let cases = [
            (KeyCode::Up, Action::OverlayUp),
            (KeyCode::Char('k'), Action::OverlayUp),
            (KeyCode::Down, Action::OverlayDown),
            (KeyCode::Char('j'), Action::OverlayDown),
            (KeyCode::PageUp, Action::OverlayPageUp),
            (KeyCode::PageDown, Action::OverlayPageDown),
            (KeyCode::Char(' '), Action::OverlayPageDown),
            (KeyCode::Home, Action::OverlayHome),
            (KeyCode::End, Action::OverlayEnd),
            (KeyCode::Enter, Action::OverlayConfirm),
            (KeyCode::Esc, Action::Dismiss),
            (KeyCode::Char('q'), Action::Dismiss),
        ];
        for (code, expected) in cases {
            assert_eq!(map_key(press(code), mode), expected, "{code:?}");
        }
    }

    #[test]
    fn deletion_and_sorting_are_bound_only_where_they_mean_something() {
        for overlay in [Overlay::Bookmarks, Overlay::Notes] {
            assert_eq!(
                map_key(press(KeyCode::Char('d')), InputMode::Overlay(overlay)),
                Action::OverlayDelete,
                "{overlay:?} entries can be deleted"
            );
        }
        assert_eq!(
            map_key(
                press(KeyCode::Char('d')),
                InputMode::Overlay(Overlay::Chapters)
            ),
            Action::Ignore,
            "a chapter is not the reader's to delete"
        );

        assert_eq!(
            map_key(
                press(KeyCode::Char('s')),
                InputMode::Overlay(Overlay::Books)
            ),
            Action::CycleLibrarySort
        );
        assert_eq!(
            map_key(
                press(KeyCode::Char('r')),
                InputMode::Overlay(Overlay::Books)
            ),
            Action::ReverseLibrarySort
        );
        assert_eq!(
            map_key(
                press(KeyCode::Char('s')),
                InputMode::Overlay(Overlay::Notes)
            ),
            Action::Ignore
        );
    }

    #[test]
    fn slash_starts_a_search_inside_an_overlay() {
        assert_eq!(
            map_key(
                press(KeyCode::Char('/')),
                InputMode::Overlay(Overlay::Chapters)
            ),
            Action::BeginOverlaySearch
        );
    }

    #[test]
    fn while_searching_letters_narrow_the_list_instead_of_navigating() {
        let mode = InputMode::OverlaySearch(Overlay::Chapters);
        assert_eq!(
            map_key(press(KeyCode::Char('j')), mode),
            Action::OverlaySearchChar('j'),
            "j is a letter here, not a movement"
        );
        assert_eq!(
            map_key(press(KeyCode::Char('q')), mode),
            Action::OverlaySearchChar('q'),
            "and q does not close the overlay"
        );
        assert_eq!(
            map_key(press(KeyCode::Backspace), mode),
            Action::OverlaySearchBackspace
        );
        assert_eq!(map_key(press(KeyCode::Esc), mode), Action::EndOverlaySearch);
        assert_eq!(map_key(press(KeyCode::Enter), mode), Action::OverlayConfirm);
    }

    #[test]
    fn arrows_still_move_the_selection_while_searching() {
        let mode = InputMode::OverlaySearch(Overlay::Books);
        assert_eq!(map_key(press(KeyCode::Down), mode), Action::OverlayDown);
        assert_eq!(map_key(press(KeyCode::Up), mode), Action::OverlayUp);
        assert_eq!(
            map_key(press(KeyCode::PageDown), mode),
            Action::OverlayPageDown
        );
    }

    #[test]
    fn folding_and_tabs_are_bound_only_where_they_exist() {
        assert_eq!(
            map_key(press(KeyCode::Char('z')), InputMode::Overlay(Overlay::Keys)),
            Action::ToggleAllGroups
        );
        assert_eq!(
            map_key(
                press(KeyCode::Char('z')),
                InputMode::Overlay(Overlay::Books)
            ),
            Action::Ignore,
            "the library has no groups to fold"
        );

        for (code, expected) in [
            (KeyCode::Right, Action::NextTab),
            (KeyCode::Left, Action::PreviousTab),
            (KeyCode::Char('l'), Action::NextTab),
            (KeyCode::Char('h'), Action::PreviousTab),
        ] {
            assert_eq!(
                map_key(press(code), InputMode::Overlay(Overlay::Settings)),
                expected,
                "{code:?} in settings"
            );
        }
        assert_eq!(
            map_key(press(KeyCode::Right), InputMode::Overlay(Overlay::Chapters)),
            Action::Ignore,
            "other overlays have no tabs"
        );
    }

    #[test]
    fn space_changes_a_setting_but_pages_everywhere_else() {
        assert_eq!(
            map_key(
                press(KeyCode::Char(' ')),
                InputMode::Overlay(Overlay::Settings)
            ),
            Action::ChangeSetting
        );
        assert_eq!(
            map_key(
                press(KeyCode::Char(' ')),
                InputMode::Overlay(Overlay::Chapters)
            ),
            Action::OverlayPageDown
        );
    }

    #[test]
    fn unbound_keys_are_ignored_rather_than_guessed_at() {
        for mode in [
            InputMode::Reading,
            InputMode::Command,
            InputMode::Overlay(Overlay::Chapters),
        ] {
            assert_eq!(
                map_key(press(KeyCode::F(5)), mode),
                Action::Ignore,
                "{mode:?}"
            );
            assert_eq!(
                map_key(control(KeyCode::Char('z')), mode),
                Action::Ignore,
                "{mode:?}"
            );
        }
    }
}

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
    /// Delete the selected overlay entry, where that is meaningful.
    OverlayDelete,
    /// Cycle the library sort key, in the library overlay.
    CycleLibrarySort,
    /// Reverse the library sort direction.
    ReverseLibrarySort,
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
}

/// How many lines a wheel notch or `j`/`k` scrolls.
pub const SCROLL_STEP: usize = 1;

/// Map a key press to an action.
#[must_use]
pub fn map_key(key: KeyEvent, mode: InputMode) -> Action {
    match mode {
        InputMode::Command => map_command_key(key),
        InputMode::Overlay(overlay) => map_overlay_key(key, overlay),
        InputMode::Reading => map_reading_key(key),
    }
}

fn map_command_key(key: KeyEvent) -> Action {
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Esc => Action::Dismiss,
        KeyCode::Enter => Action::SubmitCommand,
        KeyCode::Tab => Action::CompleteCommand,
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

fn map_overlay_key(key: KeyEvent, overlay: Overlay) -> Action {
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    if control && matches!(key.code, KeyCode::Char('c')) {
        return Action::Quit;
    }
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => Action::Dismiss,
        KeyCode::Enter => Action::OverlayConfirm,
        KeyCode::Up | KeyCode::Char('k') => Action::OverlayUp,
        KeyCode::Down | KeyCode::Char('j') => Action::OverlayDown,
        KeyCode::PageUp | KeyCode::Char('b') => Action::OverlayPageUp,
        KeyCode::PageDown | KeyCode::Char(' ') => Action::OverlayPageDown,
        KeyCode::Home | KeyCode::Char('g') => Action::OverlayHome,
        KeyCode::End | KeyCode::Char('G') => Action::OverlayEnd,
        // Deletion only means something where entries are the reader's own.
        KeyCode::Char('d') if matches!(overlay, Overlay::Bookmarks | Overlay::Notes) => {
            Action::OverlayDelete
        }
        KeyCode::Char('s') if overlay == Overlay::Books => Action::CycleLibrarySort,
        KeyCode::Char('r') if overlay == Overlay::Books => Action::ReverseLibrarySort,
        KeyCode::Char('/') => Action::FocusCommandBar,
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

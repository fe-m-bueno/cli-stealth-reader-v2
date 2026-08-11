//! Keyboard shortcut catalogue shown by the shortcuts overlay.

/// Grouping used by the overlay's collapsible sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ShortcutCategory {
    /// The handful of keys a reader needs before anything else. This group is
    /// listed first and opened by default; the rest start folded.
    Essentials,
    Navigation,
    Commands,
    View,
}

impl ShortcutCategory {
    /// Display order of the sections.
    pub const ALL: [Self; 4] = [
        Self::Essentials,
        Self::Navigation,
        Self::Commands,
        Self::View,
    ];

    /// Human-readable name of the section.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Essentials => "Essentials",
            Self::Navigation => "Navigation",
            Self::Commands => "Commands",
            Self::View => "View",
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Essentials => "essentials",
            Self::Navigation => "navigation",
            Self::Commands => "commands",
            Self::View => "view",
        }
    }

    #[must_use]
    pub fn from_id(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == value)
    }
}

/// What the shortcuts panel runs when a binding is confirmed or clicked.
///
/// The catalogue names the effect; the front end owns the action type, so this
/// stays a vocabulary both sides agree on rather than a dependency either way.
/// A binding that only means something in a particular place — Enter, Esc, the
/// keys that act on the row under the cursor — has no entry here, because
/// running it from the panel would act on the panel instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutAction {
    ScrollUp,
    ScrollDown,
    PageUp,
    PageDown,
    ChapterStart,
    ChapterEnd,
    JumpToTop,
    JumpToBottom,
    FocusCommandBar,
    OpenChapters,
    OpenBookmarks,
    OpenColorSchemes,
    OpenThemes,
    OpenSettings,
    CycleRenderMode,
    CycleProgress,
    ToggleFocusMode,
    Quit,
}

/// One documented shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shortcut {
    pub category: ShortcutCategory,
    pub key: &'static str,
    pub description: &'static str,
    /// What confirming the row in the panel does, when the binding can be run
    /// from there at all.
    pub action: Option<ShortcutAction>,
}

const fn shortcut(
    category: ShortcutCategory,
    key: &'static str,
    description: &'static str,
) -> Shortcut {
    Shortcut {
        category,
        key,
        description,
        action: None,
    }
}

/// A shortcut the panel can run itself.
const fn runnable(
    category: ShortcutCategory,
    key: &'static str,
    description: &'static str,
    action: ShortcutAction,
) -> Shortcut {
    Shortcut {
        category,
        key,
        description,
        action: Some(action),
    }
}

use ShortcutAction as Run;
use ShortcutCategory::{Commands, Essentials, Navigation, View};

/// The catalogue, in the order the overlay lists it.
pub const KEYBOARD_SHORTCUTS: [Shortcut; 30] = [
    runnable(Navigation, "k / ↑", "Scroll up", Run::ScrollUp),
    runnable(Navigation, "j / ↓", "Scroll down", Run::ScrollDown),
    runnable(
        Navigation,
        "space / PgDn",
        "Page down / toggle picker selection",
        Run::PageDown,
    ),
    runnable(Navigation, "b / PgUp", "Page up", Run::PageUp),
    runnable(
        Navigation,
        "Home",
        "Jump to chapter start",
        Run::ChapterStart,
    ),
    runnable(Navigation, "End", "Jump to chapter end", Run::ChapterEnd),
    shortcut(Navigation, "← / →", "Previous / next chapter"),
    runnable(
        Navigation,
        "Shift+T",
        "Open table of contents",
        Run::OpenChapters,
    ),
    runnable(Navigation, "Shift+B", "Open bookmarks", Run::OpenBookmarks),
    shortcut(Navigation, "[ / ]", "Back / forward in navigation history"),
    shortcut(Navigation, "wheel", "Scroll the page"),
    runnable(Navigation, "g", "Jump to top", Run::JumpToTop),
    runnable(Navigation, "Shift+G", "Jump to bottom", Run::JumpToBottom),
    shortcut(
        Commands,
        "n / Shift+N",
        "Next / previous search match (after /search)",
    ),
    runnable(Essentials, "/", "Focus command bar", Run::FocusCommandBar),
    shortcut(Essentials, "Enter", "Run command / confirm picker"),
    shortcut(Essentials, "Esc", "Close overlay or blur command input"),
    shortcut(
        Commands,
        "d",
        "Delete selected bookmark (inside bookmark overlay)",
    ),
    shortcut(Commands, "s (books)", "Cycle sort key in book library"),
    shortcut(
        Commands,
        "r (books)",
        "Reverse sort direction in book library",
    ),
    shortcut(Essentials, "? / Ctrl+. / Ctrl+X", "Open keyboard shortcuts"),
    runnable(
        View,
        "m",
        "Cycle render mode (plain → typescript → python → rust)",
        Run::CycleRenderMode,
    ),
    runnable(
        View,
        "f",
        "Focus mode: dim all but one block (j/k move it, Esc leaves)",
        Run::ToggleFocusMode,
    ),
    runnable(View, "c", "Open colorscheme picker", Run::OpenColorSchemes),
    runnable(View, "Shift+C", "Open theme picker", Run::OpenThemes),
    runnable(
        Essentials,
        "Shift+S",
        "Open tabbed reader settings with live preview",
        Run::OpenSettings,
    ),
    runnable(
        View,
        "p",
        "Cycle progress display (time left / % bars / hidden)",
        Run::CycleProgress,
    ),
    shortcut(View, "Tab", "Autocomplete or cycle command suggestions"),
    shortcut(View, "↑/↓", "Move through the command palette"),
    runnable(Essentials, "q", "Quit the reader", Run::Quit),
];

/// Shortcuts of one category, in catalogue order.
#[must_use]
pub fn shortcuts_in(category: ShortcutCategory) -> Vec<Shortcut> {
    KEYBOARD_SHORTCUTS
        .into_iter()
        .filter(|entry| entry.category == category)
        .collect()
}

/// Shortcuts whose key or description contains `query`, case-insensitively.
#[must_use]
pub fn search_shortcuts(query: &str) -> Vec<Shortcut> {
    if query.is_empty() {
        return KEYBOARD_SHORTCUTS.to_vec();
    }
    let needle = query.to_lowercase();
    KEYBOARD_SHORTCUTS
        .into_iter()
        .filter(|entry| {
            entry.key.to_lowercase().contains(&needle)
                || entry.description.to_lowercase().contains(&needle)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        KEYBOARD_SHORTCUTS, ShortcutAction, ShortcutCategory, search_shortcuts, shortcuts_in,
    };

    #[test]
    fn every_shortcut_is_documented_and_categorized() {
        assert_eq!(KEYBOARD_SHORTCUTS.len(), 30);
        for entry in KEYBOARD_SHORTCUTS {
            assert!(!entry.key.is_empty());
            assert!(!entry.description.is_empty());
        }
        let grouped: usize = ShortcutCategory::ALL
            .into_iter()
            .map(|category| shortcuts_in(category).len())
            .sum();
        assert_eq!(grouped, KEYBOARD_SHORTCUTS.len());
    }

    #[test]
    fn the_essentials_group_leads_with_the_keys_a_reader_needs_first() {
        assert_eq!(ShortcutCategory::ALL[0], ShortcutCategory::Essentials);
        let essentials: Vec<&str> = shortcuts_in(ShortcutCategory::Essentials)
            .into_iter()
            .map(|shortcut| shortcut.key)
            .collect();
        assert_eq!(
            essentials,
            vec!["/", "Enter", "Esc", "? / Ctrl+. / Ctrl+X", "Shift+S", "q"]
        );
    }

    #[test]
    fn every_category_has_a_name_and_an_identifier() {
        for category in ShortcutCategory::ALL {
            assert!(!category.label().is_empty());
            assert_eq!(
                ShortcutCategory::from_id(category.as_str()),
                Some(category),
                "{category:?} should round-trip through its id"
            );
        }
    }

    #[test]
    fn categories_keep_catalogue_order() {
        let navigation = shortcuts_in(ShortcutCategory::Navigation);
        assert_eq!(navigation.first().expect("non-empty").key, "k / ↑");
        assert_eq!(navigation.last().expect("non-empty").key, "Shift+G");
    }

    #[test]
    fn the_scroll_keys_are_documented_in_the_direction_they_move() {
        let by_key = |key: &str| {
            KEYBOARD_SHORTCUTS
                .into_iter()
                .find(|entry| entry.key == key)
                .expect("a documented key")
        };
        assert_eq!(by_key("j / ↓").description, "Scroll down");
        assert_eq!(by_key("j / ↓").action, Some(ShortcutAction::ScrollDown));
        assert_eq!(by_key("k / ↑").description, "Scroll up");
        assert_eq!(by_key("k / ↑").action, Some(ShortcutAction::ScrollUp));
    }

    #[test]
    fn only_context_free_bindings_can_be_run_from_the_panel() {
        let action_of = |key: &str| {
            KEYBOARD_SHORTCUTS
                .into_iter()
                .find(|entry| entry.key == key)
                .expect("a documented key")
                .action
        };
        assert_eq!(action_of("f"), Some(ShortcutAction::ToggleFocusMode));
        assert_eq!(action_of("Shift+T"), Some(ShortcutAction::OpenChapters));
        // Enter, Esc, and the row-scoped keys only mean something where they are
        // pressed, so the panel has nothing to run for them.
        for key in ["Enter", "Esc", "d", "Tab", "wheel"] {
            assert_eq!(action_of(key), None, "{key} should not be runnable");
        }
    }

    #[test]
    fn search_matches_keys_and_descriptions_case_insensitively() {
        assert_eq!(search_shortcuts("").len(), KEYBOARD_SHORTCUTS.len());
        let bookmarks = search_shortcuts("BOOKMARK");
        assert_eq!(bookmarks.len(), 2);
        let by_key = search_shortcuts("shift+g");
        assert_eq!(by_key.len(), 1);
        assert_eq!(by_key[0].description, "Jump to bottom");
        assert!(search_shortcuts("nonexistent").is_empty());
    }
}

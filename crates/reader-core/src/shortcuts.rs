//! Keyboard shortcut catalogue shown by the shortcuts overlay.

/// Grouping used by the overlay's collapsible sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ShortcutCategory {
    Navigation,
    Commands,
    View,
}

impl ShortcutCategory {
    /// Display order of the sections.
    pub const ALL: [Self; 3] = [Self::Navigation, Self::Commands, Self::View];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
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

/// One documented shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shortcut {
    pub category: ShortcutCategory,
    pub key: &'static str,
    pub description: &'static str,
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
    }
}

use ShortcutCategory::{Commands, Navigation, View};

/// The catalogue, in the order the overlay lists it.
pub const KEYBOARD_SHORTCUTS: [Shortcut; 29] = [
    shortcut(Navigation, "j / ↑", "Scroll up"),
    shortcut(Navigation, "k / ↓", "Scroll down"),
    shortcut(
        Navigation,
        "space / PgDn",
        "Page down / toggle picker selection",
    ),
    shortcut(Navigation, "b / PgUp", "Page up"),
    shortcut(Navigation, "Home", "Jump to chapter start"),
    shortcut(Navigation, "End", "Jump to chapter end"),
    shortcut(Navigation, "← / →", "Previous / next chapter"),
    shortcut(Navigation, "Shift+T", "Open table of contents"),
    shortcut(Navigation, "Shift+B", "Open bookmarks"),
    shortcut(Navigation, "[ / ]", "Back / forward in navigation history"),
    shortcut(Navigation, "wheel", "Scroll the page"),
    shortcut(Navigation, "g", "Jump to top"),
    shortcut(Navigation, "Shift+G", "Jump to bottom"),
    shortcut(
        Commands,
        "n / Shift+N",
        "Next / previous search match (after /search)",
    ),
    shortcut(Commands, "/", "Focus command bar"),
    shortcut(Commands, "Enter", "Run command / confirm picker"),
    shortcut(Commands, "Esc", "Close overlay or blur command input"),
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
    shortcut(Commands, "? / Ctrl+. / Ctrl+X", "Open keyboard shortcuts"),
    shortcut(
        View,
        "m",
        "Cycle render mode (plain → typescript → python → rust)",
    ),
    shortcut(View, "f", "Toggle focus mode (single block centered)"),
    shortcut(View, "c", "Open colorscheme picker"),
    shortcut(View, "Shift+C", "Open theme picker"),
    shortcut(
        View,
        "Shift+S",
        "Open tabbed reader settings with live preview",
    ),
    shortcut(
        View,
        "p",
        "Cycle progress display (time left / % bars / hidden)",
    ),
    shortcut(View, "Tab", "Autocomplete or cycle command suggestions"),
    shortcut(View, "q", "Quit the reader"),
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
    use super::{KEYBOARD_SHORTCUTS, ShortcutCategory, search_shortcuts, shortcuts_in};

    #[test]
    fn every_shortcut_is_documented_and_categorized() {
        assert_eq!(KEYBOARD_SHORTCUTS.len(), 29);
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
    fn categories_keep_catalogue_order() {
        let navigation = shortcuts_in(ShortcutCategory::Navigation);
        assert_eq!(navigation.first().expect("non-empty").key, "j / ↑");
        assert_eq!(navigation.last().expect("non-empty").key, "Shift+G");
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

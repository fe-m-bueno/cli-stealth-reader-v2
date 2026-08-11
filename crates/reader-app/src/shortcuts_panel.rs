//! The keyboard-shortcut panel.
//!
//! Twenty-nine bindings do not fit a small terminal, so they are grouped by
//! category and each group can be folded away. Folding is per session rather
//! than persisted: it is a way to see the list, not a preference.

use std::collections::BTreeSet;

use reader_core::{KEYBOARD_SHORTCUTS, ShortcutCategory};

use crate::state::ReaderState;

/// One line of the panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelRow {
    pub display: String,
    /// What a query matches against. A category header matches its own name so
    /// searching for "navigation" keeps the group visible.
    pub search: String,
    pub kind: RowKind,
}

/// Whether a row is a group header or a binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    Header(ShortcutCategory),
    Binding,
}

/// Human-readable name of a category.
#[must_use]
pub const fn category_label(category: ShortcutCategory) -> &'static str {
    match category {
        ShortcutCategory::Navigation => "Navigation",
        ShortcutCategory::Commands => "Commands",
        ShortcutCategory::View => "View",
    }
}

/// The panel's rows for the current fold state.
#[must_use]
pub fn rows(state: &ReaderState) -> Vec<PanelRow> {
    let mut rows = Vec::new();
    for category in ShortcutCategory::ALL {
        let bindings: Vec<_> = KEYBOARD_SHORTCUTS
            .into_iter()
            .filter(|shortcut| shortcut.category == category)
            .collect();
        let collapsed = state.collapsed_shortcut_categories.contains(&category);
        let marker = if collapsed { '▸' } else { '▾' };
        let label = category_label(category);

        rows.push(PanelRow {
            display: format!("{marker} {label} ({})", bindings.len()),
            search: label.to_owned(),
            kind: RowKind::Header(category),
        });
        if collapsed {
            continue;
        }
        rows.extend(bindings.into_iter().map(|shortcut| PanelRow {
            display: format!("    {:<22} {}", shortcut.key, shortcut.description),
            search: format!("{} {} {label}", shortcut.key, shortcut.description),
            kind: RowKind::Binding,
        }));
    }
    rows
}

/// Fold or unfold the category a header row belongs to.
///
/// Returns whether anything changed, so the caller can leave the status line
/// alone when the reader confirmed a binding rather than a header.
pub fn toggle(state: &mut ReaderState, row: &PanelRow) -> bool {
    let RowKind::Header(category) = row.kind else {
        return false;
    };
    if !state.collapsed_shortcut_categories.remove(&category) {
        state.collapsed_shortcut_categories.insert(category);
    }
    true
}

/// Fold every category, so the panel shows only its three headings.
pub fn collapse_all(state: &mut ReaderState) {
    state.collapsed_shortcut_categories = ShortcutCategory::ALL.into_iter().collect();
}

/// Unfold every category.
pub fn expand_all(state: &mut ReaderState) {
    state.collapsed_shortcut_categories = BTreeSet::new();
}

#[cfg(test)]
mod tests {
    use reader_core::{AppSettings, KEYBOARD_SHORTCUTS, ShortcutCategory};

    use super::{PanelRow, RowKind, collapse_all, expand_all, rows, toggle};
    use crate::overlay;
    use crate::state::{Overlay, ReaderState};

    fn reader() -> ReaderState {
        let mut state = ReaderState::new(AppSettings::default());
        state.overlay = Overlay::Keys;
        state
    }

    fn headers(rows: &[PanelRow]) -> Vec<&str> {
        rows.iter()
            .filter(|row| matches!(row.kind, RowKind::Header(_)))
            .map(|row| row.display.as_str())
            .collect()
    }

    fn bindings(rows: &[PanelRow]) -> usize {
        rows.iter()
            .filter(|row| row.kind == RowKind::Binding)
            .count()
    }

    #[test]
    fn every_binding_appears_under_its_category() {
        let state = reader();
        let rows = rows(&state);

        assert_eq!(headers(&rows).len(), 3);
        assert_eq!(bindings(&rows), KEYBOARD_SHORTCUTS.len());
        assert!(
            rows[0].display.starts_with("▾ Navigation ("),
            "{:?}",
            rows[0]
        );
        assert!(rows[1].display.contains("Scroll up"), "{:?}", rows[1]);
    }

    #[test]
    fn folding_a_category_hides_its_bindings_and_keeps_the_others() {
        let mut state = reader();
        let header = rows(&state)[0].clone();

        assert!(toggle(&mut state, &header), "a header toggles");
        let folded = rows(&state);

        assert!(
            folded[0].display.starts_with("▸ Navigation ("),
            "{:?}",
            folded[0]
        );
        assert_eq!(
            bindings(&folded),
            KEYBOARD_SHORTCUTS
                .into_iter()
                .filter(|shortcut| shortcut.category != ShortcutCategory::Navigation)
                .count()
        );
        assert_eq!(headers(&folded).len(), 3, "the headings stay");
    }

    #[test]
    fn folding_is_reversible() {
        let mut state = reader();
        let header = rows(&state)[0].clone();
        toggle(&mut state, &header);
        toggle(&mut state, &header);
        assert_eq!(bindings(&rows(&state)), KEYBOARD_SHORTCUTS.len());
    }

    #[test]
    fn confirming_a_binding_row_changes_nothing() {
        let mut state = reader();
        let binding = rows(&state)
            .into_iter()
            .find(|row| row.kind == RowKind::Binding)
            .expect("a binding row");

        assert!(!toggle(&mut state, &binding), "a binding is not foldable");
        assert!(state.collapsed_shortcut_categories.is_empty());
    }

    #[test]
    fn folding_everything_leaves_only_the_headings() {
        let mut state = reader();
        collapse_all(&mut state);
        let folded = rows(&state);
        assert_eq!(folded.len(), 3);
        assert_eq!(bindings(&folded), 0);

        expand_all(&mut state);
        assert_eq!(bindings(&rows(&state)), KEYBOARD_SHORTCUTS.len());
    }

    #[test]
    fn searching_finds_bindings_by_key_description_or_category() {
        let mut state = reader();
        let storage = reader_storage::Storage::open_in_memory().expect("database");

        for (query, expected) in [("bookmark", 2), ("Shift+G", 1)] {
            state.overlay_search.buffer = query.to_owned();
            let found = overlay::visible_entries(&state, &storage, 0);
            let matches = found
                .iter()
                .filter(|entry| entry.display.trim_start().starts_with(|c: char| c != '▾'))
                .count();
            assert!(matches >= expected, "{query:?} found {found:?}");
        }

        // A category name keeps its whole group reachable.
        state.overlay_search.buffer = "navigation".into();
        let by_category = overlay::visible_entries(&state, &storage, 0);
        assert!(
            by_category.len() > 5,
            "searching a category should keep its bindings: {by_category:?}"
        );
    }
}

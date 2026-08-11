//! The keyboard-shortcut panel.
//!
//! Twenty-nine bindings do not fit a small terminal, so they are grouped by
//! category and each group can be folded away. Only Essentials is open when the
//! panel appears — the six keys a reader needs before anything else — and the
//! rest are one keypress away. Folding is per session rather than persisted: it
//! is a way to see the list, not a preference.
//!
//! The query is applied here rather than by the generic overlay filter, because
//! a filtered list still has to keep each match under its own heading.

use std::collections::BTreeSet;

use reader_core::{KEYBOARD_SHORTCUTS, ShortcutAction, ShortcutCategory};

use crate::state::ReaderState;

/// One line of the panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelRow {
    /// The heading, or the description of a binding.
    pub label: String,
    /// The keys that trigger a binding; empty on a heading.
    pub key: String,
    /// What a query matches against. A category header matches its own name so
    /// searching for "navigation" keeps the group visible.
    pub search: String,
    pub kind: RowKind,
    /// What confirming the row runs, when the binding can be run from the panel.
    /// Headers, and keys that only mean something where they are pressed, have
    /// none.
    pub action: Option<ShortcutAction>,
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
    category.label()
}

/// Categories folded when the panel is opened: everything but Essentials.
#[must_use]
pub fn default_collapsed() -> BTreeSet<ShortcutCategory> {
    ShortcutCategory::ALL
        .into_iter()
        .filter(|category| *category != ShortcutCategory::Essentials)
        .collect()
}

/// Open the panel with Essentials showing and the rest folded away.
pub fn open(state: &mut ReaderState) {
    state.collapsed_shortcut_categories = default_collapsed();
}

/// The panel's rows for the current fold state and query.
///
/// A query unfolds every group that has a match, because hiding a match behind a
/// fold would make the search look broken.
#[must_use]
pub fn rows(state: &ReaderState) -> Vec<PanelRow> {
    let query = state.overlay_search.query().to_lowercase();
    let mut rows = Vec::new();
    for category in ShortcutCategory::ALL {
        let bindings: Vec<_> = KEYBOARD_SHORTCUTS
            .into_iter()
            .filter(|shortcut| shortcut.category == category)
            .collect();
        let label = category_label(category);
        let matches: Vec<_> = if query.is_empty() {
            bindings.clone()
        } else {
            bindings
                .iter()
                .copied()
                .filter(|shortcut| {
                    let haystack =
                        format!("{} {} {label}", shortcut.key, shortcut.description).to_lowercase();
                    haystack.contains(&query)
                })
                .collect()
        };
        if !query.is_empty() && matches.is_empty() {
            continue;
        }

        let collapsed = query.is_empty() && state.collapsed_shortcut_categories.contains(&category);
        let marker = if collapsed { '›' } else { '◆' };
        rows.push(PanelRow {
            label: format!("{marker} {label} ({})", bindings.len()),
            key: String::new(),
            search: label.to_owned(),
            kind: RowKind::Header(category),
            action: None,
        });
        if collapsed {
            continue;
        }
        rows.extend(matches.into_iter().map(|shortcut| PanelRow {
            label: shortcut.description.to_owned(),
            key: shortcut.key.to_owned(),
            search: format!("{} {} {label}", shortcut.key, shortcut.description),
            kind: RowKind::Binding,
            action: shortcut.action,
        }));
    }
    rows
}

/// Fold or unfold the category a header row belongs to.
///
/// Returns whether anything changed, so a caller that confirmed a binding
/// rather than a header knows to run the binding instead.
pub fn toggle(state: &mut ReaderState, row: &PanelRow) -> bool {
    let RowKind::Header(category) = row.kind else {
        return false;
    };
    if !state.collapsed_shortcut_categories.remove(&category) {
        state.collapsed_shortcut_categories.insert(category);
    }
    true
}

/// Fold every category, so the panel shows only its headings.
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

    use super::{PanelRow, RowKind, collapse_all, expand_all, open, rows, toggle};
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
            .map(|row| row.label.as_str())
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

        assert_eq!(headers(&rows).len(), 4);
        assert_eq!(bindings(&rows), KEYBOARD_SHORTCUTS.len());
        assert!(rows[0].label.starts_with("◆ Essentials ("), "{:?}", rows[0]);
        assert_eq!(rows[1].label, "Focus command bar", "{:?}", rows[1]);
        assert_eq!(rows[1].key, "/", "the key is its own column");
    }

    #[test]
    fn opening_the_panel_shows_the_essentials_and_folds_the_rest() {
        let mut state = reader();
        open(&mut state);
        let rows = rows(&state);

        assert_eq!(headers(&rows).len(), 4);
        assert_eq!(
            bindings(&rows),
            KEYBOARD_SHORTCUTS
                .into_iter()
                .filter(|shortcut| shortcut.category == ShortcutCategory::Essentials)
                .count()
        );
        assert!(rows[0].label.starts_with("◆ Essentials"), "{:?}", rows[0]);
        assert!(
            rows.iter().any(|row| row.label.starts_with("› Navigation")),
            "folded groups keep their marker: {rows:?}"
        );
    }

    #[test]
    fn folding_a_category_hides_its_bindings_and_keeps_the_others() {
        let mut state = reader();
        let header = rows(&state)
            .into_iter()
            .find(|row| row.kind == RowKind::Header(ShortcutCategory::Navigation))
            .expect("a navigation header");

        assert!(toggle(&mut state, &header), "a header toggles");
        let folded = rows(&state);

        assert!(
            folded
                .iter()
                .any(|row| row.label.starts_with("› Navigation (")),
            "{folded:?}"
        );
        assert_eq!(
            bindings(&folded),
            KEYBOARD_SHORTCUTS
                .into_iter()
                .filter(|shortcut| shortcut.category != ShortcutCategory::Navigation)
                .count()
        );
        assert_eq!(headers(&folded).len(), 4, "the headings stay");
    }

    #[test]
    fn a_query_unfolds_every_group_that_has_a_match() {
        let mut state = reader();
        open(&mut state);
        state.overlay_search.buffer = "bookmark".into();

        let found = rows(&state);

        assert!(
            found.iter().all(|row| match row.kind {
                RowKind::Header(_) => true,
                RowKind::Binding => {
                    row.search.to_lowercase().contains("bookmark")
                }
            }),
            "every listed binding matches: {found:?}"
        );
        assert!(
            found
                .iter()
                .any(|row| row.kind == RowKind::Header(ShortcutCategory::Navigation)),
            "a folded group with a match opens: {found:?}"
        );
        assert!(
            !found
                .iter()
                .any(|row| row.kind == RowKind::Header(ShortcutCategory::View)),
            "a group with no match is left out: {found:?}"
        );
    }

    #[test]
    fn searching_matches_a_key_a_description_or_a_group_name() {
        let mut state = reader();
        open(&mut state);

        state.overlay_search.buffer = "Shift+G".into();
        let by_key = rows(&state);
        assert_eq!(bindings(&by_key), 1, "{by_key:?}");

        state.overlay_search.buffer = "navigation".into();
        let by_group = rows(&state);
        assert_eq!(
            bindings(&by_group),
            KEYBOARD_SHORTCUTS
                .into_iter()
                .filter(|shortcut| shortcut.category == ShortcutCategory::Navigation)
                .count()
        );

        state.overlay_search.buffer = "zzz".into();
        assert!(rows(&state).is_empty(), "nothing matches, nothing shows");
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
    fn a_binding_row_is_not_foldable() {
        let mut state = reader();
        let binding = rows(&state)
            .into_iter()
            .find(|row| row.kind == RowKind::Binding)
            .expect("a binding row");

        assert!(!toggle(&mut state, &binding), "a binding is not foldable");
        assert!(state.collapsed_shortcut_categories.is_empty());
    }

    #[test]
    fn a_binding_row_carries_what_confirming_it_runs() {
        let state = reader();
        let rows = rows(&state);

        let focus = rows
            .iter()
            .find(|row| row.key == "f")
            .expect("the focus-mode binding");
        assert_eq!(
            focus.action,
            Some(reader_core::ShortcutAction::ToggleFocusMode)
        );
        assert!(
            rows.iter()
                .filter(|row| matches!(row.kind, RowKind::Header(_)))
                .all(|row| row.action.is_none()),
            "a heading folds, it does not run"
        );
    }

    #[test]
    fn folding_everything_leaves_only_the_headings() {
        let mut state = reader();
        collapse_all(&mut state);
        let folded = rows(&state);
        assert_eq!(folded.len(), 4);
        assert_eq!(bindings(&folded), 0);

        expand_all(&mut state);
        assert_eq!(bindings(&rows(&state)), KEYBOARD_SHORTCUTS.len());
    }

    #[test]
    fn the_overlay_shows_the_panel_without_filtering_it_a_second_time() {
        let mut state = reader();
        let storage = reader_storage::Storage::open_in_memory().expect("database");
        state.overlay_search.buffer = "bookmark".into();

        let found = overlay::visible_entries(&state, &storage, 0);

        assert_eq!(
            found.len(),
            rows(&state).len(),
            "the panel already applied the query: {found:?}"
        );
        assert!(
            found
                .iter()
                .any(|entry| entry.style == crate::overlay::EntryStyle::Header),
            "headings survive the query: {found:?}"
        );
        assert!(
            found.iter().any(|entry| entry.detail == "Shift+B"),
            "the key is carried as its own column: {found:?}"
        );
    }
}

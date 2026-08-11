//! Reader state and the navigation rules over it.
//!
//! The state is plain data plus methods that keep it consistent: a chapter index
//! is always inside the book, a scroll offset is always inside the chapter, and
//! history only records places the reader actually visited.

use reader_core::render::{RenderOptions, render_blocks};
use reader_core::{AppSettings, CanonicalBook, LibrarySortKey, PaceState, SortDirection, Theme};

use crate::layout::{OverlayLayout, Viewport, ViewportLayout, compute_layout};

/// Longest navigation history kept, oldest dropped first.
pub const MAX_NAV_HISTORY: usize = 50;

/// Which overlay is on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    None,
    Chapters,
    Books,
    Bookmarks,
    Notes,
    ColorSchemes,
    Themes,
    Settings,
    Help,
    Keys,
    Diagnostics,
    FilePicker,
}

impl Overlay {
    /// How this overlay affects the reading layout.
    #[must_use]
    pub const fn layout(self) -> OverlayLayout {
        match self {
            Self::None => OverlayLayout::None,
            Self::Help => OverlayLayout::FullPage,
            Self::Settings | Self::Keys | Self::Diagnostics | Self::FilePicker => {
                OverlayLayout::Modal
            }
            Self::Chapters
            | Self::Books
            | Self::Bookmarks
            | Self::Notes
            | Self::ColorSchemes
            | Self::Themes => OverlayLayout::Side,
        }
    }

    /// Whether this overlay is a centred modal.
    #[must_use]
    pub const fn is_modal(self) -> bool {
        matches!(self.layout(), OverlayLayout::Modal)
    }
}

/// One search result: a block, and the line inside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchHit {
    pub chapter_index: usize,
    pub block_index: usize,
    pub line_index: usize,
}

/// An active search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchState {
    pub query: String,
    /// Whether the search covered the whole book.
    pub global: bool,
    pub results: Vec<SearchHit>,
    pub cursor: usize,
}

impl SearchState {
    /// The currently selected hit.
    #[must_use]
    pub fn current(&self) -> Option<SearchHit> {
        self.results.get(self.cursor).copied()
    }

    /// Move to the next hit, wrapping.
    pub fn advance(&mut self, forward: bool) -> Option<SearchHit> {
        if self.results.is_empty() {
            return None;
        }
        self.cursor = if forward {
            (self.cursor + 1) % self.results.len()
        } else {
            (self.cursor + self.results.len() - 1) % self.results.len()
        };
        self.current()
    }
}

/// A place the reader has been.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NavEntry {
    pub chapter_index: usize,
    pub block_offset: usize,
}

/// Everything the reader shows and remembers for this session.
pub struct ReaderState {
    pub settings: AppSettings,
    pub theme: Theme,
    pub viewport: Viewport,

    pub current_book: Option<CanonicalBook>,
    pub chapter_index: usize,
    pub block_offset: usize,

    pub focus_mode: bool,
    pub focus_block_index: usize,

    pub overlay: Overlay,
    pub overlay_cursor: usize,
    /// Incremental search inside the open overlay.
    pub overlay_search: crate::overlay::OverlaySearch,
    pub status: String,
    pub help_command: Option<String>,

    /// Shortcut categories the reader has folded away, for this session only.
    pub collapsed_shortcut_categories: std::collections::BTreeSet<reader_core::ShortcutCategory>,
    /// Which settings tab is open.
    pub settings_tab: reader_core::SettingsTab,
    /// Settings as they were when the panel opened, restored if it is cancelled.
    pub settings_backup: Option<AppSettings>,

    pub search: Option<SearchState>,
    pub nav_history: Vec<NavEntry>,
    /// Index into `nav_history`; `None` when nothing has been recorded.
    pub nav_history_cursor: Option<usize>,

    pub pace: PaceState,
    pub library_sort_key: LibrarySortKey,
    pub library_sort_direction: SortDirection,
    pub books_tag_filter: Option<String>,

    /// Library root currently being browsed.
    pub library_directory: std::path::PathBuf,
    pub discoveries: Vec<reader_formats::Discovery>,

    /// Lines an integration wants shown in the diagnostics overlay.
    pub integration_report: Vec<String>,
    /// Text a command wants typed into the command bar, so the reader can
    /// complete it — used when Toggl needs a workspace URL pasted.
    pub command_prefill: Option<String>,

    pub should_quit: bool,
    /// Cached per-chapter line counts, invalidated whenever rendering inputs change.
    layout_metrics: Option<LayoutMetrics>,
}

/// Rendered line counts per chapter, and what they were computed for.
#[derive(Debug, Clone, PartialEq)]
struct LayoutMetrics {
    book_id: String,
    settings: AppSettings,
    content_width: u16,
    body_height: u16,
    chapter_line_counts: Vec<usize>,
    /// Screens of content per chapter, used for whole-book progress.
    chapter_view_counts: Vec<usize>,
}

impl ReaderState {
    /// A reader with no book open.
    #[must_use]
    pub fn new(settings: AppSettings) -> Self {
        Self {
            theme: settings.theme(),
            settings,
            viewport: Viewport::default(),
            current_book: None,
            chapter_index: 0,
            block_offset: 0,
            focus_mode: false,
            focus_block_index: 0,
            overlay: Overlay::None,
            overlay_cursor: 0,
            overlay_search: crate::overlay::OverlaySearch::default(),
            status: String::new(),
            help_command: None,
            collapsed_shortcut_categories: std::collections::BTreeSet::new(),
            settings_tab: reader_core::SettingsTab::Themes,
            settings_backup: None,
            search: None,
            nav_history: Vec::new(),
            nav_history_cursor: None,
            pace: PaceState::default(),
            library_sort_key: LibrarySortKey::LastOpened,
            library_sort_direction: SortDirection::Descending,
            books_tag_filter: None,
            library_directory: std::env::current_dir().unwrap_or_default(),
            discoveries: Vec::new(),
            integration_report: Vec::new(),
            command_prefill: None,
            should_quit: false,
            layout_metrics: None,
        }
    }

    /// Re-resolve the theme after a scheme or appearance change.
    pub fn refresh_theme(&mut self) {
        self.theme = self.settings.theme();
    }

    /// Open an overlay with its cursor at `cursor` and no query.
    ///
    /// A stale query would hide most of a freshly opened list, so opening always
    /// starts from the whole thing.
    pub fn open_overlay(&mut self, overlay: Overlay, cursor: usize) {
        self.overlay = overlay;
        self.overlay_cursor = cursor;
        self.overlay_search.reset();
    }

    /// Drop cached line counts; call after anything that changes rendering.
    pub fn invalidate_layout(&mut self) {
        self.layout_metrics = None;
    }

    /// The book's id, when one is open.
    #[must_use]
    pub fn book_id(&self) -> Option<&str> {
        self.current_book.as_ref().map(|book| book.id.as_str())
    }

    /// The chapter being read.
    #[must_use]
    pub fn chapter(&self) -> Option<&reader_core::CanonicalChapter> {
        self.current_book
            .as_ref()
            .and_then(|book| book.chapters.get(self.chapter_index))
    }

    /// Number of chapters in the open book.
    #[must_use]
    pub fn chapter_count(&self) -> usize {
        self.current_book
            .as_ref()
            .map_or(0, |book| book.chapters.len())
    }

    /// The current frame geometry.
    #[must_use]
    pub fn layout(&self, footer_height: u16) -> ViewportLayout {
        compute_layout(
            self.viewport,
            &self.settings,
            self.overlay.layout(),
            self.current_book.is_some(),
            footer_height,
        )
    }

    /// Render options for the reading column.
    #[must_use]
    pub fn render_options(&self, content_width: u16) -> RenderOptions<'_> {
        RenderOptions {
            mode: self.settings.render_mode,
            width: content_width as usize,
            palette: &self.theme.palette,
            code_language: self.settings.code_language,
            code_density: self.settings.code_density,
            plain_highlight: self.settings.plain_highlight,
            line_spacing: self.settings.line_spacing,
            block_index_offset: 0,
            include_trailing_spacing: true,
            search_query: self
                .search
                .as_ref()
                .map(|search| search.query.as_str())
                .filter(|query| !query.is_empty()),
        }
    }

    /// Per-chapter line counts, computed once per rendering configuration.
    fn metrics(&mut self, content_width: u16, body_height: u16) -> Option<&LayoutMetrics> {
        let book = self.current_book.as_ref()?;
        let stale = match &self.layout_metrics {
            Some(metrics) => {
                metrics.book_id != book.id
                    || metrics.settings != self.settings
                    || metrics.content_width != content_width
                    || metrics.body_height != body_height
            }
            None => true,
        };
        if stale {
            // Search highlighting never changes line counts, so it is left out
            // of the options here on purpose.
            let options = RenderOptions {
                search_query: None,
                ..self.render_options(content_width)
            };
            let counts: Vec<usize> = book
                .chapters
                .iter()
                .map(|chapter| render_blocks(&chapter.blocks, &options).len())
                .collect();
            let views = counts
                .iter()
                .map(|lines| {
                    let height = body_height.max(1) as usize;
                    lines.saturating_sub(height) + 1
                })
                .collect();
            self.layout_metrics = Some(LayoutMetrics {
                book_id: book.id.clone(),
                settings: self.settings,
                content_width,
                body_height,
                chapter_line_counts: counts,
                chapter_view_counts: views,
            });
        }
        self.layout_metrics.as_ref()
    }

    /// Rendered lines in the current chapter.
    pub fn chapter_line_count(&mut self, content_width: u16, body_height: u16) -> usize {
        let chapter_index = self.chapter_index;
        self.metrics(content_width, body_height)
            .and_then(|metrics| metrics.chapter_line_counts.get(chapter_index).copied())
            .unwrap_or(0)
    }

    /// Largest scroll offset that still shows content.
    pub fn chapter_max_offset(&mut self, content_width: u16, body_height: u16) -> usize {
        self.chapter_line_count(content_width, body_height)
            .saturating_sub(body_height as usize)
    }

    /// Progress through the current chapter, 0.0 to 1.0.
    pub fn chapter_progress(&mut self, content_width: u16, body_height: u16) -> f64 {
        let max_offset = self.chapter_max_offset(content_width, body_height);
        if max_offset == 0 {
            return 0.0;
        }
        (self.block_offset as f64 / max_offset as f64).clamp(0.0, 1.0)
    }

    /// Progress through the whole book, 0.0 to 1.0.
    pub fn book_progress(&mut self, content_width: u16, body_height: u16) -> f64 {
        let chapter_index = self.chapter_index;
        let block_offset = self.block_offset;
        let max_offset = self.chapter_max_offset(content_width, body_height);
        let Some(metrics) = self.metrics(content_width, body_height) else {
            return 0.0;
        };
        let total: usize = metrics.chapter_view_counts.iter().sum();
        if total <= 1 {
            return 0.0;
        }
        let previous: usize = metrics.chapter_view_counts[..chapter_index].iter().sum();
        let offset = block_offset.min(max_offset);
        ((previous + offset) as f64 / (total - 1) as f64).clamp(0.0, 1.0)
    }

    /// Clamp the scroll offset into the current chapter.
    pub fn clamp_offset(&mut self, content_width: u16, body_height: u16) {
        let max_offset = self.chapter_max_offset(content_width, body_height);
        self.block_offset = self.block_offset.min(max_offset);
    }

    /// Record the current place in the navigation history.
    ///
    /// Re-recording the same place is a no-op, and moving after stepping back
    /// discards the forward entries, matching how a browser history behaves.
    pub fn push_nav_history(&mut self) {
        if self.current_book.is_none() {
            return;
        }
        if let Some(cursor) = self.nav_history_cursor {
            self.nav_history.truncate(cursor + 1);
        } else {
            self.nav_history.clear();
        }
        let entry = NavEntry {
            chapter_index: self.chapter_index,
            block_offset: self.block_offset,
        };
        if self.nav_history.last() == Some(&entry) {
            self.nav_history_cursor = Some(self.nav_history.len() - 1);
            return;
        }
        self.nav_history.push(entry);
        if self.nav_history.len() > MAX_NAV_HISTORY {
            self.nav_history.remove(0);
        }
        self.nav_history_cursor = Some(self.nav_history.len() - 1);
    }

    /// Step back through the history, if there is anywhere to go.
    pub fn history_back(&mut self) -> bool {
        let Some(cursor) = self.nav_history_cursor.filter(|cursor| *cursor > 0) else {
            return false;
        };
        self.nav_history_cursor = Some(cursor - 1);
        self.apply_history_entry(cursor - 1)
    }

    /// Step forward through the history, if there is anywhere to go.
    pub fn history_forward(&mut self) -> bool {
        let Some(cursor) = self.nav_history_cursor else {
            return false;
        };
        if cursor + 1 >= self.nav_history.len() {
            return false;
        }
        self.nav_history_cursor = Some(cursor + 1);
        self.apply_history_entry(cursor + 1)
    }

    fn apply_history_entry(&mut self, index: usize) -> bool {
        let Some(entry) = self.nav_history.get(index).copied() else {
            return false;
        };
        self.chapter_index = entry
            .chapter_index
            .min(self.chapter_count().saturating_sub(1));
        self.block_offset = entry.block_offset;
        true
    }

    /// Blocks in the current chapter.
    #[must_use]
    pub fn chapter_block_count(&self) -> usize {
        self.chapter().map_or(0, |chapter| chapter.blocks.len())
    }

    /// Rendered line counts of each block, for focus-mode mapping.
    fn focus_line_counts(&self, content_width: u16) -> Vec<usize> {
        let Some(chapter) = self.chapter() else {
            return Vec::new();
        };
        let options = RenderOptions {
            search_query: None,
            ..self.render_options(content_width)
        };
        chapter
            .blocks
            .iter()
            .enumerate()
            .map(|(index, block)| {
                let options = options.clone().with_block_offset(index);
                render_blocks(std::slice::from_ref(block), &options).len()
            })
            .collect()
    }

    /// Keep the focused block inside the chapter.
    #[must_use]
    pub fn clamp_focus_index(&self, index: usize) -> usize {
        match self.chapter_block_count() {
            0 => 0,
            count => index.min(count - 1),
        }
    }

    /// The scroll offset that puts `focus_block_index` at the top.
    #[must_use]
    pub fn focus_index_to_offset(&self, content_width: u16, focus_block_index: usize) -> usize {
        let counts = self.focus_line_counts(content_width);
        if counts.is_empty() {
            return 0;
        }
        let index = focus_block_index.min(counts.len() - 1);
        counts[..index].iter().sum()
    }

    /// The block containing the line at `block_offset`.
    #[must_use]
    pub fn offset_to_focus_index(&self, content_width: u16, block_offset: usize) -> usize {
        let counts = self.focus_line_counts(content_width);
        if counts.is_empty() {
            return 0;
        }
        let mut cursor = 0usize;
        for (index, count) in counts.iter().enumerate() {
            cursor += count;
            if block_offset < cursor {
                return index;
            }
        }
        counts.len() - 1
    }
}

#[cfg(test)]
mod tests {
    use reader_core::{AppSettings, CanonicalBlock, CanonicalBook, CanonicalChapter};

    use super::{NavEntry, Overlay, ReaderState, SearchHit, SearchState};

    fn book(chapters: usize, blocks_per_chapter: usize) -> CanonicalBook {
        CanonicalBook {
            id: "book".into(),
            title: "Book".into(),
            author: "Author".into(),
            source_path: "/books/book.epub".into(),
            import_hash: "hash".into(),
            parser_version: Some(3),
            diagnostics: Vec::new(),
            chapters: (0..chapters)
                .map(|chapter_index| CanonicalChapter {
                    id: format!("ch{chapter_index}"),
                    index: chapter_index,
                    title: format!("Chapter {}", chapter_index + 1),
                    href: format!("ch{chapter_index}.xhtml"),
                    depth: 0,
                    blocks: (0..blocks_per_chapter)
                        .map(|block_index| CanonicalBlock::Paragraph {
                            id: format!("b{chapter_index}-{block_index}"),
                            text: "the quiet harbour at dawn and the long tide after it".into(),
                        })
                        .collect(),
                    word_count: blocks_per_chapter * 11,
                })
                .collect(),
            cover_path: None,
        }
    }

    fn state_with_book() -> ReaderState {
        let mut state = ReaderState::new(AppSettings::default());
        state.current_book = Some(book(3, 4));
        state
    }

    #[test]
    fn history_records_visited_places_and_ignores_repeats() {
        let mut state = state_with_book();
        state.push_nav_history();
        state.push_nav_history();
        assert_eq!(state.nav_history.len(), 1);

        state.chapter_index = 1;
        state.push_nav_history();
        assert_eq!(
            state.nav_history,
            vec![
                NavEntry {
                    chapter_index: 0,
                    block_offset: 0
                },
                NavEntry {
                    chapter_index: 1,
                    block_offset: 0
                }
            ]
        );
    }

    #[test]
    fn history_is_bounded_and_keeps_the_most_recent_entries() {
        let mut state = state_with_book();
        for offset in 0..60 {
            state.block_offset = offset;
            state.push_nav_history();
        }
        assert_eq!(state.nav_history.len(), super::MAX_NAV_HISTORY);
        assert_eq!(
            state.nav_history.last(),
            Some(&NavEntry {
                chapter_index: 0,
                block_offset: 59
            })
        );
    }

    #[test]
    fn history_without_a_book_records_nothing() {
        let mut state = ReaderState::new(AppSettings::default());
        state.push_nav_history();
        assert!(state.nav_history.is_empty());
        assert!(!state.history_back());
        assert!(!state.history_forward());
    }

    #[test]
    fn stepping_back_and_forward_walks_the_recorded_places() {
        let mut state = state_with_book();
        state.push_nav_history();
        state.chapter_index = 1;
        state.block_offset = 5;
        state.push_nav_history();
        state.chapter_index = 2;
        state.push_nav_history();

        assert!(state.history_back());
        assert_eq!(state.chapter_index, 1);
        assert_eq!(state.block_offset, 5);
        assert!(state.history_back());
        assert_eq!(state.chapter_index, 0);
        assert!(!state.history_back(), "the start of history is a dead end");

        assert!(state.history_forward());
        assert_eq!(state.chapter_index, 1);
        assert!(state.history_forward());
        assert_eq!(state.chapter_index, 2);
        assert!(!state.history_forward(), "the end of history is a dead end");
    }

    #[test]
    fn moving_after_stepping_back_discards_the_forward_entries() {
        let mut state = state_with_book();
        state.push_nav_history();
        state.chapter_index = 1;
        state.push_nav_history();
        state.chapter_index = 2;
        state.push_nav_history();

        state.history_back();
        state.chapter_index = 1;
        state.block_offset = 9;
        state.push_nav_history();

        assert_eq!(state.nav_history.len(), 3);
        assert_eq!(
            state.nav_history.last(),
            Some(&NavEntry {
                chapter_index: 1,
                block_offset: 9
            })
        );
        assert!(!state.history_forward());
    }

    #[test]
    fn scroll_bounds_come_from_the_rendered_chapter() {
        let mut state = state_with_book();
        let lines = state.chapter_line_count(40, 10);
        assert!(lines > 10, "the fixture chapter should be scrollable");
        assert_eq!(state.chapter_max_offset(40, 10), lines - 10);

        state.block_offset = 10_000;
        state.clamp_offset(40, 10);
        assert_eq!(state.block_offset, lines - 10);
    }

    #[test]
    fn a_chapter_shorter_than_the_screen_cannot_scroll() {
        let mut state = ReaderState::new(AppSettings::default());
        state.current_book = Some(book(1, 1));
        assert_eq!(state.chapter_max_offset(80, 40), 0);
        assert!((state.chapter_progress(80, 40) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn progress_grows_with_the_offset_and_the_chapter() {
        let mut state = state_with_book();
        let start = state.book_progress(40, 10);
        state.block_offset = state.chapter_max_offset(40, 10);
        let scrolled = state.book_progress(40, 10);
        state.chapter_index = 2;
        state.block_offset = state.chapter_max_offset(40, 10);
        let end = state.book_progress(40, 10);

        assert!((start - 0.0).abs() < f64::EPSILON);
        assert!(scrolled > start);
        assert!(end > scrolled);
        assert!(end <= 1.0);
        assert!((state.chapter_progress(40, 10) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn layout_metrics_are_recomputed_when_rendering_inputs_change() {
        let mut state = state_with_book();
        let narrow = state.chapter_line_count(30, 10);
        let wide = state.chapter_line_count(100, 10);
        assert!(narrow > wide, "a wider column needs fewer lines");

        state.settings.line_spacing = reader_core::LineSpacing::Relaxed;
        let relaxed = state.chapter_line_count(100, 10);
        assert!(relaxed > wide, "relaxed spacing adds lines");
    }

    #[test]
    fn focus_mapping_round_trips_between_blocks_and_offsets() {
        let state = state_with_book();
        for index in 0..state.chapter_block_count() {
            let offset = state.focus_index_to_offset(40, index);
            assert_eq!(state.offset_to_focus_index(40, offset), index);
        }
        assert_eq!(state.clamp_focus_index(99), state.chapter_block_count() - 1);
        assert_eq!(
            state.focus_index_to_offset(40, 99),
            state.focus_index_to_offset(40, state.chapter_block_count() - 1)
        );
    }

    #[test]
    fn focus_mapping_is_empty_without_a_chapter() {
        let state = ReaderState::new(AppSettings::default());
        assert_eq!(state.clamp_focus_index(3), 0);
        assert_eq!(state.focus_index_to_offset(40, 3), 0);
        assert_eq!(state.offset_to_focus_index(40, 3), 0);
    }

    #[test]
    fn search_results_cycle_in_both_directions() {
        let mut search = SearchState {
            query: "harbour".into(),
            global: true,
            results: vec![
                SearchHit {
                    chapter_index: 0,
                    block_index: 0,
                    line_index: 0,
                },
                SearchHit {
                    chapter_index: 1,
                    block_index: 2,
                    line_index: 0,
                },
            ],
            cursor: 0,
        };
        assert_eq!(search.advance(true).map(|hit| hit.chapter_index), Some(1));
        assert_eq!(search.advance(true).map(|hit| hit.chapter_index), Some(0));
        assert_eq!(search.advance(false).map(|hit| hit.chapter_index), Some(1));

        search.results.clear();
        assert!(search.advance(true).is_none());
    }

    #[test]
    fn overlays_declare_how_they_affect_the_layout() {
        assert!(Overlay::Settings.is_modal());
        assert!(Overlay::FilePicker.is_modal());
        assert!(!Overlay::Chapters.is_modal());
        assert!(!Overlay::Help.is_modal());
        assert_eq!(Overlay::None.layout(), crate::layout::OverlayLayout::None);
    }
}

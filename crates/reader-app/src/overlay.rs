//! What an overlay lists, and how a query narrows it.
//!
//! Every overlay is the same shape: a list of entries, a cursor, and an optional
//! query. Building that list in one place means the frame draws exactly what the
//! cursor indexes and what confirming acts on — a filtered list cannot select the
//! wrong item, because there is only one list.

use reader_core::{Bookmark, LibraryEntryWithProgress, Note, fuzzy, locale::format_relative_time};
use reader_storage::Storage;

use crate::state::{Overlay, ReaderState};

/// One row of an overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayEntry {
    /// The text shown in the list.
    pub display: String,
    /// A second column, right-aligned against the row's far edge: a key binding,
    /// a progress tag, a location. Empty when the row is a single column.
    pub detail: String,
    /// The text a query is matched against, which may include fields the row
    /// does not show — an author, a tag, a location.
    pub search: String,
    /// What acting on this row refers to: an index into the source list, or an
    /// identifier for rows that have one.
    pub target: EntryTarget,
    /// How the row should read on screen.
    pub style: EntryStyle,
}

/// How much weight a row carries in the list.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EntryStyle {
    /// An ordinary, selectable row.
    #[default]
    Normal,
    /// A group heading: emphasised, and folded rather than opened.
    Header,
    /// On screen but not yet the reader's — a file found on disk, an
    /// informational line.
    Muted,
}

/// What confirming or deleting a row acts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryTarget {
    /// Position in the underlying list.
    Index(usize),
    /// A stored record, addressed by id.
    Id(String),
    /// Nothing: an informational row.
    None,
}

impl OverlayEntry {
    fn new(display: impl Into<String>, search: impl Into<String>, target: EntryTarget) -> Self {
        Self {
            display: display.into(),
            detail: String::new(),
            search: search.into(),
            target,
            style: EntryStyle::Normal,
        }
    }

    /// A row whose search text is its display text.
    fn plain(display: impl Into<String>, target: EntryTarget) -> Self {
        let display = display.into();
        Self::new(display.clone(), display, target)
    }

    /// Give the row a right-aligned second column.
    #[must_use]
    fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = detail.into();
        self
    }

    /// Give the row a weight other than [`EntryStyle::Normal`].
    #[must_use]
    fn with_style(mut self, style: EntryStyle) -> Self {
        self.style = style;
        self
    }

    /// The index this row refers to, when it refers to one.
    #[must_use]
    pub fn index(&self) -> Option<usize> {
        match &self.target {
            EntryTarget::Index(index) => Some(*index),
            _ => None,
        }
    }

    /// The id this row refers to, when it refers to one.
    #[must_use]
    pub fn id(&self) -> Option<&str> {
        match &self.target {
            EntryTarget::Id(id) => Some(id),
            _ => None,
        }
    }
}

/// The overlay's incremental search.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OverlaySearch {
    pub buffer: String,
    /// Whether typing goes to the query rather than to navigation.
    pub active: bool,
}

impl OverlaySearch {
    /// Clear the query, as opening an overlay does.
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.active = false;
    }

    #[must_use]
    pub fn query(&self) -> &str {
        self.buffer.trim()
    }
}

fn bookmark_location(bookmark: &Bookmark) -> String {
    format!(
        "Ch.{} §{}",
        bookmark.chapter_index + 1,
        bookmark.block_offset
    )
}

fn note_location(note: &Note) -> String {
    match note.chapter_index {
        Some(chapter_index) => format!(
            "Ch.{} §{}",
            chapter_index + 1,
            note.block_offset.unwrap_or(0)
        ),
        None => "Book".to_owned(),
    }
}

fn library_row(entry: &LibraryEntryWithProgress, tags: &[String], now: i64) -> OverlayEntry {
    let progress = match (entry.book_progress, entry.chapter_index) {
        (Some(progress), Some(chapter_index)) => {
            format!("[Ch.{} · {:.0}%]", chapter_index + 1, progress * 100.0)
        }
        _ => "[not started]".to_owned(),
    };
    let tag_text = if tags.is_empty() {
        String::new()
    } else {
        format!(
            " {}",
            tags.iter()
                .map(|tag| format!("#{tag}"))
                .collect::<Vec<_>>()
                .join(" ")
        )
    };
    let display = format!(
        "{}  —  {}  {progress}{tag_text}  {}",
        entry.entry.title,
        entry.entry.author,
        format_relative_time(entry.entry.last_opened_at, now)
    );
    // Author and tags are searchable even though the row leads with the title.
    let search = format!(
        "{} {} {}",
        entry.entry.title,
        entry.entry.author,
        tags.join(" ")
    );
    OverlayEntry::new(display, search, EntryTarget::Id(entry.entry.id.clone()))
}

/// Build the unfiltered rows of the open overlay.
///
/// `now` is used for relative timestamps in the library list.
pub fn entries(state: &ReaderState, storage: &Storage, now: i64) -> Vec<OverlayEntry> {
    match state.overlay {
        Overlay::None => Vec::new(),

        Overlay::Chapters => state
            .current_book
            .as_ref()
            .map(|book| {
                book.chapters
                    .iter()
                    .enumerate()
                    .map(|(index, chapter)| {
                        OverlayEntry::new(
                            format!("{}{}", "  ".repeat(chapter.depth), chapter.title),
                            chapter.title.clone(),
                            EntryTarget::Index(index),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default(),

        Overlay::Books => {
            let tags = storage.tags_by_book().unwrap_or_default();
            storage
                .books_with_progress(
                    state.library_sort_key,
                    state.library_sort_direction,
                    state.books_tag_filter.as_deref(),
                )
                .unwrap_or_default()
                .iter()
                .map(|entry| {
                    let book_tags = tags.get(&entry.entry.id).cloned().unwrap_or_default();
                    library_row(entry, &book_tags, now)
                })
                .collect()
        }

        Overlay::Bookmarks => state
            .book_id()
            .and_then(|book_id| storage.bookmarks(book_id).ok())
            .unwrap_or_default()
            .into_iter()
            .map(|bookmark| {
                let location = bookmark_location(&bookmark);
                let label = bookmark.label.clone().unwrap_or_else(|| location.clone());
                OverlayEntry::new(
                    format!("{label}  ·  {location}"),
                    format!("{label} {location}"),
                    EntryTarget::Id(bookmark.id),
                )
            })
            .collect(),

        Overlay::Notes => state
            .book_id()
            .and_then(|book_id| storage.notes(book_id).ok())
            .unwrap_or_default()
            .into_iter()
            .map(|note| {
                let location = note_location(&note);
                OverlayEntry::new(
                    format!("{}  ·  {location}", note.content),
                    format!("{} {location}", note.content),
                    EntryTarget::Id(note.id),
                )
            })
            .collect(),

        Overlay::ColorSchemes => reader_core::ColorSchemeId::ALL
            .iter()
            .enumerate()
            .map(|(index, scheme)| OverlayEntry::plain(scheme.label(), EntryTarget::Index(index)))
            .collect(),

        Overlay::Themes => reader_core::theme::AppearanceThemeId::ALL
            .iter()
            .enumerate()
            .map(|(index, theme)| OverlayEntry::plain(theme.label(), EntryTarget::Index(index)))
            .collect(),

        // A checkbox rather than a highlight: the cursor says where you are, the
        // box says what will be imported, and those are different questions.
        Overlay::FilePicker => state
            .discoveries
            .iter()
            .enumerate()
            .map(|(index, discovery)| {
                let ticked = state.picker_selected.contains(&index);
                OverlayEntry::new(
                    format!(
                        "{} {}",
                        if ticked { "[x]" } else { "[ ]" },
                        discovery.file_name
                    ),
                    discovery.file_name.clone(),
                    EntryTarget::Index(index),
                )
                .with_style(if ticked {
                    EntryStyle::Normal
                } else {
                    EntryStyle::Muted
                })
            })
            .collect(),

        Overlay::Diagnostics if !state.integration_report.is_empty() => state
            .integration_report
            .iter()
            .map(|line| OverlayEntry::plain(line.clone(), EntryTarget::None))
            .collect(),

        Overlay::Diagnostics => state
            .current_book
            .as_ref()
            .map(|book| {
                book.diagnostics
                    .iter()
                    .map(|diagnostic| {
                        OverlayEntry::plain(diagnostic.message.clone(), EntryTarget::None)
                    })
                    .collect()
            })
            .unwrap_or_default(),

        Overlay::Settings => crate::settings_panel::rows(state)
            .into_iter()
            .enumerate()
            .map(|(index, row)| {
                OverlayEntry::new(row.label, row.search, EntryTarget::Index(index))
                    .with_detail(row.value)
            })
            .collect(),

        Overlay::Keys => crate::shortcuts_panel::rows(state)
            .into_iter()
            .enumerate()
            .map(|(index, row)| {
                let style = match row.kind {
                    crate::shortcuts_panel::RowKind::Header(_) => EntryStyle::Header,
                    crate::shortcuts_panel::RowKind::Binding => EntryStyle::Normal,
                };
                OverlayEntry::new(row.label, row.search, EntryTarget::Index(index))
                    .with_detail(row.key)
                    .with_style(style)
            })
            .collect(),

        Overlay::Help => reader_core::command::command_help(state.help_command.as_deref(), None)
            .into_iter()
            .map(|line| OverlayEntry::plain(line, EntryTarget::None))
            .collect(),
    }
}

/// Narrow rows by the overlay's query, ranked the way the command bar ranks.
///
/// Whitespace is trimmed first, so a half-typed query with a trailing space does
/// not empty the list.
#[must_use]
pub fn filter(rows: Vec<OverlayEntry>, query: &str) -> Vec<OverlayEntry> {
    fuzzy::filter(query.trim(), rows, |entry| entry.search.as_str())
}

/// Overlays that apply the query while building their rows.
///
/// Both keep matches under their own headings or tabs, which a rank-ordered
/// generic filter would scramble, so filtering them twice would be wrong.
const fn filters_itself(overlay: Overlay) -> bool {
    matches!(overlay, Overlay::Keys | Overlay::Settings)
}

/// The rows the overlay should show right now.
#[must_use]
pub fn visible_entries(state: &ReaderState, storage: &Storage, now: i64) -> Vec<OverlayEntry> {
    let rows = entries(state, storage, now);
    if filters_itself(state.overlay) {
        return rows;
    }
    filter(rows, state.overlay_search.query())
}

#[cfg(test)]
mod tests {
    use reader_core::{AppSettings, CanonicalBlock, CanonicalBook, CanonicalChapter, RenderMode};
    use reader_storage::Storage;

    use super::{EntryTarget, OverlaySearch, entries, filter, visible_entries};
    use crate::state::{Overlay, ReaderState};

    const NOW: i64 = 1_700_000_000_000;

    fn book() -> CanonicalBook {
        CanonicalBook {
            id: "book".into(),
            title: "Quiet Harbour".into(),
            author: "Watts".into(),
            source_path: "/books/quiet.epub".into(),
            import_hash: "hash".into(),
            parser_version: Some(3),
            diagnostics: Vec::new(),
            chapters: ["Opening", "The Long Tide", "Ending"]
                .iter()
                .enumerate()
                .map(|(index, title)| CanonicalChapter {
                    id: format!("ch{index}"),
                    index,
                    title: (*title).to_owned(),
                    href: format!("ch{index}.xhtml"),
                    depth: usize::from(index == 1),
                    blocks: vec![CanonicalBlock::Paragraph {
                        id: format!("b{index}"),
                        text: "text".into(),
                    }],
                    word_count: 1,
                })
                .collect(),
            cover_path: None,
        }
    }

    fn reader() -> (ReaderState, Storage) {
        let mut storage = Storage::open_in_memory().expect("database");
        let book = book();
        storage
            .save_book(&book, RenderMode::Plain, NOW)
            .expect("save");
        let mut state = ReaderState::new(AppSettings::default());
        state.current_book = Some(book);
        (state, storage)
    }

    fn displays(rows: &[super::OverlayEntry]) -> Vec<&str> {
        rows.iter().map(|row| row.display.as_str()).collect()
    }

    #[test]
    fn chapters_are_listed_with_their_nesting_and_keep_their_index() {
        let (mut state, storage) = reader();
        state.overlay = Overlay::Chapters;

        let rows = entries(&state, &storage, NOW);

        assert_eq!(
            displays(&rows),
            vec!["Opening", "  The Long Tide", "Ending"],
            "depth shows as indentation"
        );
        assert_eq!(rows[2].target, EntryTarget::Index(2));
    }

    #[test]
    fn a_query_narrows_the_list_and_the_rows_keep_pointing_at_the_right_chapter() {
        let (mut state, storage) = reader();
        state.overlay = Overlay::Chapters;
        state.overlay_search.buffer = "tide".into();

        let rows = visible_entries(&state, &storage, NOW);

        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].target,
            EntryTarget::Index(1),
            "the filtered row still refers to chapter 2"
        );
    }

    #[test]
    fn the_library_lists_books_with_progress_tags_and_a_relative_time() {
        let (mut state, storage) = reader();
        storage.add_tag("book", "favorite").expect("tag");
        storage
            .save_position(
                "book",
                reader_core::ReadingPosition {
                    chapter_index: 1,
                    chapter_progress: 0.5,
                    book_progress: 0.42,
                    block_offset: 3,
                },
                NOW,
            )
            .expect("position");
        state.overlay = Overlay::Books;

        let rows = entries(&state, &storage, NOW);

        assert_eq!(rows.len(), 1);
        assert!(rows[0].display.contains("Quiet Harbour"), "{:?}", rows[0]);
        assert!(rows[0].display.contains("[Ch.2 · 42%]"), "{:?}", rows[0]);
        assert!(rows[0].display.contains("#favorite"), "{:?}", rows[0]);
        assert!(rows[0].display.contains("agora"), "{:?}", rows[0]);
        assert_eq!(rows[0].target, EntryTarget::Id("book".into()));
    }

    #[test]
    fn a_book_can_be_found_by_its_author_or_tag_without_showing_them_first() {
        let (mut state, storage) = reader();
        storage.add_tag("book", "sci-fi").expect("tag");
        state.overlay = Overlay::Books;

        for query in ["watts", "sci-fi", "harbour"] {
            state.overlay_search.buffer = query.to_owned();
            assert_eq!(
                visible_entries(&state, &storage, NOW).len(),
                1,
                "searching {query:?} should find the book"
            );
        }

        state.overlay_search.buffer = "zzz".into();
        assert!(visible_entries(&state, &storage, NOW).is_empty());
    }

    #[test]
    fn an_unstarted_book_says_so_rather_than_showing_a_percentage() {
        let (mut state, storage) = reader();
        state.overlay = Overlay::Books;
        let rows = entries(&state, &storage, NOW);
        assert!(rows[0].display.contains("[not started]"), "{:?}", rows[0]);
    }

    #[test]
    fn bookmarks_and_notes_list_their_location_and_are_addressed_by_id() {
        let (mut state, storage) = reader();
        let bookmark = storage
            .add_bookmark("book", 1, 7, Some("halfway"), NOW)
            .expect("bookmark");
        let note = storage
            .add_note("book", "remember this", Some(0), Some(2), NOW)
            .expect("note");

        state.overlay = Overlay::Bookmarks;
        let marks = entries(&state, &storage, NOW);
        assert_eq!(marks[0].display, "halfway  ·  Ch.2 §7");
        assert_eq!(marks[0].target, EntryTarget::Id(bookmark.id));

        state.overlay = Overlay::Notes;
        let notes = entries(&state, &storage, NOW);
        assert_eq!(notes[0].display, "remember this  ·  Ch.1 §2");
        assert_eq!(notes[0].target, EntryTarget::Id(note.id));
    }

    #[test]
    fn an_unlabelled_bookmark_shows_its_location_as_its_label() {
        let (mut state, storage) = reader();
        storage
            .add_bookmark("book", 0, 4, None, NOW)
            .expect("bookmark");
        state.overlay = Overlay::Bookmarks;

        let rows = entries(&state, &storage, NOW);
        assert_eq!(rows[0].display, "Ch.1 §4  ·  Ch.1 §4");
    }

    #[test]
    fn a_note_on_the_whole_book_is_located_as_the_book() {
        let (mut state, storage) = reader();
        storage
            .add_note("book", "about the whole thing", None, None, NOW)
            .expect("note");
        state.overlay = Overlay::Notes;

        let rows = entries(&state, &storage, NOW);
        assert!(rows[0].display.ends_with("·  Book"), "{:?}", rows[0]);
    }

    #[test]
    fn searching_bookmarks_matches_the_label_or_the_location() {
        let (mut state, storage) = reader();
        storage
            .add_bookmark("book", 4, 0, Some("the reveal"), NOW)
            .expect("bookmark");
        storage
            .add_bookmark("book", 1, 0, Some("opening"), NOW + 1)
            .expect("bookmark");
        state.overlay = Overlay::Bookmarks;

        state.overlay_search.buffer = "reveal".into();
        assert_eq!(visible_entries(&state, &storage, NOW).len(), 1);

        state.overlay_search.buffer = "Ch.5".into();
        let by_location = visible_entries(&state, &storage, NOW);
        assert_eq!(by_location.len(), 1);
        assert!(by_location[0].display.contains("the reveal"));
    }

    #[test]
    fn overlays_without_a_book_list_nothing_rather_than_failing() {
        let mut state = ReaderState::new(AppSettings::default());
        let storage = Storage::open_in_memory().expect("database");
        for overlay in [Overlay::Chapters, Overlay::Bookmarks, Overlay::Notes] {
            state.overlay = overlay;
            assert!(entries(&state, &storage, NOW).is_empty(), "{overlay:?}");
        }
    }

    #[test]
    fn informational_overlays_have_no_target_to_act_on() {
        let (mut state, storage) = reader();
        state.overlay = Overlay::Help;
        let rows = entries(&state, &storage, NOW);
        assert!(!rows.is_empty());
        assert!(rows.iter().all(|row| row.target == EntryTarget::None));
    }

    #[test]
    fn the_diagnostics_overlay_prefers_an_integration_report() {
        let (mut state, storage) = reader();
        state.overlay = Overlay::Diagnostics;
        assert!(
            entries(&state, &storage, NOW).is_empty(),
            "no diagnostics yet"
        );

        state.integration_report = vec!["Organization 123".into()];
        let rows = entries(&state, &storage, NOW);
        assert_eq!(displays(&rows), vec!["Organization 123"]);
    }

    #[test]
    fn an_empty_query_leaves_the_list_alone() {
        let (mut state, storage) = reader();
        state.overlay = Overlay::Chapters;
        let all = entries(&state, &storage, NOW);
        assert_eq!(filter(all.clone(), ""), all);
        assert_eq!(filter(all.clone(), "   "), all);
    }

    #[test]
    fn a_search_resets_when_it_is_dismissed() {
        let mut search = OverlaySearch {
            buffer: "tide".into(),
            active: true,
        };
        search.reset();
        assert!(search.buffer.is_empty());
        assert!(!search.active);
        assert_eq!(search.query(), "");
    }
}

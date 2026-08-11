//! Library records: what the reader remembers about books between sessions.
//!
//! These types mirror the stored rows one-for-one so the storage layer stays a
//! thin mapping and the domain rules (ordering, clamping, identity) live here.

use crate::settings::RenderMode;

/// Where the reader was in a book.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReadingPosition {
    pub chapter_index: usize,
    /// Progress through the current chapter, 0.0 to 1.0.
    pub chapter_progress: f64,
    /// Progress through the whole book, 0.0 to 1.0.
    pub book_progress: f64,
    /// Scroll offset in rendered lines within the chapter.
    pub block_offset: usize,
}

impl ReadingPosition {
    /// A position at the very start of a book.
    #[must_use]
    pub const fn start() -> Self {
        Self {
            chapter_index: 0,
            chapter_progress: 0.0,
            book_progress: 0.0,
            block_offset: 0,
        }
    }
}

/// A saved place in a book, optionally labelled.
#[derive(Debug, Clone, PartialEq)]
pub struct Bookmark {
    pub id: String,
    pub book_id: String,
    pub chapter_index: usize,
    pub block_offset: usize,
    pub label: Option<String>,
    pub created_at: i64,
}

/// A note attached to a book, and optionally to a place in it.
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub id: String,
    pub book_id: String,
    pub chapter_index: Option<usize>,
    pub block_offset: Option<usize>,
    pub content: String,
    pub created_at: i64,
}

/// The learned pace for one book.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BookReadingPace {
    pub wpm: f64,
    pub active_ms: f64,
    pub updated_at: i64,
}

/// A book as the library list shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryEntry {
    pub id: String,
    pub title: String,
    pub author: String,
    pub source_path: String,
    pub import_hash: String,
    pub parser_version: Option<u32>,
    pub last_opened_at: i64,
    pub render_mode: RenderMode,
}

/// A library entry plus the reader's progress in it.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryEntryWithProgress {
    pub entry: LibraryEntry,
    pub chapter_index: Option<usize>,
    pub chapter_title: Option<String>,
    pub book_progress: Option<f64>,
}

/// Column the library list is sorted by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibrarySortKey {
    LastOpened,
    Title,
    Author,
    Progress,
}

impl LibrarySortKey {
    /// Cycle order used by the `s` shortcut in the library overlay.
    pub const ALL: [Self; 4] = [Self::LastOpened, Self::Title, Self::Author, Self::Progress];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LastOpened => "lastOpened",
            Self::Title => "title",
            Self::Author => "author",
            Self::Progress => "progress",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::LastOpened => "Last opened",
            Self::Title => "Title",
            Self::Author => "Author",
            Self::Progress => "Progress",
        }
    }

    #[must_use]
    pub fn from_id(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == value)
    }

    /// Next key, wrapping at the end.
    #[must_use]
    pub fn next(self) -> Self {
        let position = Self::ALL
            .iter()
            .position(|candidate| *candidate == self)
            .unwrap_or(0);
        Self::ALL[(position + 1) % Self::ALL.len()]
    }
}

/// Sort direction of the library list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

impl SortDirection {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ascending => "asc",
            Self::Descending => "desc",
        }
    }

    #[must_use]
    pub fn from_id(value: &str) -> Option<Self> {
        match value {
            "asc" => Some(Self::Ascending),
            "desc" => Some(Self::Descending),
            _ => None,
        }
    }

    #[must_use]
    pub const fn reversed(self) -> Self {
        match self {
            Self::Ascending => Self::Descending,
            Self::Descending => Self::Ascending,
        }
    }
}

/// Sort library entries in place.
///
/// Books with no recorded progress sort last regardless of direction, because
/// "not started" is not a position on the progress scale.
pub fn sort_library(
    entries: &mut [LibraryEntryWithProgress],
    key: LibrarySortKey,
    direction: SortDirection,
) {
    let descending = direction == SortDirection::Descending;
    match key {
        LibrarySortKey::LastOpened => entries.sort_by(|left, right| {
            let ordering = left.entry.last_opened_at.cmp(&right.entry.last_opened_at);
            if descending {
                ordering.reverse()
            } else {
                ordering
            }
        }),
        LibrarySortKey::Title => entries.sort_by(|left, right| {
            let ordering = left
                .entry
                .title
                .to_lowercase()
                .cmp(&right.entry.title.to_lowercase());
            if descending {
                ordering.reverse()
            } else {
                ordering
            }
        }),
        LibrarySortKey::Author => entries.sort_by(|left, right| {
            let ordering = left
                .entry
                .author
                .to_lowercase()
                .cmp(&right.entry.author.to_lowercase());
            if descending {
                ordering.reverse()
            } else {
                ordering
            }
        }),
        LibrarySortKey::Progress => {
            entries.sort_by(
                |left, right| match (left.book_progress, right.book_progress) {
                    (None, None) => std::cmp::Ordering::Equal,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (Some(left_progress), Some(right_progress)) => {
                        let ordering = left_progress
                            .partial_cmp(&right_progress)
                            .unwrap_or(std::cmp::Ordering::Equal);
                        if descending {
                            ordering.reverse()
                        } else {
                            ordering
                        }
                    }
                },
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        LibraryEntry, LibraryEntryWithProgress, LibrarySortKey, SortDirection, sort_library,
    };
    use crate::settings::RenderMode;

    fn entry(
        title: &str,
        author: &str,
        last_opened_at: i64,
        progress: Option<f64>,
    ) -> LibraryEntryWithProgress {
        LibraryEntryWithProgress {
            entry: LibraryEntry {
                id: title.to_lowercase(),
                title: title.to_owned(),
                author: author.to_owned(),
                source_path: format!("/books/{title}.epub"),
                import_hash: format!("hash-{title}"),
                parser_version: Some(3),
                last_opened_at,
                render_mode: RenderMode::Code,
            },
            chapter_index: progress.map(|_| 1),
            chapter_title: progress.map(|_| "Chapter Two".to_owned()),
            book_progress: progress,
        }
    }

    fn titles(entries: &[LibraryEntryWithProgress]) -> Vec<&str> {
        entries
            .iter()
            .map(|item| item.entry.title.as_str())
            .collect()
    }

    fn library() -> Vec<LibraryEntryWithProgress> {
        vec![
            entry("Dune", "Herbert", 300, Some(0.5)),
            entry("anathem", "Stephenson", 100, None),
            entry("Blindsight", "Watts", 200, Some(0.1)),
        ]
    }

    #[test]
    fn sorting_by_last_opened_defaults_to_most_recent_first() {
        let mut entries = library();
        sort_library(
            &mut entries,
            LibrarySortKey::LastOpened,
            SortDirection::Descending,
        );
        assert_eq!(titles(&entries), vec!["Dune", "Blindsight", "anathem"]);

        sort_library(
            &mut entries,
            LibrarySortKey::LastOpened,
            SortDirection::Ascending,
        );
        assert_eq!(titles(&entries), vec!["anathem", "Blindsight", "Dune"]);
    }

    #[test]
    fn sorting_by_title_and_author_ignores_case() {
        let mut entries = library();
        sort_library(
            &mut entries,
            LibrarySortKey::Title,
            SortDirection::Ascending,
        );
        assert_eq!(titles(&entries), vec!["anathem", "Blindsight", "Dune"]);

        sort_library(
            &mut entries,
            LibrarySortKey::Author,
            SortDirection::Ascending,
        );
        assert_eq!(titles(&entries), vec!["Dune", "anathem", "Blindsight"]);
    }

    #[test]
    fn unstarted_books_sort_last_in_both_directions() {
        let mut entries = library();
        sort_library(
            &mut entries,
            LibrarySortKey::Progress,
            SortDirection::Ascending,
        );
        assert_eq!(titles(&entries), vec!["Blindsight", "Dune", "anathem"]);

        sort_library(
            &mut entries,
            LibrarySortKey::Progress,
            SortDirection::Descending,
        );
        assert_eq!(titles(&entries), vec!["Dune", "Blindsight", "anathem"]);
    }

    #[test]
    fn sort_keys_cycle_and_round_trip() {
        let mut key = LibrarySortKey::LastOpened;
        let mut seen = Vec::new();
        for _ in 0..LibrarySortKey::ALL.len() {
            seen.push(key.as_str());
            assert_eq!(LibrarySortKey::from_id(key.as_str()), Some(key));
            key = key.next();
        }
        assert_eq!(seen, vec!["lastOpened", "title", "author", "progress"]);
        assert_eq!(key, LibrarySortKey::LastOpened);
    }

    #[test]
    fn directions_reverse_and_round_trip() {
        assert_eq!(
            SortDirection::Ascending.reversed(),
            SortDirection::Descending
        );
        assert_eq!(
            SortDirection::from_id("desc"),
            Some(SortDirection::Descending)
        );
        assert_eq!(SortDirection::from_id("sideways"), None);
    }
}

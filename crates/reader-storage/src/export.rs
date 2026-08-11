//! Export and import of reading state.
//!
//! The file format is v1's version-1 JSON, so exports move between the two
//! implementations. Books are matched by content hash rather than by id or path,
//! which is what lets state follow a book across machines.
//!
//! Merging is additive and conservative: bookmarks, notes, and tags are added
//! when absent and never overwritten, and a position is only taken when the
//! export is newer than the local book's last-opened time.

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::{Storage, StorageError, from_index, to_index};

/// A reading position, keyed by book content hash.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPosition {
    pub book_import_hash: String,
    pub book_title: String,
    pub chapter_index: usize,
    pub block_offset: usize,
    pub book_progress: f64,
}

/// A bookmark, keyed by book content hash.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportBookmark {
    pub book_import_hash: String,
    pub book_title: String,
    pub chapter_index: usize,
    pub block_offset: usize,
    pub label: Option<String>,
    pub created_at: i64,
}

/// A note, keyed by book content hash.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportNote {
    pub book_import_hash: String,
    pub book_title: String,
    pub chapter_index: Option<usize>,
    pub block_offset: Option<usize>,
    pub content: String,
    pub created_at: i64,
}

/// A tag, keyed by book content hash.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportTag {
    pub book_import_hash: String,
    pub book_title: String,
    pub tag: String,
}

/// One export file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportData {
    /// Always 1; a different value is refused rather than guessed at.
    pub version: u32,
    /// ISO-8601 timestamp, used to decide whether positions are newer.
    pub exported_at: String,
    pub positions: Vec<ExportPosition>,
    pub bookmarks: Vec<ExportBookmark>,
    pub notes: Vec<ExportNote>,
    pub tags: Vec<ExportTag>,
}

/// What a merge changed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImportSummary {
    pub positions_updated: usize,
    pub bookmarks_added: usize,
    pub notes_added: usize,
    pub tags_added: usize,
}

impl ImportSummary {
    /// Whether the merge changed anything at all.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.positions_updated == 0
            && self.bookmarks_added == 0
            && self.notes_added == 0
            && self.tags_added == 0
    }
}

/// Parse an ISO-8601 timestamp into epoch milliseconds.
///
/// Only the shapes v1's `Date#toISOString` produces are accepted:
/// `YYYY-MM-DDTHH:MM:SS[.mmm]Z`, always UTC.
fn parse_iso8601_millis(value: &str) -> Option<i64> {
    let value = value.trim();
    let (date, rest) = value.split_once('T')?;
    let time = rest.strip_suffix('Z')?;

    let mut date_parts = date.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: i64 = date_parts.next()?.parse().ok()?;
    let day: i64 = date_parts.next()?.parse().ok()?;
    if date_parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    let (clock, fraction) = match time.split_once('.') {
        Some((clock, fraction)) => (clock, fraction),
        None => (time, "0"),
    };
    let mut clock_parts = clock.split(':');
    let hour: i64 = clock_parts.next()?.parse().ok()?;
    let minute: i64 = clock_parts.next()?.parse().ok()?;
    let second: i64 = clock_parts.next()?.parse().ok()?;
    if clock_parts.next().is_some() || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    let millis: i64 = format!("{fraction:0<3}")
        .get(..3)
        .and_then(|digits| digits.parse().ok())?;

    // Days from the civil calendar, using Howard Hinnant's algorithm.
    let year_adjusted = if month <= 2 { year - 1 } else { year };
    let era = year_adjusted.div_euclid(400);
    let year_of_era = year_adjusted - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;

    Some(((days * 24 + hour) * 60 + minute) * 60_000 + second * 1_000 + millis)
}

/// Format epoch milliseconds the way `Date#toISOString` does.
fn format_iso8601_millis(millis: i64) -> String {
    let days = millis.div_euclid(86_400_000);
    let time_of_day = millis.rem_euclid(86_400_000);

    // Inverse of the civil-from-days algorithm above.
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    let year = if month <= 2 { year + 1 } else { year };

    let hour = time_of_day / 3_600_000;
    let minute = (time_of_day % 3_600_000) / 60_000;
    let second = (time_of_day % 60_000) / 1_000;
    let millis = time_of_day % 1_000;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

impl Storage {
    /// Collect every position, bookmark, note, and tag into an export.
    ///
    /// `now` stamps the export; it decides whether a later import overwrites
    /// positions, so it must come from the caller's clock.
    pub fn export_all(&self, now: i64) -> Result<ExportData, StorageError> {
        let positions = self
            .connection()
            .prepare(
                "SELECT b.import_hash, b.title, p.chapter_index, p.block_offset, p.book_progress
                 FROM positions p JOIN books b ON b.id = p.book_id",
            )?
            .query_map([], |row| {
                Ok(ExportPosition {
                    book_import_hash: row.get(0)?,
                    book_title: row.get(1)?,
                    chapter_index: to_index(row.get(2)?),
                    block_offset: to_index(row.get(3)?),
                    book_progress: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;

        let bookmarks = self
            .connection()
            .prepare(
                "SELECT b.import_hash, b.title, m.chapter_index, m.block_offset, m.label, m.created_at
                 FROM bookmarks m JOIN books b ON b.id = m.book_id",
            )?
            .query_map([], |row| {
                Ok(ExportBookmark {
                    book_import_hash: row.get(0)?,
                    book_title: row.get(1)?,
                    chapter_index: to_index(row.get(2)?),
                    block_offset: to_index(row.get(3)?),
                    label: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;

        let notes = self
            .connection()
            .prepare(
                "SELECT b.import_hash, b.title, n.chapter_index, n.block_offset, n.content, n.created_at
                 FROM notes n JOIN books b ON b.id = n.book_id",
            )?
            .query_map([], |row| {
                Ok(ExportNote {
                    book_import_hash: row.get(0)?,
                    book_title: row.get(1)?,
                    chapter_index: row.get::<_, Option<i64>>(2)?.map(to_index),
                    block_offset: row.get::<_, Option<i64>>(3)?.map(to_index),
                    content: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;

        let tags = self
            .connection()
            .prepare(
                "SELECT b.import_hash, b.title, t.tag
                 FROM book_tags t JOIN books b ON b.id = t.book_id",
            )?
            .query_map([], |row| {
                Ok(ExportTag {
                    book_import_hash: row.get(0)?,
                    book_title: row.get(1)?,
                    tag: row.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;

        Ok(ExportData {
            version: 1,
            exported_at: format_iso8601_millis(now),
            positions,
            bookmarks,
            notes,
            tags,
        })
    }

    /// Merge an export into the library, matching books by content hash.
    ///
    /// Entries for books that are not in the library are skipped, not an error:
    /// an export usually covers more books than any one machine holds.
    pub fn import_merge(&mut self, data: &ExportData) -> Result<ImportSummary, StorageError> {
        if data.version != 1 {
            return Err(StorageError::InvalidExport(format!(
                "Unsupported export version: {}",
                data.version
            )));
        }
        let exported_at = parse_iso8601_millis(&data.exported_at).ok_or_else(|| {
            StorageError::InvalidExport("Export file has an invalid exportedAt date.".to_owned())
        })?;

        let mut summary = ImportSummary::default();
        let transaction = self.connection.transaction()?;

        for position in &data.positions {
            let book: Option<(String, i64)> = transaction
                .query_row(
                    "SELECT id, last_opened_at FROM books WHERE import_hash = ?",
                    params![position.book_import_hash],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let Some((book_id, last_opened_at)) = book else {
                continue;
            };
            if exported_at <= last_opened_at {
                continue;
            }
            transaction.execute(
                "INSERT INTO positions (book_id, chapter_index, chapter_progress, book_progress, block_offset)
                 VALUES (?, ?, 0, ?, ?)
                 ON CONFLICT(book_id) DO UPDATE SET
                   chapter_index = excluded.chapter_index,
                   chapter_progress = 0,
                   book_progress = excluded.book_progress,
                   block_offset = excluded.block_offset",
                params![
                    book_id,
                    from_index(position.chapter_index),
                    position.book_progress,
                    from_index(position.block_offset)
                ],
            )?;
            summary.positions_updated += 1;
        }

        for bookmark in &data.bookmarks {
            let Some(book_id) = book_id_for_hash(&transaction, &bookmark.book_import_hash)? else {
                continue;
            };
            // A bookmark is identified by where it points, not by its label.
            let exists: Option<String> = transaction
                .query_row(
                    "SELECT id FROM bookmarks WHERE book_id = ? AND chapter_index = ? AND block_offset = ?",
                    params![
                        book_id,
                        from_index(bookmark.chapter_index),
                        from_index(bookmark.block_offset)
                    ],
                    |row| row.get(0),
                )
                .optional()?;
            if exists.is_some() {
                continue;
            }
            transaction.execute(
                "INSERT INTO bookmarks (id, book_id, chapter_index, block_offset, label, created_at)
                 VALUES (?, ?, ?, ?, ?, ?)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    book_id,
                    from_index(bookmark.chapter_index),
                    from_index(bookmark.block_offset),
                    bookmark.label,
                    bookmark.created_at,
                ],
            )?;
            summary.bookmarks_added += 1;
        }

        for note in &data.notes {
            let Some(book_id) = book_id_for_hash(&transaction, &note.book_import_hash)? else {
                continue;
            };
            // A note is identified by its text and creation time, so the same
            // note imported twice is not duplicated.
            let exists: Option<String> = transaction
                .query_row(
                    "SELECT id FROM notes WHERE book_id = ? AND content = ? AND created_at = ?",
                    params![book_id, note.content, note.created_at],
                    |row| row.get(0),
                )
                .optional()?;
            if exists.is_some() {
                continue;
            }
            transaction.execute(
                "INSERT INTO notes (id, book_id, chapter_index, block_offset, content, created_at)
                 VALUES (?, ?, ?, ?, ?, ?)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    book_id,
                    note.chapter_index.map(from_index),
                    note.block_offset.map(from_index),
                    note.content,
                    note.created_at,
                ],
            )?;
            summary.notes_added += 1;
        }

        for tag in &data.tags {
            let Some(book_id) = book_id_for_hash(&transaction, &tag.book_import_hash)? else {
                continue;
            };
            let changed = transaction.execute(
                "INSERT OR IGNORE INTO book_tags (book_id, tag) VALUES (?, ?)",
                params![book_id, tag.tag],
            )?;
            if changed > 0 {
                summary.tags_added += 1;
            }
        }

        transaction.commit()?;
        Ok(summary)
    }
}

fn book_id_for_hash(
    transaction: &rusqlite::Transaction<'_>,
    import_hash: &str,
) -> rusqlite::Result<Option<String>> {
    transaction
        .query_row(
            "SELECT id FROM books WHERE import_hash = ?",
            params![import_hash],
            |row| row.get(0),
        )
        .optional()
}

#[cfg(test)]
mod tests {
    use reader_core::{ReadingPosition, RenderMode};

    use super::{
        ExportBookmark, ExportData, ExportNote, ExportPosition, ExportTag, format_iso8601_millis,
        parse_iso8601_millis,
    };
    use crate::Storage;

    fn book(id: &str, hash: &str) -> reader_core::CanonicalBook {
        reader_core::CanonicalBook {
            id: id.to_owned(),
            title: "My Book".to_owned(),
            author: "Author X".to_owned(),
            source_path: format!("/books/{id}.epub"),
            import_hash: hash.to_owned(),
            parser_version: Some(3),
            diagnostics: Vec::new(),
            chapters: Vec::new(),
            cover_path: None,
        }
    }

    fn storage_with_book(hash: &str, last_opened_at: i64) -> Storage {
        let mut storage = Storage::open_in_memory().expect("open");
        storage
            .save_book(&book("b1", hash), RenderMode::Plain, last_opened_at)
            .expect("save");
        storage
    }

    fn export(exported_at: &str) -> ExportData {
        ExportData {
            version: 1,
            exported_at: exported_at.to_owned(),
            positions: Vec::new(),
            bookmarks: Vec::new(),
            notes: Vec::new(),
            tags: Vec::new(),
        }
    }

    #[test]
    fn timestamps_round_trip_in_the_v1_iso_format() {
        // Values cross-checked against Date#toISOString.
        assert_eq!(format_iso8601_millis(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(
            format_iso8601_millis(1_700_000_000_000),
            "2023-11-14T22:13:20.000Z"
        );
        assert_eq!(
            parse_iso8601_millis("2023-11-14T22:13:20.000Z"),
            Some(1_700_000_000_000)
        );
        assert_eq!(parse_iso8601_millis("1970-01-01T00:00:00Z"), Some(0));
        for millis in [0, 1, 86_399_999, 1_000_000_000_123, 1_700_000_000_000] {
            assert_eq!(
                parse_iso8601_millis(&format_iso8601_millis(millis)),
                Some(millis),
                "{millis} did not round-trip"
            );
        }
    }

    #[test]
    fn invalid_timestamps_are_rejected() {
        for value in [
            "",
            "not-a-date",
            "2023-11-14 22:13:20Z",
            "2023-11-14T22:13:20+01:00",
            "2023-13-14T22:13:20.000Z",
            "2023-11-14T25:13:20.000Z",
        ] {
            assert!(parse_iso8601_millis(value).is_none(), "{value:?} parsed");
        }
    }

    #[test]
    fn an_export_is_version_one_with_every_section_present() {
        let storage = Storage::open_in_memory().expect("open");
        let data = storage.export_all(1_700_000_000_000).expect("export");
        assert_eq!(data.version, 1);
        assert_eq!(data.exported_at, "2023-11-14T22:13:20.000Z");
        assert!(data.positions.is_empty());
        assert!(data.bookmarks.is_empty());
        assert!(data.notes.is_empty());
        assert!(data.tags.is_empty());
    }

    #[test]
    fn exports_are_keyed_by_content_hash_and_carry_the_title() {
        let storage = storage_with_book("sha256abc", 1_000);
        storage
            .save_position(
                "b1",
                ReadingPosition {
                    chapter_index: 3,
                    chapter_progress: 0.5,
                    book_progress: 0.31,
                    block_offset: 42,
                },
                1_000,
            )
            .expect("position");
        storage
            .add_bookmark("b1", 2, 10, Some("My mark"), 2_000)
            .expect("bookmark");
        storage
            .add_note("b1", "Great passage", Some(1), Some(5), 3_000)
            .expect("note");
        storage.add_tag("b1", "fiction").expect("tag");

        let data = storage.export_all(4_000).expect("export");

        assert_eq!(data.positions.len(), 1);
        assert_eq!(data.positions[0].book_import_hash, "sha256abc");
        assert_eq!(data.positions[0].book_title, "My Book");
        assert_eq!(data.positions[0].chapter_index, 3);
        assert_eq!(data.positions[0].block_offset, 42);
        assert!((data.positions[0].book_progress - 0.31).abs() < 1e-9);
        assert_eq!(data.bookmarks[0].label.as_deref(), Some("My mark"));
        assert_eq!(data.notes[0].content, "Great passage");
        assert_eq!(data.tags[0].tag, "fiction");
    }

    #[test]
    fn the_json_shape_matches_the_v1_file_format() {
        let storage = storage_with_book("hash", 1_000);
        let data = storage.export_all(0).expect("export");
        let json = serde_json::to_value(&data).expect("serialize");
        assert_eq!(json["version"], 1);
        assert_eq!(json["exportedAt"], "1970-01-01T00:00:00.000Z");
        assert!(json["positions"].is_array());

        // A v1 file deserializes unchanged.
        let parsed: ExportData = serde_json::from_str(
            r#"{"version":1,"exportedAt":"2023-11-14T22:13:20.000Z",
                "positions":[{"bookImportHash":"h","bookTitle":"T","chapterIndex":1,"blockOffset":2,"bookProgress":0.5}],
                "bookmarks":[{"bookImportHash":"h","bookTitle":"T","chapterIndex":1,"blockOffset":2,"label":null,"createdAt":10}],
                "notes":[{"bookImportHash":"h","bookTitle":"T","chapterIndex":null,"blockOffset":null,"content":"c","createdAt":11}],
                "tags":[{"bookImportHash":"h","bookTitle":"T","tag":"x"}]}"#,
        )
        .expect("v1 export should parse");
        assert_eq!(parsed.positions[0].chapter_index, 1);
        assert_eq!(parsed.notes[0].chapter_index, None);
    }

    #[test]
    fn a_newer_export_replaces_the_local_position() {
        let mut storage = storage_with_book("hash1", 1_000);
        let mut data = export("1970-01-01T00:00:02.000Z");
        data.positions.push(ExportPosition {
            book_import_hash: "hash1".to_owned(),
            book_title: "Book".to_owned(),
            chapter_index: 5,
            block_offset: 20,
            book_progress: 0.7,
        });

        let summary = storage.import_merge(&data).expect("merge");

        assert_eq!(summary.positions_updated, 1);
        let position = storage.position("b1").expect("load").expect("position");
        assert_eq!(position.chapter_index, 5);
        assert_eq!(position.block_offset, 20);
        // v1 resets chapter progress, since the export does not carry it.
        assert!((position.chapter_progress - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn an_older_export_leaves_the_local_position_alone() {
        let mut storage = storage_with_book("hash1", 10_000);
        storage
            .save_position(
                "b1",
                ReadingPosition {
                    chapter_index: 3,
                    ..ReadingPosition::start()
                },
                10_000,
            )
            .expect("position");
        let mut data = export("1970-01-01T00:00:05.000Z");
        data.positions.push(ExportPosition {
            book_import_hash: "hash1".to_owned(),
            book_title: "Book".to_owned(),
            chapter_index: 99,
            block_offset: 99,
            book_progress: 0.99,
        });

        let summary = storage.import_merge(&data).expect("merge");

        assert_eq!(summary.positions_updated, 0);
        assert_eq!(
            storage
                .position("b1")
                .expect("load")
                .expect("position")
                .chapter_index,
            3
        );
    }

    #[test]
    fn entries_for_unknown_books_are_skipped() {
        let mut storage = storage_with_book("hash1", 1_000);
        let mut data = export("2023-11-14T22:13:20.000Z");
        data.positions.push(ExportPosition {
            book_import_hash: "unknown".to_owned(),
            book_title: "Ghost".to_owned(),
            chapter_index: 1,
            block_offset: 0,
            book_progress: 0.1,
        });
        data.tags.push(ExportTag {
            book_import_hash: "unknown".to_owned(),
            book_title: "Ghost".to_owned(),
            tag: "ghostly".to_owned(),
        });

        let summary = storage.import_merge(&data).expect("merge");

        assert!(summary.is_empty());
    }

    #[test]
    fn bookmarks_notes_and_tags_merge_additively_without_duplicates() {
        let mut storage = storage_with_book("hash1", 1_000);
        storage
            .add_bookmark("b1", 1, 5, Some("existing"), 500)
            .expect("bookmark");
        storage
            .add_note("b1", "existing note", Some(1), Some(5), 600)
            .expect("note");
        storage.add_tag("b1", "kept").expect("tag");

        let mut data = export("2023-11-14T22:13:20.000Z");
        data.bookmarks.push(ExportBookmark {
            book_import_hash: "hash1".to_owned(),
            book_title: "Book".to_owned(),
            chapter_index: 1,
            block_offset: 5,
            label: Some("a different label, same place".to_owned()),
            created_at: 700,
        });
        data.bookmarks.push(ExportBookmark {
            book_import_hash: "hash1".to_owned(),
            book_title: "Book".to_owned(),
            chapter_index: 2,
            block_offset: 9,
            label: None,
            created_at: 800,
        });
        data.notes.push(ExportNote {
            book_import_hash: "hash1".to_owned(),
            book_title: "Book".to_owned(),
            chapter_index: Some(1),
            block_offset: Some(5),
            content: "existing note".to_owned(),
            created_at: 600,
        });
        data.notes.push(ExportNote {
            book_import_hash: "hash1".to_owned(),
            book_title: "Book".to_owned(),
            chapter_index: None,
            block_offset: None,
            content: "new note".to_owned(),
            created_at: 900,
        });
        data.tags.push(ExportTag {
            book_import_hash: "hash1".to_owned(),
            book_title: "Book".to_owned(),
            tag: "kept".to_owned(),
        });
        data.tags.push(ExportTag {
            book_import_hash: "hash1".to_owned(),
            book_title: "Book".to_owned(),
            tag: "added".to_owned(),
        });

        let summary = storage.import_merge(&data).expect("merge");

        assert_eq!(summary.bookmarks_added, 1);
        assert_eq!(summary.notes_added, 1);
        assert_eq!(summary.tags_added, 1);
        assert_eq!(storage.bookmarks("b1").expect("list").len(), 2);
        assert_eq!(storage.notes("b1").expect("list").len(), 2);
        assert_eq!(storage.tags("b1").expect("list"), vec!["added", "kept"]);
    }

    #[test]
    fn merging_the_same_export_twice_changes_nothing_the_second_time() {
        let mut storage = storage_with_book("hash1", 1_000);
        let mut data = export("2023-11-14T22:13:20.000Z");
        data.bookmarks.push(ExportBookmark {
            book_import_hash: "hash1".to_owned(),
            book_title: "Book".to_owned(),
            chapter_index: 1,
            block_offset: 5,
            label: None,
            created_at: 700,
        });
        data.notes.push(ExportNote {
            book_import_hash: "hash1".to_owned(),
            book_title: "Book".to_owned(),
            chapter_index: Some(1),
            block_offset: Some(5),
            content: "note".to_owned(),
            created_at: 800,
        });

        let first = storage.import_merge(&data).expect("merge");
        assert_eq!(first.bookmarks_added, 1);
        assert_eq!(first.notes_added, 1);

        let second = storage.import_merge(&data).expect("merge again");
        assert!(second.is_empty());
    }

    #[test]
    fn an_unsupported_version_or_timestamp_is_refused() {
        let mut storage = storage_with_book("hash1", 1_000);
        let mut future = export("2023-11-14T22:13:20.000Z");
        future.version = 2;
        assert!(
            storage
                .import_merge(&future)
                .expect_err("version 2 is refused")
                .to_string()
                .contains("Unsupported export version")
        );

        let broken = export("yesterday");
        assert_eq!(
            storage
                .import_merge(&broken)
                .expect_err("bad date")
                .to_string(),
            "Export file has an invalid exportedAt date."
        );
    }

    #[test]
    fn an_export_round_trips_through_a_second_library() {
        let source = storage_with_book("shared-hash", 1_000);
        source
            .add_bookmark("b1", 4, 8, Some("here"), 1_500)
            .expect("bookmark");
        source.add_tag("b1", "shared").expect("tag");
        source
            .save_position(
                "b1",
                ReadingPosition {
                    chapter_index: 2,
                    chapter_progress: 0.4,
                    book_progress: 0.2,
                    block_offset: 6,
                },
                1_000,
            )
            .expect("position");
        let data = source.export_all(9_000).expect("export");

        // A second machine holds the same book, opened earlier.
        let mut destination = storage_with_book("shared-hash", 2_000);
        let summary = destination.import_merge(&data).expect("merge");

        assert_eq!(summary.positions_updated, 1);
        assert_eq!(summary.bookmarks_added, 1);
        assert_eq!(summary.tags_added, 1);
        assert_eq!(
            destination
                .position("b1")
                .expect("load")
                .expect("position")
                .chapter_index,
            2
        );
    }
}

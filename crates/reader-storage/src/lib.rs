//! SQLite-backed library storage.
//!
//! The schema keeps v1's tables and columns, so both implementations can open the
//! same `library.db` during the beta. It differs in two ways, both applied to an
//! existing database on open and both invisible to v1: `chapters` is keyed per
//! book rather than globally, and four missing indexes are added. See
//! `docs/migration/improvements.md`.
//!
//! Write paths that touch several tables run in one transaction, because a
//! half-saved book or a half-saved settings page is worse than a failed one.
//!
//! Timestamps are passed in rather than read from the clock, which keeps the
//! storage layer deterministic under test.

pub mod export;
pub mod paths;

use std::path::Path;

use reader_core::{
    BookReadingPace, Bookmark, CanonicalBook, CanonicalChapter, DiagnosticSeverity,
    ImportDiagnostic, LibraryEntry, LibraryEntryWithProgress, LibrarySortKey, Note,
    ReadingPosition, RenderMode, SortDirection,
};
use rusqlite::{Connection, OptionalExtension, params};

pub use export::{ExportData, ImportSummary};
pub use paths::AppPaths;

/// The EPUB parser version stored on import, used to decide whether a book
/// should be re-extracted. Mirrors `reader_formats::EPUB_PARSER_VERSION`.
pub const CURRENT_EPUB_PARSER_VERSION: u32 = 3;

/// Anything that can go wrong talking to the database.
#[derive(Debug)]
pub enum StorageError {
    Database(rusqlite::Error),
    Io(std::io::Error),
    /// Stored JSON that no longer matches the block schema.
    CorruptBlocks {
        chapter_id: String,
        detail: String,
    },
    /// An export file the reader cannot merge.
    InvalidExport(String),
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(error) => write!(formatter, "{error}"),
            Self::Io(error) => write!(formatter, "{error}"),
            Self::CorruptBlocks { chapter_id, detail } => {
                write!(
                    formatter,
                    "chapter {chapter_id} has unreadable blocks: {detail}"
                )
            }
            Self::InvalidExport(detail) => write!(formatter, "{detail}"),
        }
    }
}

impl std::error::Error for StorageError {}

impl From<rusqlite::Error> for StorageError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}

impl From<std::io::Error> for StorageError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

type Result<T> = std::result::Result<T, StorageError>;

/// The v1 schema, created on first open and idempotent afterwards.
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS books (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  author TEXT NOT NULL,
  source_path TEXT NOT NULL,
  import_hash TEXT NOT NULL,
  parser_version INTEGER NOT NULL DEFAULT 1,
  last_opened_at INTEGER NOT NULL,
  render_mode TEXT NOT NULL
);
-- The primary key is per book, unlike v1's global one; see MIGRATE_CHAPTERS_KEY.
CREATE TABLE IF NOT EXISTS chapters (
  id TEXT NOT NULL,
  book_id TEXT NOT NULL,
  chapter_index INTEGER NOT NULL,
  title TEXT NOT NULL,
  href TEXT NOT NULL,
  depth INTEGER NOT NULL,
  word_count INTEGER NOT NULL,
  blocks_json TEXT NOT NULL,
  PRIMARY KEY (book_id, id)
);
CREATE TABLE IF NOT EXISTS diagnostics (
  book_id TEXT NOT NULL,
  severity TEXT NOT NULL,
  message TEXT NOT NULL,
  context TEXT
);
CREATE TABLE IF NOT EXISTS positions (
  book_id TEXT PRIMARY KEY,
  chapter_index INTEGER NOT NULL,
  chapter_progress REAL NOT NULL,
  book_progress REAL NOT NULL,
  block_offset INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS command_history (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  raw_command TEXT NOT NULL,
  normalized_name TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS bookmarks (
  id TEXT PRIMARY KEY,
  book_id TEXT NOT NULL,
  chapter_index INTEGER NOT NULL,
  block_offset INTEGER NOT NULL,
  label TEXT,
  created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS book_tags (
  book_id TEXT NOT NULL,
  tag TEXT NOT NULL,
  PRIMARY KEY (book_id, tag)
);
CREATE TABLE IF NOT EXISTS notes (
  id TEXT PRIMARY KEY,
  book_id TEXT NOT NULL,
  chapter_index INTEGER,
  block_offset INTEGER,
  content TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS reading_pace (
  book_id TEXT PRIMARY KEY,
  wpm REAL NOT NULL,
  active_ms INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  FOREIGN KEY (book_id) REFERENCES books(id)
);
CREATE INDEX IF NOT EXISTS idx_book_tags_tag ON book_tags(tag);
CREATE INDEX IF NOT EXISTS idx_notes_book_id ON notes(book_id);
";

/// Indexes v1 lacked. They change no data and no query results, only how fast
/// the reader answers them: chapter loads and bookmark lists were full scans.
const INDEXES: &str = "
CREATE INDEX IF NOT EXISTS idx_chapters_book ON chapters(book_id, chapter_index);
CREATE INDEX IF NOT EXISTS idx_bookmarks_book ON bookmarks(book_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_diagnostics_book ON diagnostics(book_id);
CREATE INDEX IF NOT EXISTS idx_books_last_opened ON books(last_opened_at DESC);
";

/// Rebuild `chapters` with a per-book primary key.
///
/// v1 made `chapters.id` a global primary key while deriving it from the
/// chapter's href and index alone, so two books that share an href — a common
/// shape, since `text/ch1.xhtml` is not unusual — could not both be stored: the
/// second import failed on a constraint violation. The id stays as it was, so
/// nothing that reads a chapter changes; only its uniqueness scope does.
const MIGRATE_CHAPTERS_KEY: &str = "
CREATE TABLE chapters_migrated (
  id TEXT NOT NULL,
  book_id TEXT NOT NULL,
  chapter_index INTEGER NOT NULL,
  title TEXT NOT NULL,
  href TEXT NOT NULL,
  depth INTEGER NOT NULL,
  word_count INTEGER NOT NULL,
  blocks_json TEXT NOT NULL,
  PRIMARY KEY (book_id, id)
);
INSERT INTO chapters_migrated (id, book_id, chapter_index, title, href, depth, word_count, blocks_json)
  SELECT id, book_id, chapter_index, title, href, depth, word_count, blocks_json FROM chapters;
DROP TABLE chapters;
ALTER TABLE chapters_migrated RENAME TO chapters;
";

/// Replace a Toggl API key in a command line with a placeholder.
///
/// Command history is written to disk, so a key typed into the command bar must
/// never land there — including keys written by older builds, which are
/// rewritten when the database is opened.
#[must_use]
pub fn redact_sensitive_command(raw_command: &str, normalized_name: &str) -> String {
    if !normalized_name.eq_ignore_ascii_case("toggl") {
        return raw_command.to_owned();
    }
    let trimmed = raw_command.trim_start();
    let leading = &raw_command[..raw_command.len() - trimmed.len()];
    let body = trimmed.strip_prefix('/').unwrap_or(trimmed);
    let slash = if trimmed.starts_with('/') { "/" } else { "" };

    let mut parts = body.splitn(3, char::is_whitespace);
    let (Some(command), Some(action)) = (parts.next(), parts.next()) else {
        return raw_command.to_owned();
    };
    if !command.eq_ignore_ascii_case("toggl") || !action.eq_ignore_ascii_case("auth") {
        return raw_command.to_owned();
    }
    let Some(rest) = parts.next().map(str::trim) else {
        return raw_command.to_owned();
    };
    if rest.is_empty() || rest == "<redacted>" || rest.starts_with("--") {
        return raw_command.to_owned();
    }
    format!("{leading}{slash}{command} {action} <redacted>")
}

/// The library database.
pub struct Storage {
    connection: Connection,
    chapter_cache_dir: std::path::PathBuf,
}

impl Storage {
    /// Open (or create) the database under `paths`, applying schema and
    /// migrations. Opening is idempotent and safe on a v1 database.
    pub fn open(paths: &AppPaths) -> Result<Self> {
        paths.ensure()?;
        let connection = Connection::open(&paths.db_path)?;
        Self::initialize(connection, paths.chapter_cache_dir())
    }

    /// Open an in-memory database, for tests and dry runs.
    pub fn open_in_memory() -> Result<Self> {
        let connection = Connection::open_in_memory()?;
        Self::initialize(connection, std::env::temp_dir().join("reader-cache-unused"))
    }

    fn initialize(connection: Connection, chapter_cache_dir: std::path::PathBuf) -> Result<Self> {
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.execute_batch(SCHEMA)?;

        let mut storage = Self {
            connection,
            chapter_cache_dir,
        };
        storage.migrate()?;
        storage.redact_command_history()?;
        storage.seed_settings()?;
        Ok(storage)
    }

    /// The directory holding this database's cached book JSON.
    #[must_use]
    pub fn chapter_cache_dir(&self) -> &Path {
        &self.chapter_cache_dir
    }

    /// Escape hatch for tests and one-off maintenance queries.
    #[must_use]
    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    /// Bring an older database up to the current schema.
    ///
    /// Every step is idempotent and preserves existing rows, so a v1 database
    /// can be opened by either implementation afterwards.
    fn migrate(&mut self) -> Result<()> {
        let has_parser_version = self
            .connection
            .prepare("PRAGMA table_info(books)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .flatten()
            .any(|name| name == "parser_version");
        if !has_parser_version {
            self.connection.execute_batch(
                "ALTER TABLE books ADD COLUMN parser_version INTEGER NOT NULL DEFAULT 1",
            )?;
        }

        if self.chapters_key_is_global()? {
            // One transaction, so a failure leaves the old table in place.
            let transaction = self.connection.transaction()?;
            transaction.execute_batch(MIGRATE_CHAPTERS_KEY)?;
            transaction.commit()?;
        }

        self.connection.execute_batch(INDEXES)?;
        Ok(())
    }

    /// Whether `chapters` still uses v1's single-column primary key.
    fn chapters_key_is_global(&self) -> Result<bool> {
        let key_columns: Vec<String> = self
            .connection
            .prepare("PRAGMA table_info(chapters)")?
            .query_map([], |row| {
                Ok((row.get::<_, String>(1)?, row.get::<_, i64>(5)?))
            })?
            .collect::<rusqlite::Result<Vec<(String, i64)>>>()?
            .into_iter()
            .filter(|(_, key_position)| *key_position > 0)
            .map(|(name, _)| name)
            .collect();
        Ok(key_columns == ["id"])
    }

    /// Rewrite Toggl keys stored by earlier builds.
    fn redact_command_history(&mut self) -> Result<()> {
        let rows: Vec<(i64, String, String)> = self
            .connection
            .prepare(
                "SELECT id, raw_command, normalized_name FROM command_history
                 WHERE lower(normalized_name) = 'toggl'",
            )?
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;

        let transaction = self.connection.transaction()?;
        let mut update = transaction
            .prepare_cached("UPDATE command_history SET raw_command = ? WHERE id = ?")?;
        for (id, raw_command, normalized_name) in rows {
            let redacted = redact_sensitive_command(&raw_command, &normalized_name);
            if redacted != raw_command {
                update.execute(params![redacted, id])?;
            }
        }
        drop(update);
        transaction.commit()?;
        Ok(())
    }

    /// Write default settings for keys that have no row yet.
    fn seed_settings(&mut self) -> Result<()> {
        let defaults = reader_core::AppSettings::default();
        let transaction = self.connection.transaction()?;
        let mut insert = transaction
            .prepare_cached("INSERT OR IGNORE INTO settings (key, value) VALUES (?, ?)")?;
        for (key, value) in defaults.entries() {
            insert.execute(params![key, value])?;
        }
        drop(insert);
        transaction.commit()?;
        Ok(())
    }

    // ── settings ─────────────────────────────────────────────────────────────

    /// Every reader preference, with invalid stored values falling back to their
    /// default.
    pub fn settings(&self) -> Result<reader_core::AppSettings> {
        let rows: Vec<(String, String)> = self
            .connection
            .prepare("SELECT key, value FROM settings")?
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(reader_core::AppSettings::from_stored(
            rows.iter()
                .map(|(key, value)| (key.as_str(), value.as_str())),
        ))
    }

    /// A raw settings value, for keys outside [`reader_core::AppSettings`] such
    /// as the Toggl token and the global pace model.
    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .connection
            .query_row(
                "SELECT value FROM settings WHERE key = ?",
                params![key],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// Write one raw settings value.
    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.connection.execute(
            "INSERT INTO settings (key, value) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Remove one settings row, used when disconnecting an integration.
    pub fn delete_setting(&self, key: &str) -> Result<()> {
        self.connection
            .execute("DELETE FROM settings WHERE key = ?", params![key])?;
        Ok(())
    }

    /// Save every preference in one transaction, so a rejected value leaves the
    /// previous configuration intact.
    pub fn save_settings(&mut self, settings: &reader_core::AppSettings) -> Result<()> {
        let transaction = self.connection.transaction()?;
        let mut upsert = transaction.prepare_cached(
            "INSERT INTO settings (key, value) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )?;
        for (key, value) in settings.entries() {
            upsert.execute(params![key, value])?;
        }
        drop(upsert);
        transaction.commit()?;
        Ok(())
    }

    // ── books ────────────────────────────────────────────────────────────────

    /// The id of a stored book with the same content whose file has since
    /// disappeared — that is, the same book after a rename or move.
    fn renamed_book_id(&self, book: &CanonicalBook) -> Result<Option<String>> {
        let candidates: Vec<(String, String)> = self
            .connection
            .prepare_cached(
                "SELECT id, source_path FROM books
                 WHERE import_hash = ? AND id != ?
                 ORDER BY last_opened_at DESC",
            )?
            .query_map(params![book.import_hash, book.id], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(candidates
            .into_iter()
            .find(|(_, source_path)| !Path::new(source_path).exists())
            .map(|(id, _)| id))
    }

    /// Store a book and its chapters, replacing any previous extraction.
    ///
    /// Returns the id the book was stored under, which differs from
    /// `book.id` when the file was recognized as a renamed copy: reusing the old
    /// id preserves the reader's position, bookmarks, and notes.
    pub fn save_book(
        &mut self,
        book: &CanonicalBook,
        render_mode: RenderMode,
        now: i64,
    ) -> Result<String> {
        let book_id = self
            .renamed_book_id(book)?
            .unwrap_or_else(|| book.id.clone());
        let transaction = self.connection.transaction()?;
        transaction.prepare_cached(
            "INSERT INTO books (id, title, author, source_path, import_hash, parser_version, last_opened_at, render_mode)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
               title = excluded.title,
               author = excluded.author,
               source_path = excluded.source_path,
               import_hash = excluded.import_hash,
               parser_version = excluded.parser_version,
               last_opened_at = excluded.last_opened_at,
               render_mode = excluded.render_mode",
        )?.execute(params![
                book_id,
                book.title,
                book.author,
                book.source_path,
                book.import_hash,
                book.parser_version.unwrap_or(CURRENT_EPUB_PARSER_VERSION),
                now,
                render_mode.as_str(),
            ])?;
        transaction
            .prepare_cached("DELETE FROM chapters WHERE book_id = ?")?
            .execute(params![book_id])?;
        transaction
            .prepare_cached("DELETE FROM diagnostics WHERE book_id = ?")?
            .execute(params![book_id])?;

        {
            let mut insert_chapter = transaction.prepare_cached(
                "INSERT INTO chapters (id, book_id, chapter_index, title, href, depth, word_count, blocks_json)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )?;
            for chapter in &book.chapters {
                let blocks = serde_json::to_string(&chapter.blocks).map_err(|error| {
                    StorageError::CorruptBlocks {
                        chapter_id: chapter.id.clone(),
                        detail: error.to_string(),
                    }
                })?;
                insert_chapter.execute(params![
                    chapter.id,
                    book_id,
                    from_index(chapter.index),
                    chapter.title,
                    chapter.href,
                    from_index(chapter.depth),
                    from_index(chapter.word_count),
                    blocks,
                ])?;
            }
        }
        {
            let mut insert_diagnostic = transaction.prepare_cached(
                "INSERT INTO diagnostics (book_id, severity, message, context) VALUES (?, ?, ?, ?)",
            )?;
            for diagnostic in &book.diagnostics {
                insert_diagnostic.execute(params![
                    book_id,
                    severity_name(diagnostic.severity),
                    diagnostic.message,
                    diagnostic.context,
                ])?;
            }
        }
        transaction.commit()?;
        Ok(book_id)
    }

    /// Load a stored book, or `None` when it is not in the library.
    pub fn book(&self, book_id: &str) -> Result<Option<CanonicalBook>> {
        let header = self
            .connection
            .prepare_cached(
                "SELECT id, title, author, source_path, import_hash, parser_version
                 FROM books WHERE id = ?",
            )?
            .query_row(params![book_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<u32>>(5)?,
                ))
            })
            .optional()?;
        let Some((id, title, author, source_path, import_hash, parser_version)) = header else {
            return Ok(None);
        };

        let chapters = {
            let mut statement = self.connection.prepare_cached(
                "SELECT id, chapter_index, title, href, depth, word_count, blocks_json
                 FROM chapters WHERE book_id = ? ORDER BY chapter_index ASC",
            )?;
            let mut rows = statement.query(params![book_id])?;
            let mut chapters = Vec::new();
            while let Some(row) = rows.next()? {
                let id: String = row.get(0)?;
                let blocks_json: String = row.get(6)?;
                let blocks = serde_json::from_str(&blocks_json).map_err(|error| {
                    StorageError::CorruptBlocks {
                        chapter_id: id.clone(),
                        detail: error.to_string(),
                    }
                })?;
                chapters.push(CanonicalChapter {
                    id,
                    index: to_index(row.get(1)?),
                    title: row.get(2)?,
                    href: row.get(3)?,
                    depth: to_index(row.get(4)?),
                    word_count: to_index(row.get(5)?),
                    blocks,
                });
            }
            chapters
        };

        let diagnostics = self
            .connection
            .prepare_cached("SELECT severity, message, context FROM diagnostics WHERE book_id = ?")?
            .query_map(params![book_id], |row| {
                Ok(ImportDiagnostic {
                    severity: parse_severity(&row.get::<_, String>(0)?),
                    message: row.get(1)?,
                    context: row.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(Some(CanonicalBook {
            id,
            title,
            author,
            source_path,
            import_hash,
            parser_version,
            diagnostics,
            chapters,
            cover_path: None,
        }))
    }

    /// Every book, most recently opened first.
    pub fn books(&self) -> Result<Vec<LibraryEntry>> {
        Ok(self
            .connection
            .prepare(
                "SELECT id, title, author, source_path, import_hash, parser_version, last_opened_at, render_mode
                 FROM books ORDER BY last_opened_at DESC",
            )?
            .query_map([], |row| {
                Ok(LibraryEntry {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    author: row.get(2)?,
                    source_path: row.get(3)?,
                    import_hash: row.get(4)?,
                    parser_version: row.get(5)?,
                    last_opened_at: row.get(6)?,
                    render_mode: RenderMode::from_id(&row.get::<_, String>(7)?)
                        .unwrap_or(RenderMode::Code),
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// Every book with its progress, sorted, and optionally filtered by tag.
    pub fn books_with_progress(
        &self,
        sort: LibrarySortKey,
        direction: SortDirection,
        tag_filter: Option<&str>,
    ) -> Result<Vec<LibraryEntryWithProgress>> {
        let mut entries: Vec<LibraryEntryWithProgress> = self
            .connection
            .prepare(
                "SELECT b.id, b.title, b.author, b.source_path, b.import_hash, b.parser_version,
                        b.last_opened_at, b.render_mode,
                        p.chapter_index, p.book_progress, c.title
                 FROM books b
                 LEFT JOIN positions p ON p.book_id = b.id
                 LEFT JOIN chapters c ON c.book_id = b.id AND c.chapter_index = p.chapter_index",
            )?
            .query_map([], |row| {
                Ok(LibraryEntryWithProgress {
                    entry: LibraryEntry {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        author: row.get(2)?,
                        source_path: row.get(3)?,
                        import_hash: row.get(4)?,
                        parser_version: row.get(5)?,
                        last_opened_at: row.get(6)?,
                        render_mode: RenderMode::from_id(&row.get::<_, String>(7)?)
                            .unwrap_or(RenderMode::Code),
                    },
                    chapter_index: row.get::<_, Option<i64>>(8)?.map(to_index),
                    book_progress: row.get(9)?,
                    chapter_title: row.get(10)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;

        if let Some(tag) = tag_filter {
            let tagged: std::collections::HashSet<String> = self
                .connection
                .prepare("SELECT book_id FROM book_tags WHERE LOWER(tag) = LOWER(?)")?
                .query_map(params![tag], |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            entries.retain(|item| tagged.contains(&item.entry.id));
        }
        reader_core::library::sort_library(&mut entries, sort, direction);
        Ok(entries)
    }

    /// Remove a book and everything attached to it, including its cache.
    pub fn remove_book(&mut self, book_id: &str) -> Result<()> {
        let transaction = self.connection.transaction()?;
        // reading_pace references books, so it goes first.
        for statement in [
            "DELETE FROM reading_pace WHERE book_id = ?",
            "DELETE FROM books WHERE id = ?",
            "DELETE FROM chapters WHERE book_id = ?",
            "DELETE FROM diagnostics WHERE book_id = ?",
            "DELETE FROM positions WHERE book_id = ?",
            "DELETE FROM bookmarks WHERE book_id = ?",
            "DELETE FROM book_tags WHERE book_id = ?",
            "DELETE FROM notes WHERE book_id = ?",
        ] {
            transaction.execute(statement, params![book_id])?;
        }
        transaction.commit()?;
        std::fs::remove_dir_all(self.chapter_cache_dir.join(book_id)).ok();
        Ok(())
    }

    /// The most recently opened book, for `--resume`.
    pub fn latest_book_id(&self) -> Result<Option<String>> {
        Ok(self
            .connection
            .query_row(
                "SELECT id FROM books ORDER BY last_opened_at DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// Whether a stored EPUB was extracted by an older parser and should be
    /// re-imported.
    pub fn needs_epub_reimport(&self, book_id: &str) -> Result<bool> {
        let row: Option<(Option<u32>, String)> = self
            .connection
            .query_row(
                "SELECT parser_version, source_path FROM books WHERE id = ?",
                params![book_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        Ok(match row {
            Some((parser_version, source_path)) => {
                source_path.to_lowercase().ends_with(".epub")
                    && parser_version.unwrap_or(1) < CURRENT_EPUB_PARSER_VERSION
            }
            None => false,
        })
    }

    // ── positions and pace ───────────────────────────────────────────────────

    /// Save the reading position and mark the book as opened now.
    pub fn save_position(&self, book_id: &str, position: ReadingPosition, now: i64) -> Result<()> {
        // `unchecked_transaction` only relaxes Rust's exclusive-borrow check;
        // SQLite still rejects accidental nesting. This method is a public,
        // top-level write and keeping both rows atomic is worth that tradeoff.
        let transaction = self.connection.unchecked_transaction()?;
        transaction.prepare_cached(
            "INSERT INTO positions (book_id, chapter_index, chapter_progress, book_progress, block_offset)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(book_id) DO UPDATE SET
               chapter_index = excluded.chapter_index,
               chapter_progress = excluded.chapter_progress,
               book_progress = excluded.book_progress,
               block_offset = excluded.block_offset",
        )?.execute(params![
                book_id,
                from_index(position.chapter_index),
                position.chapter_progress,
                position.book_progress,
                from_index(position.block_offset),
            ])?;
        transaction
            .prepare_cached("UPDATE books SET last_opened_at = ? WHERE id = ?")?
            .execute(params![now, book_id])?;
        transaction.commit()?;
        Ok(())
    }

    /// The saved position for a book, if any.
    pub fn position(&self, book_id: &str) -> Result<Option<ReadingPosition>> {
        Ok(self
            .connection
            .query_row(
                "SELECT chapter_index, chapter_progress, book_progress, block_offset
                 FROM positions WHERE book_id = ?",
                params![book_id],
                |row| {
                    Ok(ReadingPosition {
                        chapter_index: to_index(row.get(0)?),
                        chapter_progress: row.get(1)?,
                        book_progress: row.get(2)?,
                        block_offset: to_index(row.get(3)?),
                    })
                },
            )
            .optional()?)
    }

    /// The learned pace for one book.
    pub fn reading_pace(&self, book_id: &str) -> Result<Option<BookReadingPace>> {
        Ok(self
            .connection
            .query_row(
                "SELECT wpm, active_ms, updated_at FROM reading_pace WHERE book_id = ?",
                params![book_id],
                |row| {
                    Ok(BookReadingPace {
                        wpm: row.get(0)?,
                        active_ms: row.get::<_, i64>(1)? as f64,
                        updated_at: row.get(2)?,
                    })
                },
            )
            .optional()?)
    }

    /// Store the learned pace for one book.
    pub fn save_reading_pace(&self, book_id: &str, pace: BookReadingPace) -> Result<()> {
        self.connection.execute(
            "INSERT INTO reading_pace (book_id, wpm, active_ms, updated_at)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(book_id) DO UPDATE SET
               wpm = excluded.wpm,
               active_ms = excluded.active_ms,
               updated_at = excluded.updated_at",
            params![book_id, pace.wpm, pace.active_ms as i64, pace.updated_at],
        )?;
        Ok(())
    }

    // ── command history ──────────────────────────────────────────────────────

    /// Append a command to the history, with secrets redacted.
    pub fn save_command_history(
        &self,
        raw_command: &str,
        normalized_name: &str,
        now: i64,
    ) -> Result<()> {
        self.connection.execute(
            "INSERT INTO command_history (raw_command, normalized_name, created_at) VALUES (?, ?, ?)",
            params![
                redact_sensitive_command(raw_command, normalized_name),
                normalized_name,
                now
            ],
        )?;
        Ok(())
    }

    /// Recent commands, most recent first.
    pub fn command_history(&self, limit: usize) -> Result<Vec<String>> {
        Ok(self
            .connection
            .prepare("SELECT raw_command FROM command_history ORDER BY id DESC LIMIT ?")?
            .query_map(params![from_index(limit)], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    }

    // ── bookmarks ────────────────────────────────────────────────────────────

    /// Create a bookmark at a position. An all-whitespace label is stored as none.
    pub fn add_bookmark(
        &self,
        book_id: &str,
        chapter_index: usize,
        block_offset: usize,
        label: Option<&str>,
        now: i64,
    ) -> Result<Bookmark> {
        let bookmark = Bookmark {
            id: uuid::Uuid::new_v4().to_string(),
            book_id: book_id.to_owned(),
            chapter_index,
            block_offset,
            label: label
                .map(str::trim)
                .filter(|label| !label.is_empty())
                .map(str::to_owned),
            created_at: now,
        };
        self.connection.execute(
            "INSERT INTO bookmarks (id, book_id, chapter_index, block_offset, label, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
            params![
                bookmark.id,
                bookmark.book_id,
                from_index(bookmark.chapter_index),
                from_index(bookmark.block_offset),
                bookmark.label,
                bookmark.created_at,
            ],
        )?;
        Ok(bookmark)
    }

    /// Bookmarks for a book, newest first.
    pub fn bookmarks(&self, book_id: &str) -> Result<Vec<Bookmark>> {
        Ok(self
            .connection
            .prepare(
                "SELECT id, book_id, chapter_index, block_offset, label, created_at
                 FROM bookmarks WHERE book_id = ? ORDER BY created_at DESC",
            )?
            .query_map(params![book_id], |row| {
                Ok(Bookmark {
                    id: row.get(0)?,
                    book_id: row.get(1)?,
                    chapter_index: to_index(row.get(2)?),
                    block_offset: to_index(row.get(3)?),
                    label: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// Delete one bookmark.
    pub fn delete_bookmark(&self, id: &str) -> Result<()> {
        self.connection
            .execute("DELETE FROM bookmarks WHERE id = ?", params![id])?;
        Ok(())
    }

    // ── tags ─────────────────────────────────────────────────────────────────

    /// Attach a tag to a book. Re-tagging is a no-op.
    pub fn add_tag(&self, book_id: &str, tag: &str) -> Result<()> {
        self.connection.execute(
            "INSERT OR IGNORE INTO book_tags (book_id, tag) VALUES (?, ?)",
            params![book_id, tag.trim()],
        )?;
        Ok(())
    }

    /// Remove a tag from a book.
    pub fn remove_tag(&self, book_id: &str, tag: &str) -> Result<()> {
        self.connection.execute(
            "DELETE FROM book_tags WHERE book_id = ? AND tag = ?",
            params![book_id, tag.trim()],
        )?;
        Ok(())
    }

    /// Tags of one book, alphabetically.
    pub fn tags(&self, book_id: &str) -> Result<Vec<String>> {
        Ok(self
            .connection
            .prepare("SELECT tag FROM book_tags WHERE book_id = ? ORDER BY tag")?
            .query_map(params![book_id], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// Tags of every book, keyed by book id.
    pub fn tags_by_book(&self) -> Result<std::collections::BTreeMap<String, Vec<String>>> {
        let mut map: std::collections::BTreeMap<String, Vec<String>> =
            std::collections::BTreeMap::new();
        let mut statement = self
            .connection
            .prepare_cached("SELECT book_id, tag FROM book_tags ORDER BY book_id, tag")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let book_id: String = row.get(0)?;
            let tag: String = row.get(1)?;
            map.entry(book_id).or_default().push(tag);
        }
        Ok(map)
    }

    // ── notes ────────────────────────────────────────────────────────────────

    /// Create a note at a position.
    pub fn add_note(
        &self,
        book_id: &str,
        content: &str,
        chapter_index: Option<usize>,
        block_offset: Option<usize>,
        now: i64,
    ) -> Result<Note> {
        let note = Note {
            id: uuid::Uuid::new_v4().to_string(),
            book_id: book_id.to_owned(),
            chapter_index,
            block_offset,
            content: content.trim().to_owned(),
            created_at: now,
        };
        self.connection.execute(
            "INSERT INTO notes (id, book_id, chapter_index, block_offset, content, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
            params![
                note.id,
                note.book_id,
                note.chapter_index.map(from_index),
                note.block_offset.map(from_index),
                note.content,
                note.created_at,
            ],
        )?;
        Ok(note)
    }

    /// Notes for a book, newest first, with insertion order breaking ties.
    pub fn notes(&self, book_id: &str) -> Result<Vec<Note>> {
        Ok(self
            .connection
            .prepare(
                "SELECT id, book_id, chapter_index, block_offset, content, created_at
                 FROM notes WHERE book_id = ? ORDER BY created_at DESC, rowid DESC",
            )?
            .query_map(params![book_id], |row| {
                Ok(Note {
                    id: row.get(0)?,
                    book_id: row.get(1)?,
                    chapter_index: row.get::<_, Option<i64>>(2)?.map(to_index),
                    block_offset: row.get::<_, Option<i64>>(3)?.map(to_index),
                    content: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// Delete one note.
    pub fn delete_note(&self, id: &str) -> Result<()> {
        self.connection
            .execute("DELETE FROM notes WHERE id = ?", params![id])?;
        Ok(())
    }
}

/// SQLite stores integers as signed 64-bit, so counts and offsets convert on the
/// way in and out. A negative stored value would mean a corrupt row; clamping to
/// zero keeps the reader usable instead of panicking on it.
pub(crate) const fn to_index(value: i64) -> usize {
    if value < 0 { 0 } else { value as usize }
}

pub(crate) fn from_index(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

const fn severity_name(severity: DiagnosticSeverity) -> &'static str {
    match severity {
        DiagnosticSeverity::Warning => "warning",
        DiagnosticSeverity::Error => "error",
    }
}

fn parse_severity(value: &str) -> DiagnosticSeverity {
    if value == "error" {
        DiagnosticSeverity::Error
    } else {
        DiagnosticSeverity::Warning
    }
}

#[cfg(test)]
mod tests {
    use reader_core::{
        AppSettings, CanonicalBlock, CanonicalBook, CanonicalChapter, DiagnosticSeverity,
        ImportDiagnostic, LibrarySortKey, LineSpacing, ReadingPosition, RenderMode, SortDirection,
    };

    use super::{Storage, redact_sensitive_command};

    fn storage() -> Storage {
        Storage::open_in_memory().expect("in-memory database should open")
    }

    fn book(id: &str, hash: &str) -> CanonicalBook {
        CanonicalBook {
            id: id.to_owned(),
            title: format!("Title {id}"),
            author: "Author".to_owned(),
            source_path: format!("/books/{id}.epub"),
            import_hash: hash.to_owned(),
            parser_version: Some(3),
            diagnostics: vec![ImportDiagnostic {
                severity: DiagnosticSeverity::Warning,
                message: "a warning".to_owned(),
                context: Some("navigation".to_owned()),
            }],
            chapters: vec![
                CanonicalChapter {
                    id: format!("{id}-ch1"),
                    index: 0,
                    title: "One".to_owned(),
                    href: "text/ch1.xhtml".to_owned(),
                    depth: 0,
                    blocks: vec![CanonicalBlock::Paragraph {
                        id: "b1".to_owned(),
                        text: "First chapter text".to_owned(),
                    }],
                    word_count: 3,
                },
                CanonicalChapter {
                    id: format!("{id}-ch2"),
                    index: 1,
                    title: "Two".to_owned(),
                    href: "text/ch2.xhtml".to_owned(),
                    depth: 1,
                    blocks: vec![CanonicalBlock::Heading {
                        id: "b2".to_owned(),
                        text: "Two".to_owned(),
                        level: Some(2),
                    }],
                    word_count: 1,
                },
            ],
            cover_path: None,
        }
    }

    #[test]
    fn a_fresh_database_seeds_the_default_settings() {
        let storage = storage();
        assert_eq!(
            storage.settings().expect("settings should load"),
            AppSettings::default()
        );
        assert_eq!(
            storage.setting("renderMode").expect("row"),
            Some("code".to_owned())
        );
    }

    #[test]
    fn settings_persist_and_reject_invalid_values() {
        let mut storage = storage();
        let settings = AppSettings {
            render_mode: RenderMode::Plain,
            font_scale: 1.3,
            margin_size: 12,
            line_spacing: LineSpacing::Relaxed,
            mouse_capture: true,
            ..AppSettings::default()
        };
        storage.save_settings(&settings).expect("save");

        assert_eq!(storage.settings().expect("load"), settings);

        // Values written outside the allow-list fall back to the default.
        storage.set_setting("fontScale", "1.7").expect("write");
        storage.set_setting("marginSize", "30").expect("write");
        storage.set_setting("lineSpacing", "huge").expect("write");
        let fallback = storage.settings().expect("load");
        assert!((fallback.font_scale - 1.0).abs() < f64::EPSILON);
        assert_eq!(fallback.margin_size, 0);
        assert_eq!(fallback.line_spacing, LineSpacing::Normal);
    }

    #[test]
    fn a_failed_settings_write_rolls_back_every_preference() {
        let mut storage = storage();
        let before = storage.settings().expect("load");
        storage
            .connection
            .execute_batch(
                "CREATE TRIGGER reject_margin BEFORE UPDATE ON settings
                 WHEN NEW.key = 'marginSize' AND NEW.value = '24'
                 BEGIN SELECT RAISE(ABORT, 'rejected setting'); END;",
            )
            .expect("trigger");

        let mut settings = before;
        settings.margin_size = 24;
        settings.font_scale = 1.5;
        let error = storage
            .save_settings(&settings)
            .expect_err("write is rejected");
        assert!(error.to_string().contains("rejected setting"));

        assert_eq!(storage.settings().expect("load"), before);
    }

    #[test]
    fn raw_settings_round_trip_for_keys_outside_the_preference_set() {
        let storage = storage();
        storage.set_setting("globalWpm", "215.25").expect("write");
        assert_eq!(
            storage.setting("globalWpm").expect("read"),
            Some("215.25".to_owned())
        );
        storage.delete_setting("globalWpm").expect("delete");
        assert_eq!(storage.setting("globalWpm").expect("read"), None);
        assert_eq!(storage.setting("neverSet").expect("read"), None);
    }

    #[test]
    fn a_book_round_trips_with_its_chapters_and_diagnostics() {
        let mut storage = storage();
        let original = book("b1", "hash-1");
        let stored_id = storage
            .save_book(&original, RenderMode::Plain, 1_000)
            .expect("save");
        assert_eq!(stored_id, "b1");

        let loaded = storage.book("b1").expect("load").expect("book exists");
        assert_eq!(loaded.title, original.title);
        assert_eq!(loaded.import_hash, original.import_hash);
        assert_eq!(loaded.parser_version, Some(3));
        assert_eq!(loaded.chapters.len(), 2);
        assert_eq!(loaded.chapters[0].blocks, original.chapters[0].blocks);
        assert_eq!(loaded.chapters[1].depth, 1);
        assert_eq!(loaded.diagnostics, original.diagnostics);
        assert!(storage.book("missing").expect("load").is_none());
    }

    #[test]
    fn two_books_can_share_chapter_ids() {
        // v1 keyed `chapters` globally while deriving the id from the href and
        // index alone, so importing a second book whose chapters happen to share
        // an href failed on a constraint violation. Both must now store.
        let mut storage = storage();
        let first = book("b1", "hash-1");
        let mut second = book("b2", "hash-2");
        second.chapters = first.chapters.clone();

        storage
            .save_book(&first, RenderMode::Code, 1_000)
            .expect("the first book saves");
        storage
            .save_book(&second, RenderMode::Code, 2_000)
            .expect("a book sharing chapter ids must also save");

        assert_eq!(
            storage
                .book("b1")
                .expect("load")
                .expect("book")
                .chapters
                .len(),
            2
        );
        assert_eq!(
            storage
                .book("b2")
                .expect("load")
                .expect("book")
                .chapters
                .len(),
            2
        );
    }

    #[test]
    fn saving_a_book_again_replaces_its_chapters() {
        let mut storage = storage();
        let mut original = book("b1", "hash-1");
        storage
            .save_book(&original, RenderMode::Code, 1_000)
            .expect("save");
        original.chapters.truncate(1);
        original.title = "Renamed".to_owned();
        storage
            .save_book(&original, RenderMode::Code, 2_000)
            .expect("save again");

        let loaded = storage.book("b1").expect("load").expect("book exists");
        assert_eq!(loaded.chapters.len(), 1);
        assert_eq!(loaded.title, "Renamed");
        assert_eq!(
            loaded.diagnostics.len(),
            1,
            "diagnostics are not duplicated"
        );
    }

    #[test]
    fn a_moved_file_reuses_the_id_of_the_book_it_replaces() {
        let mut storage = storage();
        // The stored book points at a path that does not exist.
        storage
            .save_book(&book("old-id", "same-hash"), RenderMode::Code, 1_000)
            .expect("save");
        storage
            .save_position(
                "old-id",
                ReadingPosition {
                    chapter_index: 1,
                    ..ReadingPosition::start()
                },
                1_000,
            )
            .expect("position");

        let mut moved = book("new-id", "same-hash");
        moved.source_path = "/books/moved.epub".to_owned();
        let stored_id = storage
            .save_book(&moved, RenderMode::Code, 2_000)
            .expect("save");

        assert_eq!(
            stored_id, "old-id",
            "the reading position must survive a move"
        );
        assert_eq!(
            storage
                .position("old-id")
                .expect("load")
                .map(|position| position.chapter_index),
            Some(1)
        );
        assert!(storage.book("new-id").expect("load").is_none());
    }

    #[test]
    fn a_book_whose_file_still_exists_is_left_alone() {
        let mut storage = storage();
        let mut existing = book("old-id", "same-hash");
        // Point at a path that really exists so the rename heuristic declines.
        existing.source_path = std::env::current_dir().expect("cwd").display().to_string();
        storage
            .save_book(&existing, RenderMode::Code, 1_000)
            .expect("save");

        let stored_id = storage
            .save_book(&book("new-id", "same-hash"), RenderMode::Code, 2_000)
            .expect("save");
        assert_eq!(stored_id, "new-id");
    }

    #[test]
    fn positions_round_trip_and_touch_the_last_opened_time() {
        let mut storage = storage();
        storage
            .save_book(&book("b1", "hash-1"), RenderMode::Code, 1_000)
            .expect("save");
        let position = ReadingPosition {
            chapter_index: 7,
            chapter_progress: 0.6,
            book_progress: 0.45,
            block_offset: 33,
        };
        storage.save_position("b1", position, 5_000).expect("save");

        let loaded = storage
            .position("b1")
            .expect("load")
            .expect("position exists");
        assert_eq!(loaded.chapter_index, 7);
        assert_eq!(loaded.block_offset, 33);
        assert!((loaded.book_progress - 0.45).abs() < 1e-9);
        assert_eq!(storage.books().expect("list")[0].last_opened_at, 5_000);
        assert!(storage.position("missing").expect("load").is_none());
    }

    #[test]
    fn removing_a_book_cascades_to_everything_attached_to_it() {
        let mut storage = storage();
        storage
            .save_book(&book("b1", "hash-1"), RenderMode::Code, 1_000)
            .expect("save");
        storage
            .save_position("b1", ReadingPosition::start(), 1_000)
            .expect("position");
        storage
            .add_bookmark("b1", 0, 0, Some("mark"), 1_000)
            .expect("bookmark");
        storage.add_tag("b1", "favorite").expect("tag");
        storage
            .add_note("b1", "a note", Some(0), Some(0), 1_000)
            .expect("note");
        storage
            .save_reading_pace(
                "b1",
                reader_core::BookReadingPace {
                    wpm: 210.5,
                    active_ms: 90_000.0,
                    updated_at: 1_000,
                },
            )
            .expect("pace");

        storage.remove_book("b1").expect("remove");

        assert!(storage.book("b1").expect("load").is_none());
        assert!(storage.position("b1").expect("load").is_none());
        assert!(storage.bookmarks("b1").expect("list").is_empty());
        assert!(storage.tags("b1").expect("list").is_empty());
        assert!(storage.notes("b1").expect("list").is_empty());
        assert!(storage.reading_pace("b1").expect("load").is_none());
        assert!(storage.books().expect("list").is_empty());
    }

    #[test]
    fn reading_pace_upserts_per_book() {
        let mut storage = storage();
        storage
            .save_book(&book("b1", "hash-1"), RenderMode::Code, 1_000)
            .expect("save");
        assert!(storage.reading_pace("b1").expect("load").is_none());

        for wpm in [210.5, 240.0] {
            storage
                .save_reading_pace(
                    "b1",
                    reader_core::BookReadingPace {
                        wpm,
                        active_ms: 90_000.0,
                        updated_at: 1_700_000_000_000,
                    },
                )
                .expect("pace");
        }
        let pace = storage
            .reading_pace("b1")
            .expect("load")
            .expect("pace exists");
        assert!((pace.wpm - 240.0).abs() < 1e-9);
        assert!((pace.active_ms - 90_000.0).abs() < 1e-9);
    }

    #[test]
    fn bookmarks_are_listed_newest_first_and_can_be_deleted() {
        let storage = storage();
        let first = storage
            .add_bookmark("b1", 2, 42, Some("Trecho"), 1_000)
            .expect("bookmark");
        let second = storage
            .add_bookmark("b1", 3, 10, None, 2_000)
            .expect("bookmark");
        let blank_label = storage
            .add_bookmark("b1", 4, 0, Some("   "), 3_000)
            .expect("bookmark");

        assert_eq!(first.label.as_deref(), Some("Trecho"));
        assert_eq!(second.label, None);
        assert_eq!(blank_label.label, None, "a blank label is stored as none");

        let listed = storage.bookmarks("b1").expect("list");
        assert_eq!(
            listed
                .iter()
                .map(|item| item.created_at)
                .collect::<Vec<_>>(),
            vec![3_000, 2_000, 1_000]
        );

        storage.delete_bookmark(&first.id).expect("delete");
        assert_eq!(storage.bookmarks("b1").expect("list").len(), 2);
    }

    #[test]
    fn tags_are_unique_per_book_and_listed_alphabetically() {
        let storage = storage();
        storage.add_tag("b1", "sci-fi").expect("tag");
        storage.add_tag("b1", "  favorite  ").expect("tag");
        storage.add_tag("b1", "favorite").expect("tag again");
        storage.add_tag("b2", "classic").expect("tag");

        assert_eq!(
            storage.tags("b1").expect("list"),
            vec!["favorite", "sci-fi"]
        );
        let by_book = storage.tags_by_book().expect("map");
        assert_eq!(by_book["b1"], vec!["favorite", "sci-fi"]);
        assert_eq!(by_book["b2"], vec!["classic"]);

        storage.remove_tag("b1", "favorite").expect("untag");
        assert_eq!(storage.tags("b1").expect("list"), vec!["sci-fi"]);
    }

    #[test]
    fn notes_trim_content_and_list_newest_first() {
        let storage = storage();
        storage
            .add_note("b1", "  first note  ", Some(1), Some(2), 1_000)
            .expect("note");
        let second = storage
            .add_note("b1", "second note", None, None, 1_000)
            .expect("note");

        let listed = storage.notes("b1").expect("list");
        assert_eq!(listed.len(), 2);
        // Equal timestamps fall back to insertion order, newest first.
        assert_eq!(listed[0].id, second.id);
        assert_eq!(listed[1].content, "first note");
        assert_eq!(listed[0].chapter_index, None);

        storage.delete_note(&second.id).expect("delete");
        assert_eq!(storage.notes("b1").expect("list").len(), 1);
    }

    #[test]
    fn command_history_redacts_toggl_keys_on_write() {
        let storage = storage();
        storage
            .save_command_history("/toggl auth super-secret-token", "toggl", 1_000)
            .expect("write");
        let history = storage.command_history(10).expect("read");
        assert_eq!(history, vec!["/toggl auth <redacted>"]);
    }

    #[test]
    fn redaction_only_touches_toggl_authentication() {
        assert_eq!(
            redact_sensitive_command("/toggl auth super-secret", "toggl"),
            "/toggl auth <redacted>"
        );
        assert_eq!(
            redact_sensitive_command("toggl auth super-secret", "toggl"),
            "toggl auth <redacted>"
        );
        assert_eq!(
            redact_sensitive_command("  /toggl auth secret", "toggl"),
            "  /toggl auth <redacted>"
        );
        assert_eq!(
            redact_sensitive_command("/TOGGL AUTH secret", "TOGGL"),
            "/TOGGL AUTH <redacted>"
        );
        // Nothing to hide in these.
        for command in [
            "/toggl auth",
            "/toggl auth <redacted>",
            "/toggl auth --open",
            "/toggl sync",
            "/search auth secret",
        ] {
            assert_eq!(
                redact_sensitive_command(command, "toggl"),
                command,
                "{command:?} should be stored as typed"
            );
        }
        assert_eq!(
            redact_sensitive_command("/search auth secret", "search"),
            "/search auth secret"
        );
    }

    #[test]
    fn reopening_a_database_redacts_keys_written_by_older_builds() {
        let directory = std::env::temp_dir().join(format!(
            "reader-storage-redact-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::remove_dir_all(&directory).ok();
        let paths = super::AppPaths::from_roots(&directory.join("data"), &directory.join("cache"));

        {
            let storage = Storage::open(&paths).expect("open");
            storage
                .connection
                .execute(
                    "INSERT INTO command_history (raw_command, normalized_name, created_at)
                     VALUES ('/toggl auth legacy-secret-token', 'toggl', 1000)",
                    [],
                )
                .expect("legacy row");
        }

        let reopened = Storage::open(&paths).expect("reopen");
        assert_eq!(
            reopened.command_history(10).expect("read"),
            vec!["/toggl auth <redacted>"]
        );
        std::fs::remove_dir_all(directory).ok();
    }

    #[test]
    fn the_library_list_sorts_filters_and_reports_progress() {
        let mut storage = storage();
        storage
            .save_book(&book("b1", "hash-1"), RenderMode::Code, 1_000)
            .expect("save");
        storage
            .save_book(&book("b2", "hash-2"), RenderMode::Plain, 2_000)
            .expect("save");
        storage
            .save_position(
                "b1",
                ReadingPosition {
                    chapter_index: 1,
                    chapter_progress: 0.5,
                    book_progress: 0.75,
                    block_offset: 4,
                },
                3_000,
            )
            .expect("position");
        storage.add_tag("b2", "favorite").expect("tag");

        let by_progress = storage
            .books_with_progress(LibrarySortKey::Progress, SortDirection::Descending, None)
            .expect("list");
        assert_eq!(by_progress.len(), 2);
        assert_eq!(by_progress[0].entry.id, "b1");
        assert_eq!(by_progress[0].chapter_title.as_deref(), Some("Two"));
        assert!((by_progress[0].book_progress.expect("progress") - 0.75).abs() < 1e-9);
        assert!(by_progress[1].book_progress.is_none());

        let filtered = storage
            .books_with_progress(
                LibrarySortKey::Title,
                SortDirection::Ascending,
                Some("FAVORITE"),
            )
            .expect("list");
        assert_eq!(
            filtered
                .iter()
                .map(|item| item.entry.id.as_str())
                .collect::<Vec<_>>(),
            vec!["b2"]
        );
    }

    #[test]
    fn the_latest_book_and_reimport_check_follow_the_stored_metadata() {
        let mut storage = storage();
        assert!(storage.latest_book_id().expect("query").is_none());

        storage
            .save_book(&book("b1", "hash-1"), RenderMode::Code, 1_000)
            .expect("save");
        let mut stale = book("b2", "hash-2");
        stale.parser_version = Some(1);
        storage
            .save_book(&stale, RenderMode::Code, 2_000)
            .expect("save");

        assert_eq!(
            storage.latest_book_id().expect("query"),
            Some("b2".to_owned())
        );
        assert!(storage.needs_epub_reimport("b2").expect("check"));
        assert!(!storage.needs_epub_reimport("b1").expect("check"));
        assert!(!storage.needs_epub_reimport("missing").expect("check"));
    }
}

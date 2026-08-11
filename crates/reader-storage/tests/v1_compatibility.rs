//! Opening a real v1 database.
//!
//! `tests/fixtures/v1-library.db` was written by the TypeScript implementation
//! (see `tools/generate-storage-fixture.mjs`), and `v1-library.json` records
//! what v1 read back from it. This suite proves the Rust storage layer opens
//! that file, reads every table the same way, and applies the token redaction
//! migration to history rows written before it existed.
//!
//! The fixture is copied to a scratch directory first, because opening it
//! upgrades the journal and rewrites redacted rows.

use std::path::{Path, PathBuf};

use reader_core::{AppSettings, LibrarySortKey, RenderMode, SortDirection};
use reader_storage::{AppPaths, Storage};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Expected {
    settings: ExpectedSettings,
    raw_settings: RawSettings,
    books: Vec<ExpectedBook>,
    position: ExpectedPosition,
    bookmarks: Vec<ExpectedBookmark>,
    notes: Vec<ExpectedNote>,
    tags: Vec<String>,
    reading_pace: ExpectedPace,
    book: ExpectedCanonicalBook,
    needs_reimport: std::collections::BTreeMap<String, bool>,
    latest_book_id: String,
    export_all: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExpectedSettings {
    theme_id: String,
    appearance_theme_id: String,
    progress_visibility: String,
    render_mode: String,
    code_language: String,
    code_density: u8,
    plain_highlight: bool,
    font_scale: f64,
    margin_size: u16,
    line_spacing: String,
    mouse_capture: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSettings {
    global_wpm: String,
    global_active_ms: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExpectedBook {
    id: String,
    title: String,
    author: String,
    source_path: String,
    import_hash: String,
    parser_version: Option<u32>,
    last_opened_at: i64,
    render_mode: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExpectedPosition {
    chapter_index: usize,
    chapter_progress: f64,
    book_progress: f64,
    block_offset: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExpectedBookmark {
    chapter_index: usize,
    block_offset: usize,
    label: Option<String>,
    created_at: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExpectedNote {
    chapter_index: Option<usize>,
    block_offset: Option<usize>,
    content: String,
    created_at: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExpectedPace {
    wpm: f64,
    active_ms: f64,
    updated_at: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExpectedCanonicalBook {
    title: String,
    author: String,
    chapters: Vec<ExpectedChapter>,
    diagnostics: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExpectedChapter {
    id: String,
    index: usize,
    title: String,
    href: String,
    depth: usize,
    word_count: usize,
    blocks: Vec<serde_json::Value>,
}

/// Copy the fixture database into a fresh directory and open it.
fn open_fixture(name: &str) -> (Storage, PathBuf) {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("v1-library.db");
    let scratch =
        std::env::temp_dir().join(format!("reader-storage-v1-{}-{name}", std::process::id()));
    std::fs::remove_dir_all(&scratch).ok();
    let paths = AppPaths::from_roots(&scratch.join("data"), &scratch.join("cache"));
    paths.ensure().expect("scratch directories");
    std::fs::copy(&fixture, &paths.db_path).expect("fixture should be copyable");

    let storage = Storage::open(&paths).expect("a v1 database should open");
    (storage, scratch)
}

fn expected() -> Expected {
    serde_json::from_str(include_str!("fixtures/v1-library.json"))
        .expect("fixture description should parse")
}

#[test]
fn settings_written_by_v1_are_read_back_unchanged() {
    let (storage, scratch) = open_fixture("settings");
    let expected = expected();

    let settings = storage.settings().expect("settings should load");
    assert_eq!(settings.theme_id.as_str(), expected.settings.theme_id);
    assert_eq!(
        settings.appearance_theme_id.as_str(),
        expected.settings.appearance_theme_id
    );
    assert_eq!(
        settings.progress_visibility.as_str(),
        expected.settings.progress_visibility
    );
    assert_eq!(settings.render_mode.as_str(), expected.settings.render_mode);
    assert_eq!(
        settings.code_language.as_str(),
        expected.settings.code_language
    );
    assert_eq!(settings.code_density.get(), expected.settings.code_density);
    assert_eq!(settings.plain_highlight, expected.settings.plain_highlight);
    assert!((settings.font_scale - expected.settings.font_scale).abs() < f64::EPSILON);
    assert_eq!(settings.margin_size, expected.settings.margin_size);
    assert_eq!(
        settings.line_spacing.as_str(),
        expected.settings.line_spacing
    );
    assert_eq!(settings.mouse_capture, expected.settings.mouse_capture);

    // Keys outside the preference set are readable as raw strings.
    assert_eq!(
        storage.setting("globalWpm").expect("read"),
        Some(expected.raw_settings.global_wpm)
    );
    assert_eq!(
        storage.setting("globalActiveMs").expect("read"),
        Some(expected.raw_settings.global_active_ms)
    );
    std::fs::remove_dir_all(scratch).ok();
}

#[test]
fn the_library_book_list_matches_what_v1_reported() {
    let (storage, scratch) = open_fixture("books");
    let expected = expected();

    let books = storage.books().expect("books should load");
    assert_eq!(books.len(), expected.books.len());
    for (actual, golden) in books.iter().zip(&expected.books) {
        assert_eq!(actual.id, golden.id);
        assert_eq!(actual.title, golden.title);
        assert_eq!(actual.author, golden.author);
        assert_eq!(actual.source_path, golden.source_path);
        assert_eq!(actual.import_hash, golden.import_hash);
        assert_eq!(actual.parser_version, golden.parser_version);
        assert_eq!(actual.last_opened_at, golden.last_opened_at);
        assert_eq!(actual.render_mode.as_str(), golden.render_mode);
    }
    assert_eq!(
        storage.latest_book_id().expect("query"),
        Some(expected.latest_book_id)
    );
    for (book_id, needs) in expected.needs_reimport {
        assert_eq!(
            storage.needs_epub_reimport(&book_id).expect("query"),
            needs,
            "{book_id} reimport check"
        );
    }
    std::fs::remove_dir_all(scratch).ok();
}

#[test]
fn a_stored_book_loads_with_the_same_chapters_and_blocks() {
    let (storage, scratch) = open_fixture("book");
    let expected = expected();

    let book = storage
        .book("fixture-book")
        .expect("book should load")
        .expect("book exists");
    assert_eq!(book.title, expected.book.title);
    assert_eq!(book.author, expected.book.author);
    assert_eq!(book.chapters.len(), expected.book.chapters.len());
    for (chapter, golden) in book.chapters.iter().zip(&expected.book.chapters) {
        assert_eq!(chapter.id, golden.id);
        assert_eq!(chapter.index, golden.index);
        assert_eq!(chapter.title, golden.title);
        assert_eq!(chapter.href, golden.href);
        assert_eq!(chapter.depth, golden.depth);
        assert_eq!(chapter.word_count, golden.word_count);
        let blocks: Vec<serde_json::Value> = chapter
            .blocks
            .iter()
            .map(|block| serde_json::to_value(block).expect("serialize"))
            .collect();
        assert_eq!(blocks, golden.blocks, "chapter {} blocks", golden.index);
    }
    // v1 always wrote `context: null`; v2 omits the field when it is absent, so
    // the comparison fills it in. Blocks need no such treatment: they are stored
    // as JSON in the database, and that shape is asserted verbatim above.
    let diagnostics: Vec<serde_json::Value> = book
        .diagnostics
        .iter()
        .map(|diagnostic| {
            let mut value = serde_json::to_value(diagnostic).expect("serialize");
            if let Some(object) = value.as_object_mut() {
                object.entry("context").or_insert(serde_json::Value::Null);
            }
            value
        })
        .collect();
    assert_eq!(diagnostics, expected.book.diagnostics);
    std::fs::remove_dir_all(scratch).ok();
}

#[test]
fn positions_bookmarks_notes_tags_and_pace_survive_the_crossing() {
    let (storage, scratch) = open_fixture("state");
    let expected = expected();

    let position = storage
        .position("fixture-book")
        .expect("load")
        .expect("position exists");
    assert_eq!(position.chapter_index, expected.position.chapter_index);
    assert_eq!(position.block_offset, expected.position.block_offset);
    assert!((position.chapter_progress - expected.position.chapter_progress).abs() < 1e-9);
    assert!((position.book_progress - expected.position.book_progress).abs() < 1e-9);

    let bookmarks = storage.bookmarks("fixture-book").expect("load");
    assert_eq!(bookmarks.len(), expected.bookmarks.len());
    for (actual, golden) in bookmarks.iter().zip(&expected.bookmarks) {
        assert_eq!(actual.chapter_index, golden.chapter_index);
        assert_eq!(actual.block_offset, golden.block_offset);
        assert_eq!(actual.label, golden.label);
        assert_eq!(actual.created_at, golden.created_at);
    }

    let notes = storage.notes("fixture-book").expect("load");
    assert_eq!(notes.len(), expected.notes.len());
    for (actual, golden) in notes.iter().zip(&expected.notes) {
        assert_eq!(actual.chapter_index, golden.chapter_index);
        assert_eq!(actual.block_offset, golden.block_offset);
        assert_eq!(actual.content, golden.content);
        assert_eq!(actual.created_at, golden.created_at);
    }

    assert_eq!(storage.tags("fixture-book").expect("load"), expected.tags);

    let pace = storage
        .reading_pace("fixture-book")
        .expect("load")
        .expect("pace exists");
    assert!((pace.wpm - expected.reading_pace.wpm).abs() < 1e-9);
    assert!((pace.active_ms - expected.reading_pace.active_ms).abs() < 1e-9);
    assert_eq!(pace.updated_at, expected.reading_pace.updated_at);
    std::fs::remove_dir_all(scratch).ok();
}

#[test]
fn exporting_a_v1_database_reproduces_the_v1_export() {
    let (storage, scratch) = open_fixture("export");
    let expected = expected();

    let exported = storage.export_all(1_700_000_000_000).expect("export");
    let actual = serde_json::to_value(&exported).expect("serialize");

    for section in ["positions", "bookmarks", "notes", "tags"] {
        assert_eq!(
            actual[section], expected.export_all[section],
            "{section} differ from the v1 export"
        );
    }
    assert_eq!(actual["version"], 1);
    std::fs::remove_dir_all(scratch).ok();
}

#[test]
fn opening_a_v1_database_redacts_tokens_written_before_the_migration() {
    let (storage, scratch) = open_fixture("redaction");

    let history = storage.command_history(10).expect("read");
    assert!(
        history.contains(&"/toggl auth <redacted>".to_owned()),
        "the legacy token was not redacted: {history:?}"
    );
    assert!(
        !history
            .iter()
            .any(|entry| entry.contains("toggl_sk_legacy_secret")),
        "a secret survived the migration: {history:?}"
    );
    assert!(
        history.contains(&"/next 2".to_owned()),
        "unrelated history was lost: {history:?}"
    );
    std::fs::remove_dir_all(scratch).ok();
}

#[test]
fn opening_a_v1_database_migrates_the_chapter_key_without_losing_rows() {
    let (mut storage, scratch) = open_fixture("chapter-key");

    // The chapters that were already there survived the table rebuild.
    let book = storage
        .book("fixture-book")
        .expect("load")
        .expect("book exists");
    assert_eq!(book.chapters.len(), 2);
    assert_eq!(book.chapters[0].id, "fixture-ch1");

    // And the v1 defect is gone: a second book may reuse those chapter ids.
    let mut clashing = book.clone();
    clashing.id = "clashing-book".into();
    clashing.import_hash = "clashing-hash".into();
    clashing.source_path = "/books/clashing.epub".into();
    storage
        .save_book(&clashing, RenderMode::Code, 1_800_000_000_000)
        .expect("a book sharing chapter ids must save");
    assert_eq!(
        storage
            .book("clashing-book")
            .expect("load")
            .expect("book exists")
            .chapters
            .len(),
        2
    );

    // Migrating twice is a no-op.
    drop(storage);
    let paths = AppPaths::from_roots(&scratch.join("data"), &scratch.join("cache"));
    let reopened = Storage::open(&paths).expect("reopen");
    assert_eq!(
        reopened
            .book("fixture-book")
            .expect("load")
            .expect("book exists")
            .chapters
            .len(),
        2
    );
    std::fs::remove_dir_all(scratch).ok();
}

#[test]
fn a_v1_database_stays_writable_after_being_opened_by_v2() {
    let (mut storage, scratch) = open_fixture("writable");

    storage
        .add_bookmark("fixture-book", 0, 5, Some("added by v2"), 1_800_000_000_000)
        .expect("bookmark");
    let settings = AppSettings {
        render_mode: RenderMode::Code,
        ..AppSettings::default()
    };
    storage.save_settings(&settings).expect("settings");

    assert_eq!(storage.bookmarks("fixture-book").expect("load").len(), 3);
    assert_eq!(
        storage.settings().expect("load").render_mode,
        RenderMode::Code
    );

    let listed = storage
        .books_with_progress(LibrarySortKey::LastOpened, SortDirection::Descending, None)
        .expect("list");
    assert_eq!(listed[0].entry.id, "fixture-book");
    assert_eq!(listed[0].chapter_title.as_deref(), Some("Chapter Two"));
    std::fs::remove_dir_all(scratch).ok();
}

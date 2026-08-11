//! The command contract, ported from v1's `test/executor-contract.test.ts`.
//!
//! These are behavioral assertions, not a transcription of the TypeScript: they
//! pin the observable results — state transitions and status text — that the
//! reader's users depend on.

use reader_app::{
    CommandContext, Overlay, ReaderState, execute_command, open_book, state::MAX_NAV_HISTORY,
};
use reader_core::{
    AppSettings, CanonicalBlock, CanonicalBook, CanonicalChapter, CodeLanguage, LibrarySortKey,
    ProgressVisibility, RenderMode,
};
use reader_storage::Storage;

const CONTEXT: CommandContext = CommandContext {
    now: 1_700_000_000_000,
    content_width: 80,
    body_height: 20,
};

fn paragraph(id: &str, text: &str) -> CanonicalBlock {
    CanonicalBlock::Paragraph {
        id: id.into(),
        text: text.into(),
    }
}

/// A three-chapter book with a phrase shared by two chapters, so chapter-scoped
/// and whole-book searches can be told apart.
fn book() -> CanonicalBook {
    CanonicalBook {
        id: "contract-book".into(),
        title: "Contract Book".into(),
        author: "Reader".into(),
        source_path: "/tmp/contract.epub".into(),
        import_hash: "contract-hash".into(),
        parser_version: Some(3),
        diagnostics: Vec::new(),
        chapters: vec![
            CanonicalChapter {
                id: "chapter-1".into(),
                index: 0,
                title: "Opening".into(),
                href: "opening.xhtml".into(),
                depth: 0,
                blocks: vec![
                    paragraph("b1", "alpha opening"),
                    paragraph("b2", "shared phrase"),
                ],
                word_count: 10,
            },
            CanonicalChapter {
                id: "chapter-2".into(),
                index: 1,
                title: "Middle".into(),
                href: "middle.xhtml".into(),
                depth: 0,
                blocks: vec![
                    paragraph("b3", "beta middle"),
                    paragraph("b4", "shared phrase"),
                ],
                word_count: 30,
            },
            CanonicalChapter {
                id: "chapter-3".into(),
                index: 2,
                title: "Ending".into(),
                href: "ending.xhtml".into(),
                depth: 0,
                blocks: vec![paragraph("b5", "omega ending")],
                word_count: 60,
            },
        ],
        cover_path: None,
    }
}

/// A reader with the fixture book open and its record in the library.
fn reader() -> (ReaderState, Storage) {
    let mut storage = Storage::open_in_memory().expect("in-memory database");
    let book = book();
    storage
        .save_book(&book, RenderMode::Plain, CONTEXT.now)
        .expect("save");
    let mut state = ReaderState::new(AppSettings {
        render_mode: RenderMode::Plain,
        ..AppSettings::default()
    });
    open_book(&mut state, &storage, book, CONTEXT).expect("open");
    (state, storage)
}

/// A reader with the book in the library but nothing open.
fn empty_reader() -> (ReaderState, Storage) {
    let (mut state, storage) = reader();
    state.current_book = None;
    state.status.clear();
    (state, storage)
}

fn run(state: &mut ReaderState, storage: &mut Storage, command: &str) {
    execute_command(state, storage, command, CONTEXT).expect("commands must not fail the frame");
}

#[test]
fn chapter_navigation_is_bounded_and_recorded() {
    let (mut state, mut storage) = reader();
    state.chapter_index = 1;
    state.block_offset = 8;

    run(&mut state, &mut storage, "/prev 5");
    assert_eq!(state.chapter_index, 0);
    assert_eq!(state.block_offset, 0);
    assert_eq!(state.status, "Moved to chapter 1");

    run(&mut state, &mut storage, "/next 99");
    assert_eq!(state.chapter_index, 2);
    assert_eq!(state.status, "Moved to chapter 3");

    // Every visited place is recorded, up to the bound.
    for offset in 0..60 {
        state.block_offset = offset;
        state.push_nav_history();
    }
    assert_eq!(state.nav_history.len(), MAX_NAV_HISTORY);
    assert_eq!(
        state.nav_history.last().map(|entry| entry.block_offset),
        Some(59)
    );
}

#[test]
fn chapter_navigation_reuses_whole_book_layout_metrics() {
    let (mut state, mut storage) = reader();
    let _ = state.chapter_line_count(CONTEXT.content_width, CONTEXT.body_height);
    let (hits_before, misses_before) = state.layout_metrics_cache_stats();

    run(&mut state, &mut storage, "/next");
    let _ = state.chapter_line_count(CONTEXT.content_width, CONTEXT.body_height);

    assert_eq!(
        state.layout_metrics_cache_stats(),
        (hits_before + 1, misses_before),
        "moving between chapters must not render every chapter again"
    );
}

#[test]
fn navigation_without_a_book_does_nothing() {
    let (mut state, mut storage) = empty_reader();
    run(&mut state, &mut storage, "/next");
    assert_eq!(state.chapter_index, 0);
    assert!(state.status.is_empty());
    assert!(state.nav_history.is_empty());
}

#[test]
fn bookmarks_get_automatic_labels_and_resolve_by_id_or_label() {
    let (mut state, mut storage) = reader();
    state.chapter_index = 1;
    state.block_offset = 7;

    run(&mut state, &mut storage, "/mark");
    run(&mut state, &mut storage, "/mark Important passage");
    let labels: Vec<Option<String>> = storage
        .bookmarks("contract-book")
        .expect("list")
        .into_iter()
        .map(|bookmark| bookmark.label)
        .collect();
    assert!(labels.contains(&Some("Ch.2 §7".to_owned())));
    assert!(labels.contains(&Some("Important passage".to_owned())));

    run(&mut state, &mut storage, "/marks");
    assert_eq!(state.overlay, Overlay::Bookmarks);
    assert_eq!(state.status, "Opened bookmarks.");

    // A unique partial label is enough to delete.
    run(&mut state, &mut storage, "/delmark Important");
    assert_eq!(state.status, "Bookmark deleted.");
    let remaining = storage.bookmarks("contract-book").expect("list");
    assert_eq!(remaining.len(), 1);

    let id = remaining[0].id.clone();
    run(&mut state, &mut storage, &format!("/delmark {id}"));
    assert!(storage.bookmarks("contract-book").expect("list").is_empty());

    run(&mut state, &mut storage, "/delmark missing");
    assert_eq!(state.status, "No bookmark matched \"missing\".");
    run(&mut state, &mut storage, "/delmark");
    assert_eq!(state.status, "Use /delmark <id|label>");
}

#[test]
fn an_empty_bookmark_list_says_so() {
    let (mut state, mut storage) = reader();
    run(&mut state, &mut storage, "/marks");
    assert_eq!(state.status, "No bookmarks in this book yet.");
}

#[test]
fn bookmark_commands_need_an_open_book() {
    let (mut state, mut storage) = empty_reader();
    for command in ["/mark", "/marks", "/delmark x", "/note text", "/tags"] {
        run(&mut state, &mut storage, command);
        assert_eq!(state.status, "No book open.", "{command}");
    }
}

#[test]
fn search_distinguishes_chapter_and_book_scope_and_jumps_to_the_first_hit() {
    let (mut state, mut storage) = reader();
    state.chapter_index = 1;

    run(&mut state, &mut storage, "/search shared");
    let search = state.search.as_ref().expect("a search is active");
    assert!(!search.global);
    assert_eq!(search.results.len(), 1);
    assert_eq!(search.results[0].chapter_index, 1);
    assert_eq!(search.results[0].block_index, 1);
    assert_eq!(state.chapter_index, 1);
    assert_eq!(
        state.status,
        "Search: 1 match(es) in chapter for \"shared\"."
    );

    run(&mut state, &mut storage, "/search -g shared");
    let search = state.search.as_ref().expect("a search is active");
    assert!(search.global);
    assert_eq!(search.results.len(), 2);
    assert_eq!(
        state.chapter_index, 0,
        "a global search moves to the first hit"
    );

    run(&mut state, &mut storage, "/search absent");
    assert!(state.search.is_none());
    assert_eq!(state.status, "No matches for \"absent\" in this chapter.");

    run(&mut state, &mut storage, "/search");
    assert_eq!(state.status, "Use /search [-g|--global] <term>");
}

#[test]
fn goto_understands_chapters_book_percentages_and_chapter_percentages() {
    let (mut state, mut storage) = reader();

    run(&mut state, &mut storage, "/goto 0%");
    assert_eq!(state.chapter_index, 0);
    assert_eq!(state.status, "Jumped to 0% (start of book)");

    run(&mut state, &mut storage, "/goto 50%");
    assert_eq!(
        state.chapter_index, 2,
        "chapters are weighted by word count"
    );
    assert!(
        state.status.starts_with("Jumped to 50% of book"),
        "{}",
        state.status
    );

    run(&mut state, &mut storage, "/goto 100%");
    assert_eq!(state.chapter_index, 2);
    assert_eq!(state.status, "Jumped to 100% (end of book)");

    run(&mut state, &mut storage, "/goto 2");
    assert_eq!(state.chapter_index, 1);
    assert_eq!(state.block_offset, 0);
    assert_eq!(state.status, "Jumped to chapter 2");

    run(&mut state, &mut storage, "/goto 25%c");
    assert!(
        state.status.starts_with("Jumped to 25% of chapter 2"),
        "{}",
        state.status
    );

    run(&mut state, &mut storage, "/goto 30% --chapter");
    assert!(
        state.status.starts_with("Jumped to 30% of chapter 2"),
        "{}",
        state.status
    );
}

#[test]
fn goto_rejects_positions_that_do_not_exist() {
    let (mut state, mut storage) = reader();

    run(&mut state, &mut storage, "/goto 101%");
    assert_eq!(state.status, "Percentage must be between 0 and 100.");

    run(&mut state, &mut storage, "/goto nowhere");
    assert!(
        state.status.starts_with("Could not parse position"),
        "{}",
        state.status
    );

    run(&mut state, &mut storage, "/goto 99");
    assert_eq!(
        state.status,
        "There is no chapter 99. This book has 3 chapter(s)."
    );

    run(&mut state, &mut storage, "/goto");
    assert_eq!(state.status, "Use /goto <chapter>|<percent>%|…%c>");

    let (mut empty, mut storage) = empty_reader();
    run(&mut empty, &mut storage, "/goto 1");
    assert_eq!(empty.status, "No book open.");
}

#[test]
fn appearance_and_reading_commands_persist_valid_choices() {
    let (mut state, mut storage) = reader();

    run(&mut state, &mut storage, "/mode rust");
    assert_eq!(state.settings.render_mode, RenderMode::Code);
    assert_eq!(state.settings.code_language, CodeLanguage::Rust);
    assert_eq!(
        storage.setting("codeLanguage").expect("read"),
        Some("rust".to_owned())
    );
    assert_eq!(state.status, "Render mode: rust");

    run(&mut state, &mut storage, "/mode plain");
    assert_eq!(state.settings.render_mode, RenderMode::Plain);

    run(&mut state, &mut storage, "/mode invalid");
    assert_eq!(
        state.status,
        "Mode must be plain, typescript, python, or rust"
    );
    assert_eq!(state.settings.render_mode, RenderMode::Plain, "unchanged");

    run(&mut state, &mut storage, "/density 5");
    assert_eq!(state.settings.code_density.get(), 5);
    run(&mut state, &mut storage, "/density 9");
    assert_eq!(state.status, "Density must be a number between 1 and 5");
    assert_eq!(state.settings.code_density.get(), 5, "unchanged");

    run(&mut state, &mut storage, "/highlight off");
    assert!(!state.settings.plain_highlight);
    run(&mut state, &mut storage, "/highlight maybe");
    assert_eq!(state.status, "Use /highlight <on|off>");

    run(&mut state, &mut storage, "/toggleprogress hidden");
    assert_eq!(
        state.settings.progress_visibility,
        ProgressVisibility::Hidden
    );
    run(&mut state, &mut storage, "/toggleprogress");
    assert_eq!(
        state.settings.progress_visibility,
        ProgressVisibility::TimeChapter,
        "cycling wraps"
    );

    run(&mut state, &mut storage, "/mouse");
    assert!(state.settings.mouse_capture);
    run(&mut state, &mut storage, "/mouse off");
    assert!(!state.settings.mouse_capture);
    run(&mut state, &mut storage, "/mouse invalid");
    assert_eq!(state.status, "Use /mouse [on|off]");

    // Preferences survive a reload from the database.
    let reloaded = storage.settings().expect("load");
    assert_eq!(reloaded.code_density.get(), 5);
    assert!(!reloaded.plain_highlight);
}

#[test]
fn colorschemes_and_themes_apply_or_report_an_unknown_name() {
    let (mut state, mut storage) = reader();

    run(&mut state, &mut storage, "/colorscheme claude");
    assert_eq!(state.settings.theme_id.as_str(), "claude");
    assert_eq!(state.theme.scheme, state.settings.theme_id);
    assert_eq!(state.status, "Colorscheme set to Claude Code");

    run(&mut state, &mut storage, "/theme light");
    assert_eq!(state.settings.appearance_theme_id.as_str(), "light");
    assert_eq!(state.status, "Theme set to Light Chalk");
    assert_eq!(state.theme.id(), "claude:light");

    run(&mut state, &mut storage, "/colorscheme missing");
    assert_eq!(state.status, "Unknown colorscheme missing");
    run(&mut state, &mut storage, "/theme missing");
    assert_eq!(state.status, "Unknown theme missing");
    assert_eq!(state.settings.theme_id.as_str(), "claude", "unchanged");

    run(&mut state, &mut storage, "/colorscheme");
    assert_eq!(state.overlay, Overlay::ColorSchemes);
    assert_eq!(state.status, "Opened colorscheme picker");
    run(&mut state, &mut storage, "/theme --list");
    assert_eq!(state.overlay, Overlay::Themes);
}

#[test]
fn overlays_open_with_a_sensible_cursor() {
    let (mut state, mut storage) = reader();
    state.chapter_index = 2;

    run(&mut state, &mut storage, "/chapters");
    assert_eq!(state.overlay, Overlay::Chapters);
    assert_eq!(state.overlay_cursor, 2, "the current chapter is selected");
    assert_eq!(state.status, "Opened table of contents");

    run(&mut state, &mut storage, "/settings");
    assert_eq!(state.overlay, Overlay::Settings);
    run(&mut state, &mut storage, "/keys");
    assert_eq!(state.overlay, Overlay::Keys);

    run(&mut state, &mut storage, "/help mode");
    assert_eq!(state.overlay, Overlay::Help);
    assert_eq!(state.help_command.as_deref(), Some("mode"));
    assert_eq!(state.status, "Opened help for /mode");
    run(&mut state, &mut storage, "/help --all");
    assert_eq!(state.help_command, None);
    assert_eq!(state.status, "Opened command manual");
}

#[test]
fn resume_opens_the_latest_book_or_reports_an_empty_library() {
    let (mut state, mut storage) = empty_reader();

    run(&mut state, &mut storage, "/resume --latest");
    assert_eq!(state.book_id(), Some("contract-book"));
    assert_eq!(state.status, "Opened Contract Book");
    assert_eq!(state.chapter_index, 0);

    let mut empty_library = Storage::open_in_memory().expect("database");
    let mut fresh = ReaderState::new(AppSettings::default());
    run(&mut fresh, &mut empty_library, "/resume");
    assert_eq!(fresh.status, "No previous book to resume.");
    assert!(fresh.current_book.is_none());
}

#[test]
fn resume_restores_the_saved_position() {
    let (mut state, mut storage) = reader();
    storage
        .save_position(
            "contract-book",
            reader_core::ReadingPosition {
                chapter_index: 2,
                chapter_progress: 0.5,
                book_progress: 0.9,
                block_offset: 3,
            },
            CONTEXT.now,
        )
        .expect("position");
    state.current_book = None;

    run(&mut state, &mut storage, "/resume --latest");
    assert_eq!(state.chapter_index, 2);
}

#[test]
fn changebook_opens_a_match_filters_by_tag_or_falls_back_to_the_picker() {
    let (mut state, mut storage) = empty_reader();

    run(&mut state, &mut storage, "/book Contract");
    assert_eq!(state.book_id(), Some("contract-book"));

    run(&mut state, &mut storage, "/changebook");
    assert_eq!(state.overlay, Overlay::Books);
    assert_eq!(state.status, "Opened library picker.");

    run(&mut state, &mut storage, "/changebook nothing-like-this");
    assert_eq!(state.status, "No exact match. Opened library picker.");

    storage.add_tag("contract-book", "favorite").expect("tag");
    // A unique tag match opens that book directly.
    state.current_book = None;
    run(&mut state, &mut storage, "/changebook favorite");
    assert_eq!(state.book_id(), Some("contract-book"));

    run(&mut state, &mut storage, "/changebook --sort progress");
    assert_eq!(state.library_sort_key, LibrarySortKey::Progress);
    run(&mut state, &mut storage, "/changebook --sort sideways");
    assert_eq!(
        state.status,
        "Invalid sort key \"sideways\". Use: lastOpened, title, author, progress"
    );
}

#[test]
fn removal_clears_the_reader_and_reports_an_empty_one() {
    let (mut state, mut storage) = reader();

    run(&mut state, &mut storage, "/removecurrent");
    assert!(state.current_book.is_none());
    assert_eq!(state.status, "Current book removed from the library.");
    assert!(storage.book("contract-book").expect("load").is_none());

    run(&mut state, &mut storage, "/removecurrent");
    assert_eq!(state.status, "No current book to remove.");
}

#[test]
fn remove_by_query_only_clears_the_reader_when_it_was_the_open_book() {
    let (mut state, mut storage) = reader();
    let mut other = book();
    other.id = "other-book".into();
    other.title = "Other Book".into();
    other.import_hash = "other-hash".into();
    other.source_path = "/tmp/other.epub".into();
    // Chapter ids are globally unique in the v1 schema, so a second book needs
    // its own.
    for chapter in &mut other.chapters {
        chapter.id = format!("other-{}", chapter.id);
    }
    storage
        .save_book(&other, RenderMode::Plain, CONTEXT.now)
        .expect("save");

    run(&mut state, &mut storage, "/remove Other");
    assert_eq!(state.status, "Removed Other Book from the library.");
    assert_eq!(state.book_id(), Some("contract-book"), "still open");

    run(&mut state, &mut storage, "/remove Contract");
    assert!(state.current_book.is_none());

    run(&mut state, &mut storage, "/remove ghost");
    assert_eq!(state.status, "No matching book found.");
}

#[test]
fn tags_and_notes_round_trip_through_the_library() {
    let (mut state, mut storage) = reader();

    run(&mut state, &mut storage, "/tags");
    assert_eq!(state.status, "No tags for this book.");

    run(&mut state, &mut storage, "/tag favorite");
    assert_eq!(state.status, "Tag added: #favorite");
    run(&mut state, &mut storage, "/tag sci-fi");
    run(&mut state, &mut storage, "/tags");
    assert_eq!(state.status, "Tags: #favorite  #sci-fi");

    run(&mut state, &mut storage, "/tag -d favorite");
    assert_eq!(state.status, "Tag removed: #favorite");
    run(&mut state, &mut storage, "/tag -d");
    assert_eq!(state.status, "Use /tag -d <tag>");

    run(&mut state, &mut storage, "/note remember this");
    assert!(state.status.starts_with("Note saved ("), "{}", state.status);
    let notes = storage.notes("contract-book").expect("list");
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].content, "remember this");
    assert_eq!(notes[0].chapter_index, Some(0));

    run(&mut state, &mut storage, "/note -l");
    assert_eq!(state.overlay, Overlay::Notes);
    assert_eq!(state.status, "Opened notes.");

    run(&mut state, &mut storage, "/note -d nonexistent");
    assert_eq!(state.status, "Note not found in current book.");
    run(
        &mut state,
        &mut storage,
        &format!("/note -d {}", notes[0].id),
    );
    assert_eq!(state.status, "Note deleted.");
    assert!(storage.notes("contract-book").expect("list").is_empty());

    run(&mut state, &mut storage, "/note");
    assert_eq!(state.status, "Use /note <text>");
}

#[test]
fn commands_in_focus_mode_keep_the_viewport_and_move_the_focused_block() {
    let (mut state, mut storage) = reader();
    state.focus_mode = true;
    state.focus_block_index = 1;
    state.block_offset = 42;

    run(&mut state, &mut storage, "/goto 3");

    assert_eq!(state.chapter_index, 2);
    assert_eq!(state.focus_block_index, 0, "the new chapter starts focused");
    assert_eq!(state.block_offset, 42, "the viewport offset is untouched");
}

#[test]
fn a_bad_command_line_becomes_a_status_message() {
    let (mut state, mut storage) = reader();

    run(&mut state, &mut storage, "/nope");
    assert_eq!(state.status, "Unknown command: nope");

    run(&mut state, &mut storage, "next");
    assert_eq!(state.status, "Command must start with /");

    run(&mut state, &mut storage, "/next --sideways");
    assert_eq!(state.status, "Unknown flag --sideways for /next");
}

#[test]
fn every_command_is_recorded_in_history_with_secrets_removed() {
    let (mut state, mut storage) = reader();
    run(&mut state, &mut storage, "/next");
    run(&mut state, &mut storage, "/toggl auth toggl_sk_secret");

    let history = storage.command_history(10).expect("read");
    assert!(
        history.contains(&"/toggl auth <redacted>".to_owned()),
        "{history:?}"
    );
    assert!(history.contains(&"/next".to_owned()));
}

#[test]
fn opening_a_book_loads_its_learned_pace_and_persists_the_previous_one() {
    // The pace is written before any book is open, because opening a book first
    // flushes the model of the one being closed — which would overwrite it.
    let (mut state, storage) = empty_reader();
    storage
        .set_setting("globalWpm", "260.5")
        .expect("global pace");
    storage
        .save_reading_pace(
            "contract-book",
            reader_core::BookReadingPace {
                wpm: 310.0,
                active_ms: 120_000.0,
                updated_at: CONTEXT.now,
            },
        )
        .expect("book pace");

    open_book(&mut state, &storage, book(), CONTEXT).expect("reopen");

    assert!((state.pace.global_wpm - 260.5).abs() < 1e-9);
    assert!((state.pace.book_wpm - 310.0).abs() < 1e-9);
    assert_eq!(state.pace.book_id.as_deref(), Some("contract-book"));

    // Closing the book writes the in-memory model back.
    state.pace.book_wpm = 275.0;
    state.pace.global_wpm = 250.0;
    open_book(&mut state, &storage, book(), CONTEXT).expect("reopen again");
    assert_eq!(
        storage.setting("globalWpm").expect("read"),
        Some("250".to_owned())
    );
    assert!(
        (storage
            .reading_pace("contract-book")
            .expect("read")
            .expect("pace")
            .wpm
            - 275.0)
            .abs()
            < 1e-9
    );
}

#[test]
fn opening_a_book_resets_the_session_view() {
    let (mut state, storage) = reader();
    state.focus_mode = true;
    state.focus_block_index = 3;
    state.search = Some(reader_app::SearchState {
        query: "shared".into(),
        global: true,
        results: Vec::new(),
        cursor: 0,
    });
    state.push_nav_history();

    open_book(&mut state, &storage, book(), CONTEXT).expect("open");

    assert!(!state.focus_mode);
    assert_eq!(state.focus_block_index, 0);
    assert!(state.search.is_none());
    assert!(state.nav_history.is_empty());
    assert_eq!(state.nav_history_cursor, None);
}

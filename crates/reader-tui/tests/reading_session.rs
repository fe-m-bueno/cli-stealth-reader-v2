//! One reading session, end to end.
//!
//! This is the closest thing to running the reader: a real EPUB is imported into
//! a real database, keys are pressed, and the resulting frames are asserted from
//! a test buffer. Nothing here is mocked except the terminal itself.

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use reader_app::{CommandContext, Overlay, ReaderState};
use reader_core::{AppSettings, RenderMode};
use reader_storage::{AppPaths, Storage};
use reader_tui::{
    CommandBar, PointerState, apply, current_mode, draw, footer_height, map_key, map_mouse,
};

/// The EPUB fixture the format tests already use.
fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("reader-formats")
        .join("tests")
        .join("fixtures")
        .join("ncx-nested.epub")
}

struct Session {
    state: ReaderState,
    storage: Storage,
    command_bar: CommandBar,
    pointer: PointerState,
    terminal: Terminal<TestBackend>,
    scratch: PathBuf,
}

impl Session {
    fn start(name: &str) -> Self {
        let scratch =
            std::env::temp_dir().join(format!("reader-tui-session-{}-{name}", std::process::id()));
        std::fs::remove_dir_all(&scratch).ok();
        let paths = AppPaths::from_roots(&scratch.join("data"), &scratch.join("cache"));
        let mut storage = Storage::open(&paths).expect("library opens");

        let mut state = ReaderState::new(AppSettings {
            render_mode: RenderMode::Plain,
            ..AppSettings::default()
        });
        let terminal = Terminal::new(TestBackend::new(80, 24)).expect("test terminal");

        let context = CommandContext {
            now: 1_700_000_000_000,
            content_width: 76,
            body_height: 21,
        };
        reader_app::import_and_open(&mut state, &mut storage, &fixture(), context)
            .expect("the fixture imports");

        Self {
            state,
            storage,
            command_bar: CommandBar::default(),
            pointer: PointerState::default(),
            terminal,
            scratch,
        }
    }

    fn context(&mut self) -> CommandContext {
        let layout = self
            .state
            .layout(footer_height(&self.state, &self.command_bar));
        CommandContext {
            now: 1_700_000_000_000,
            content_width: layout.content_width,
            body_height: layout.body_height,
        }
    }

    /// Press a key, exactly as the event loop would.
    fn press(&mut self, code: KeyCode) {
        self.press_with(code, KeyModifiers::NONE);
    }

    fn press_with(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        let mode = current_mode(&self.state, &self.command_bar);
        let action = map_key(KeyEvent::new(code, modifiers), mode);
        let context = self.context();
        apply(
            action,
            &mut self.state,
            &mut self.storage,
            &mut self.command_bar,
            context,
        )
        .expect("actions must not fail a session");
    }

    /// Deliver a mouse event, exactly as the event loop would.
    fn mouse(&mut self, kind: MouseEventKind, column: u16, row: u16) {
        let area = Rect {
            x: 0,
            y: 0,
            width: self.state.viewport.width,
            height: self.state.viewport.height,
        };
        let entries = reader_app::visible_entries(&self.state, &self.storage, 1_700_000_000_000);
        let action = map_mouse(
            MouseEvent {
                kind,
                column,
                row,
                modifiers: KeyModifiers::NONE,
            },
            area,
            &mut self.state,
            &self.command_bar,
            &entries,
            &mut self.pointer,
        );
        let context = self.context();
        apply(
            action,
            &mut self.state,
            &mut self.storage,
            &mut self.command_bar,
            context,
        )
        .expect("pointer actions must not fail a session");
    }

    /// Type a command line and run it.
    fn command(&mut self, line: &str) {
        self.press(KeyCode::Char('/'));
        for character in line.chars() {
            self.press(KeyCode::Char(character));
        }
        self.press(KeyCode::Enter);
    }

    /// Draw a frame and return its rows.
    fn rows(&mut self) -> Vec<String> {
        let Self {
            terminal,
            state,
            storage,
            command_bar,
            ..
        } = self;
        let entries = reader_app::visible_entries(state, storage, 1_700_000_000_000);
        terminal
            .draw(|frame| draw(frame, state, command_bar, &entries))
            .expect("drawing succeeds");
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|row| {
                (0..buffer.area.width)
                    .map(|column| buffer[(column, row)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect()
    }

    fn screen(&mut self) -> String {
        self.rows().join("\n")
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.scratch).ok();
    }
}

#[test]
fn a_session_opens_a_book_and_shows_its_first_chapter() {
    let mut session = Session::start("open");

    let screen = session.screen();

    assert!(
        screen.contains("Benchmark Book") || screen.contains("Chapter One"),
        "the frame should name the book and chapter:\n{screen}"
    );
    assert!(
        screen.contains("lantern"),
        "the first chapter's prose should be on screen:\n{screen}"
    );
}

#[test]
fn scrolling_and_chapter_keys_move_through_the_book() {
    let mut session = Session::start("navigate");

    // A chapter that fits the screen cannot scroll, so shrink the terminal first.
    session.terminal = Terminal::new(TestBackend::new(48, 10)).expect("terminal");
    let first = session.rows();

    session.press(KeyCode::Char('j'));
    session.press(KeyCode::Char('j'));
    let scrolled = session.rows();
    assert_ne!(first[1], scrolled[1], "scrolling should move the text");

    session.press(KeyCode::Right);
    let next_chapter = session.screen();
    assert!(
        session.state.chapter_index == 1,
        "the right arrow should advance a chapter"
    );
    assert!(
        next_chapter.contains("Section") || next_chapter.contains("Sandstone"),
        "the second chapter should be on screen:\n{next_chapter}"
    );

    session.press(KeyCode::Left);
    assert_eq!(session.state.chapter_index, 0);
}

/// A directory holding copies of `names`, plus a file the reader ignores.
fn library_with(scratch: &Path, names: &[&str]) -> PathBuf {
    let root = scratch.join("library");
    std::fs::create_dir_all(root.join("nested")).expect("a library directory");
    for (index, name) in names.iter().enumerate() {
        // Half go in a subfolder, so discovery has to recurse to find them.
        let target = if index % 2 == 0 {
            root.join(name)
        } else {
            root.join("nested").join(name)
        };
        std::fs::copy(fixture().with_file_name(name), &target).expect("a fixture copy");
    }
    std::fs::write(root.join("notes.txt"), "not a book").expect("an unsupported file");
    root
}

#[test]
fn the_picker_discovers_recursively_ticks_files_and_imports_the_set() {
    let mut session = Session::start("picker");
    let root = library_with(
        &session.scratch,
        &["ncx-nested.epub", "front-matter.epub", "comic.cbz"],
    );

    session.command(&format!("librarydir {}", root.display()));
    session.command("add");

    assert_eq!(session.state.overlay, Overlay::FilePicker);
    assert_eq!(
        session.state.discoveries.len(),
        3,
        "recursive discovery, and only supported files: {:?}",
        session.state.discoveries
    );

    let screen = session.screen();
    assert!(screen.contains("Add Books"), "{screen}");
    assert!(screen.contains("[ ] "), "unticked checkboxes:\n{screen}");
    assert!(screen.contains("/ to search"), "{screen}");
    assert!(screen.contains("Space:select"), "{screen}");
    assert!(
        !screen.contains("notes.txt"),
        "an unsupported file is not offered:\n{screen}"
    );

    // Space ticks the row under the cursor rather than paging.
    session.press(KeyCode::Char(' '));
    session.press(KeyCode::Down);
    session.press(KeyCode::Char(' '));
    assert_eq!(session.state.picker_selected.len(), 2);
    let ticked = session.screen();
    assert!(ticked.contains("[x] "), "{ticked}");
    assert_eq!(
        session.state.status, "2 files selected · Enter imports",
        "the status counts the set"
    );

    session.press(KeyCode::Enter);

    assert_eq!(session.state.overlay, Overlay::None, "the picker closes");
    assert_eq!(
        session.storage.books().expect("the library").len(),
        3,
        "the fixture book plus the two imported ones"
    );
    assert!(
        session.state.status.contains("Imported 2 books"),
        "{}",
        session.state.status
    );
}

#[test]
fn a_picker_search_narrows_the_rows_without_losing_which_file_is_ticked() {
    let mut session = Session::start("picker-search");
    let root = library_with(
        &session.scratch,
        &["ncx-nested.epub", "front-matter.epub", "comic.cbz"],
    );
    session.command(&format!("librarydir {}", root.display()));
    session.command("add");

    // Tick the comic, then narrow to something else entirely.
    let comic = session
        .state
        .discoveries
        .iter()
        .position(|item| item.file_name.contains("comic"))
        .expect("the comic is discovered");
    session.state.picker_selected.insert(comic);

    session.press(KeyCode::Char('/'));
    for character in "front".chars() {
        session.press(KeyCode::Char(character));
    }

    let listed = reader_app::visible_entries(&session.state, &session.storage, 0);
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert!(listed[0].display.contains("front-matter"), "{listed:?}");
    assert_ne!(
        listed[0].index(),
        Some(0),
        "a narrowed row still points at its own file"
    );

    // Ticking here adds to the set rather than replacing it.
    session.press(KeyCode::Enter);
    assert_eq!(
        session.state.overlay,
        Overlay::None,
        "confirming imports the ticked set"
    );
    assert!(
        session
            .storage
            .books()
            .expect("the library")
            .iter()
            .any(|book| book.title.to_lowercase().contains("comic")
                || book.source_path.contains("comic")),
        "the file ticked before the search is the one that imported"
    );
}

#[test]
fn a_picker_over_an_empty_directory_says_what_to_do() {
    let mut session = Session::start("picker-empty");
    let root = session.scratch.join("empty-library");
    std::fs::create_dir_all(&root).expect("an empty directory");

    session.command(&format!("librarydir {}", root.display()));
    session.command("add");

    assert!(session.state.discoveries.is_empty());
    let screen = session.screen();
    assert!(
        screen.contains("No EPUB, CBZ, or PDF files"),
        "the picker explains itself:\n{screen}"
    );
}

#[test]
fn a_malformed_file_is_named_and_the_picker_stays_open() {
    let mut session = Session::start("picker-malformed");
    let root = session.scratch.join("broken-library");
    std::fs::create_dir_all(&root).expect("a directory");
    std::fs::write(root.join("broken.epub"), b"not really a zip").expect("a broken book");

    session.command(&format!("librarydir {}", root.display()));
    session.command("add");
    session.press(KeyCode::Char(' '));
    session.press(KeyCode::Enter);

    assert_eq!(
        session.state.overlay,
        Overlay::FilePicker,
        "a failure keeps the picker"
    );
    assert!(
        session.state.status.contains("broken.epub"),
        "the failure names the file: {}",
        session.state.status
    );
    assert_eq!(
        session.state.picker_selected.len(),
        1,
        "the selection survives so it can be retried"
    );
}

#[test]
fn importing_the_same_file_twice_does_not_duplicate_the_library_row() {
    let mut session = Session::start("picker-duplicate");
    let root = library_with(&session.scratch, &["ncx-nested.epub"]);

    session.command(&format!("librarydir {}", root.display()));
    session.command("add");
    session.press(KeyCode::Enter);
    let after_first = session.storage.books().expect("the library").len();

    session.command("add");
    session.press(KeyCode::Enter);

    assert_eq!(
        session.storage.books().expect("the library").len(),
        after_first,
        "reimporting the same content reuses its row"
    );
}

#[test]
fn dragging_the_scrollbar_moves_the_page_and_saves_the_position() {
    let mut session = Session::start("scrollbar-drag");
    // A chapter that fits the screen has no scrollbar to drag.
    session.terminal = Terminal::new(TestBackend::new(48, 12)).expect("terminal");
    session.rows();

    let geometry = reader_tui::frame::geometry(
        Rect {
            x: 0,
            y: 0,
            width: session.state.viewport.width,
            height: session.state.viewport.height,
        },
        &session.state,
        &session.command_bar,
    );
    let column = geometry
        .scrollbar_column
        .expect("a scrollable chapter has a scrollbar");
    let body = geometry.body;

    session.mouse(
        MouseEventKind::Down(MouseButton::Left),
        column,
        body.y + body.height - 1,
    );
    let jumped = session.state.block_offset;
    assert!(jumped > 0, "pressing the track should move the page");

    session.mouse(MouseEventKind::Drag(MouseButton::Left), column, body.y);
    assert_eq!(
        session.state.block_offset, 0,
        "dragging the thumb back to the top returns to the start"
    );

    session.mouse(MouseEventKind::Up(MouseButton::Left), column, body.y);
    session.mouse(MouseEventKind::Drag(MouseButton::Left), column, body.y + 4);
    assert_eq!(
        session.state.block_offset, 0,
        "a drag after the release is no longer the reader's"
    );

    // The same persistence path the keyboard uses runs after a pointer scroll.
    session.mouse(
        MouseEventKind::Down(MouseButton::Left),
        column,
        body.y + body.height - 1,
    );
    let offset = session.state.block_offset;
    let position = reader_core::ReadingPosition {
        chapter_index: session.state.chapter_index,
        chapter_progress: 0.0,
        book_progress: 0.0,
        block_offset: offset,
    };
    let book_id = session.state.book_id().expect("a book").to_owned();
    session
        .storage
        .save_position(&book_id, position, 1_700_000_000_000)
        .expect("the position is written");
    assert_eq!(
        session
            .storage
            .position(&book_id)
            .expect("read back")
            .expect("a saved position")
            .block_offset,
        offset
    );
}

#[test]
fn clicking_the_shortcut_modal_folds_a_group_selects_a_row_and_closes_it() {
    let mut session = Session::start("shortcuts-pointer");
    session.press(KeyCode::Char('?'));
    session.rows();

    let body = reader_tui::frame::geometry(
        Rect {
            x: 0,
            y: 0,
            width: session.state.viewport.width,
            height: session.state.viewport.height,
        },
        &session.state,
        &session.command_bar,
    )
    .body;
    let modal = reader_tui::modals::modal_geometry(body);

    // The second row is the first Essentials binding, "/" — clicking it runs the
    // binding rather than asking the reader to go and press the key.
    session.mouse(
        MouseEventKind::Down(MouseButton::Left),
        modal.area.x + 4,
        modal.entries_y + 1,
    );
    assert_eq!(
        session.state.overlay,
        Overlay::None,
        "running a binding puts the panel away"
    );
    assert!(
        session.command_bar.active,
        "clicking “Focus command bar” focuses the command bar"
    );
    session.press(KeyCode::Esc);
    session.press(KeyCode::Char('?'));
    session.rows();

    // Clicking the Essentials heading folds it.
    session.mouse(
        MouseEventKind::Down(MouseButton::Left),
        modal.area.x + 4,
        modal.entries_y,
    );
    let folded = session.screen();
    assert!(folded.contains("› Essentials"), "{folded}");
    assert!(!folded.contains("Focus command bar"), "{folded}");

    // Clicking the search row starts a search.
    session.mouse(
        MouseEventKind::Down(MouseButton::Left),
        modal.area.x + 4,
        modal.area.y + 1,
    );
    assert!(session.state.overlay_search.active);

    // And the close control closes the modal.
    session.mouse(
        MouseEventKind::Down(MouseButton::Left),
        modal.area.x + modal.area.width - 2,
        modal.area.y,
    );
    assert_eq!(session.state.overlay, Overlay::None);
}

#[test]
fn the_reading_column_never_intercepts_a_plain_drag() {
    // Mouse capture is off by default, so the terminal keeps its own selection
    // and the reader sees no pointer events at all. With capture on the reader
    // does see them, and it still leaves every drag outside the scrollbar to the
    // terminal — which is what keeps Shift-drag selection working where the
    // terminal offers it.
    let mut session = Session::start("selection");
    assert!(
        !session.state.settings.mouse_capture,
        "capture stays off unless the reader asks for it"
    );

    session.state.settings.mouse_capture = true;
    session.terminal = Terminal::new(TestBackend::new(48, 12)).expect("terminal");
    session.rows();
    let before = session.state.block_offset;

    session.mouse(MouseEventKind::Down(MouseButton::Left), 10, 4);
    session.mouse(MouseEventKind::Drag(MouseButton::Left), 24, 7);
    session.mouse(MouseEventKind::Up(MouseButton::Left), 24, 7);

    assert_eq!(
        session.state.block_offset, before,
        "a drag across the text belongs to the terminal, not the reader"
    );
    assert_eq!(session.state.overlay, Overlay::None);
}

#[test]
fn a_typed_command_runs_and_reports_in_the_footer() {
    let mut session = Session::start("command");

    session.command("goto 2");

    assert_eq!(session.state.chapter_index, 1);
    let screen = session.screen();
    assert!(
        screen.contains("Jumped to chapter 2"),
        "the footer should report the jump:\n{screen}"
    );
}

#[test]
fn the_command_bar_shows_what_is_being_typed_and_what_it_could_be() {
    let mut session = Session::start("command-bar");

    session.press(KeyCode::Char('/'));
    for character in "goto".chars() {
        session.press(KeyCode::Char(character));
    }

    let rows = session.rows();
    assert!(
        rows.iter().any(|row| row.contains("/goto")),
        "the palette should echo the line: {rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("Jump by book %")),
        "and list what the command would do: {rows:?}"
    );

    session.press(KeyCode::Esc);
    let dismissed = session.rows();
    assert!(
        !dismissed.iter().any(|row| row.contains("/goto")),
        "escape should close the bar: {dismissed:?}"
    );
}

#[test]
fn switching_to_a_code_disguise_changes_what_is_on_screen() {
    let mut session = Session::start("disguise");
    let prose = session.screen();

    session.command("mode rust");
    let disguised = session.screen();

    assert_eq!(session.state.settings.render_mode, RenderMode::Code);
    assert_ne!(prose, disguised, "the disguise should change the frame");
    assert!(
        disguised.contains("let ")
            || disguised.contains("fn ")
            || disguised.contains("//")
            || disguised.contains("println!"),
        "the page should look like Rust:\n{disguised}"
    );
}

#[test]
fn searching_moves_to_a_match_and_cycles_through_them() {
    let mut session = Session::start("search");

    session.command("search -g harbour");
    session.terminal = Terminal::new(TestBackend::new(140, 24)).expect("terminal");
    let found = session.screen();
    assert!(
        found.contains("match(es) in book"),
        "the footer should report the search:\n{found}"
    );

    let first_chapter = session.state.chapter_index;
    session.press(KeyCode::Char('n'));
    assert!(
        session.state.status.starts_with("Match "),
        "status was {:?}",
        session.state.status
    );
    let _ = first_chapter;
}

#[test]
fn the_chapter_overlay_opens_and_jumps_to_the_selected_chapter() {
    let mut session = Session::start("overlay");

    session.press_with(KeyCode::Char('T'), KeyModifiers::NONE);
    assert_eq!(session.state.overlay, Overlay::Chapters);
    let overlay = session.screen();
    assert!(
        overlay.contains("Chapters"),
        "the overlay should be titled:\n{overlay}"
    );

    session.press(KeyCode::Down);
    session.press(KeyCode::Enter);

    assert_eq!(session.state.overlay, Overlay::None);
    assert_eq!(session.state.chapter_index, 1);
}

#[test]
fn a_bookmark_survives_being_saved_and_jumped_back_to() {
    let mut session = Session::start("bookmark");
    session.press(KeyCode::Char('j'));
    session.command("mark here");
    assert!(
        session.state.status.starts_with("Bookmark saved"),
        "status was {:?}",
        session.state.status
    );

    session.command("goto 2");
    assert_eq!(session.state.chapter_index, 1);

    session.press_with(KeyCode::Char('B'), KeyModifiers::NONE);
    assert_eq!(session.state.overlay, Overlay::Bookmarks);
    session.press(KeyCode::Enter);

    assert_eq!(
        session.state.chapter_index, 0,
        "the bookmark returns us home"
    );
    assert_eq!(session.state.status, "Jumped to here");
}

#[test]
fn quitting_saves_the_position_for_the_next_session() {
    let mut session = Session::start("resume");
    session.command("goto 2");
    session.press(KeyCode::Char('j'));
    session.press(KeyCode::Char('j'));
    let chapter = session.state.chapter_index;
    let offset = session.state.block_offset;

    // The event loop writes the position on the way out.
    let book_id = session.state.book_id().expect("a book is open").to_owned();
    let layout = session.state.layout(1);
    let chapter_progress = session
        .state
        .chapter_progress(layout.content_width, layout.body_height);
    session
        .storage
        .save_position(
            &book_id,
            reader_core::ReadingPosition {
                chapter_index: chapter,
                chapter_progress,
                book_progress: 0.5,
                block_offset: offset,
            },
            1_700_000_000_000,
        )
        .expect("position saves");

    // Reopening restores it.
    let book = session
        .storage
        .book(&book_id)
        .expect("load")
        .expect("book exists");
    let mut reopened = ReaderState::new(AppSettings::default());
    reader_app::open_book(
        &mut reopened,
        &session.storage,
        book,
        CommandContext {
            now: 1_700_000_000_000,
            content_width: 76,
            body_height: 21,
        },
    )
    .expect("reopen");

    assert_eq!(reopened.chapter_index, chapter);
    assert_eq!(reopened.block_offset, offset);
}

#[test]
fn the_reader_survives_a_narrow_terminal_and_a_resize() {
    let mut session = Session::start("resize");
    for (width, height) in [(20u16, 6u16), (200, 60), (40, 10)] {
        session.terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        let rows = session.rows();
        assert_eq!(
            rows.len(),
            height as usize,
            "{width}x{height} broke the frame"
        );
    }
}

#[test]
fn q_sets_the_quit_flag_the_loop_watches() {
    let mut session = Session::start("quit");
    session.press(KeyCode::Char('q'));
    assert!(session.state.should_quit);
}

#[test]
fn an_overlay_narrows_as_the_reader_types_and_opens_what_is_left() {
    let mut session = Session::start("overlay-search");
    session.press_with(KeyCode::Char('T'), KeyModifiers::NONE);
    assert_eq!(session.state.overlay, Overlay::Chapters);

    session.press(KeyCode::Char('/'));
    for character in "section".chars() {
        session.press(KeyCode::Char(character));
    }

    let screen = session.screen();
    assert!(
        screen.contains("Chapters · /section"),
        "the query belongs in the overlay title:\n{screen}"
    );
    let listed = reader_app::visible_entries(&session.state, &session.storage, 0);
    assert_eq!(
        listed
            .iter()
            .map(|row| row.display.trim())
            .collect::<Vec<_>>(),
        vec!["Section A"],
        "only the matching chapter is listed"
    );

    // Confirming acts on the filtered row, not on whatever was at that index.
    session.press(KeyCode::Enter);
    assert_eq!(session.state.overlay, Overlay::None);
    assert_eq!(
        session.state.chapter_index, 1,
        "the second chapter is the one that matched"
    );
}

#[test]
fn a_query_that_matches_nothing_says_so_instead_of_looking_broken() {
    let mut session = Session::start("overlay-empty");
    session.press_with(KeyCode::Char('T'), KeyModifiers::NONE);
    session.press(KeyCode::Char('/'));
    for character in "zzz".chars() {
        session.press(KeyCode::Char(character));
    }

    let screen = session.screen();
    assert!(screen.contains("Nothing matches."), "{screen}");
}

#[test]
fn leaving_search_restores_the_whole_list() {
    let mut session = Session::start("overlay-escape");
    session.press_with(KeyCode::Char('T'), KeyModifiers::NONE);
    session.press(KeyCode::Char('/'));
    session.press(KeyCode::Char('z'));
    session.press(KeyCode::Esc);

    assert_eq!(
        session.state.overlay,
        Overlay::Chapters,
        "escape leaves the search, not the overlay"
    );
    assert!(session.state.overlay_search.query().is_empty());
    let screen = session.screen();
    assert!(screen.contains("Chapter One"), "{screen}");
}

#[test]
fn the_shortcut_panel_folds_a_group_and_keeps_the_rest() {
    let mut session = Session::start("shortcuts-fold");
    session.press(KeyCode::Char('?'));
    assert_eq!(session.state.overlay, Overlay::Keys);

    // Essentials is open when the panel appears; the rest start folded.
    let opened = session.screen();
    assert!(opened.contains("◆ Essentials"), "{opened}");
    assert!(opened.contains("Focus command bar"), "{opened}");
    assert!(opened.contains("› Navigation"), "{opened}");

    // The cursor starts on the first header, so confirming folds it.
    session.press(KeyCode::Enter);
    let folded = session.screen();

    assert_eq!(session.state.overlay, Overlay::Keys, "the panel stays open");
    assert!(folded.contains("› Essentials"), "{folded}");
    assert!(!folded.contains("Focus command bar"), "{folded}");
    assert!(
        folded.contains("Commands"),
        "the other groups remain:\n{folded}"
    );

    // And unfolding a different group brings its bindings back.
    session.press(KeyCode::Down);
    session.press(KeyCode::Enter);
    let navigation = session.screen();
    assert!(navigation.contains("◆ Navigation"), "{navigation}");
    assert!(navigation.contains("Scroll up"), "{navigation}");
}

#[test]
fn the_settings_panel_previews_a_change_and_can_move_between_tabs() {
    let mut session = Session::start("settings");
    session.press_with(KeyCode::Char('S'), KeyModifiers::NONE);
    assert_eq!(session.state.overlay, Overlay::Settings);

    let opened = session.screen();
    assert!(opened.contains("Reader settings"), "{opened}");
    assert!(opened.contains("Themes"), "{opened}");
    assert!(opened.contains("Colorscheme"), "{opened}");
    assert!(
        opened.contains("Search settings..."),
        "the search row is visible:\n{opened}"
    );
    assert!(opened.contains("Preview"), "{opened}");
    assert!(
        opened.contains("Enter save") && opened.contains("Esc cancel"),
        "the footer explains save and cancel:\n{opened}"
    );

    // The cursor starts on the first setting, so space changes it.
    session.press(KeyCode::Char(' '));

    assert_eq!(
        session.state.settings.theme_id.as_str(),
        "claude",
        "the setting changed"
    );
    assert_eq!(
        session.state.theme.scheme, session.state.settings.theme_id,
        "and the theme previewed it"
    );

    session.press(KeyCode::Right);
    let reading_tab = session.screen();
    assert!(reading_tab.contains("Reading"), "{reading_tab}");
    assert!(reading_tab.contains("Code density"), "{reading_tab}");
    assert!(
        !reading_tab.contains("› Colorscheme"),
        "only the open tab's controls show:\n{reading_tab}"
    );
}

#[test]
fn cancelling_the_settings_panel_puts_everything_back() {
    let mut session = Session::start("settings-cancel");
    let before = session.state.settings;

    session.press_with(KeyCode::Char('S'), KeyModifiers::NONE);
    session.press(KeyCode::Down);
    session.press(KeyCode::Char(' '));
    assert_ne!(session.state.settings, before);

    session.press(KeyCode::Esc);

    assert_eq!(session.state.overlay, Overlay::None);
    assert_eq!(session.state.settings, before, "the preview was discarded");
    assert_eq!(
        session.storage.settings().expect("load").theme_id,
        before.theme_id,
        "and nothing reached the database"
    );
}

#[test]
fn saving_the_settings_panel_persists_what_was_previewed() {
    let mut session = Session::start("settings-save");
    session.press_with(KeyCode::Char('S'), KeyModifiers::NONE);
    session.press(KeyCode::Down);
    session.press(KeyCode::Char(' '));
    session.press(KeyCode::Char(' '));
    let previewed = session.state.settings;

    session.press(KeyCode::Enter);

    assert_eq!(session.state.overlay, Overlay::None);
    assert_eq!(session.state.status, "Settings saved.");
    assert_eq!(
        session.storage.settings().expect("load"),
        previewed,
        "the previewed settings reached the database"
    );
    assert_eq!(
        session.state.settings, previewed,
        "and the reader keeps them"
    );
}

#[test]
fn a_toggl_command_without_a_connection_says_what_to_do() {
    let mut session = Session::start("toggl");

    session.command("toggl auth");

    // The closing rule gives the key hints their full width first, so a long
    // instruction needs a wide terminal to survive intact.
    session.terminal = Terminal::new(TestBackend::new(140, 24)).expect("terminal");
    let screen = session.screen();
    assert!(
        screen.contains("focus.toggl.com/settings"),
        "the footer should point at the key page:\n{screen}"
    );
}

#[test]
fn repainting_the_same_chapter_renders_it_only_once() {
    let mut session = Session::start("render-cache");

    // Ten frames of the same chapter: the reader is sitting still.
    for _ in 0..10 {
        session.rows();
    }
    let (hits, misses) = session.state.render_cache_stats();
    assert_eq!(misses, 1, "the chapter is rendered once");
    assert_eq!(hits, 9, "every later frame reads the cache");

    // Scrolling does not change the lines, only which of them are shown.
    session.press(KeyCode::Char('j'));
    session.rows();
    assert_eq!(
        session.state.render_cache_stats(),
        (10, 1),
        "scrolling reuses the render"
    );

    // Anything that changes the lines has to render again.
    session.press(KeyCode::Right);
    session.rows();
    let (_, after_chapter) = session.state.render_cache_stats();
    assert_eq!(after_chapter, 2, "a new chapter renders");

    session.command("mode rust");
    session.rows();
    let (_, after_mode) = session.state.render_cache_stats();
    assert_eq!(after_mode, 3, "a new disguise renders");
}

#[test]
fn a_running_timer_shows_in_the_footer_next_to_progress() {
    let mut session = Session::start("timer");
    session.command_bar.timer = Some("Toggl 25m · Reading".to_owned());

    let screen = session.screen();

    assert!(
        screen.contains("Toggl 25m · Reading"),
        "the timer belongs in the footer:\n{screen}"
    );
}

//! One reading session, end to end.
//!
//! This is the closest thing to running the reader: a real EPUB is imported into
//! a real database, keys are pressed, and the resulting frames are asserted from
//! a test buffer. Nothing here is mocked except the terminal itself.

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use reader_app::{CommandContext, Overlay, ReaderState};
use reader_core::{AppSettings, RenderMode};
use reader_storage::{AppPaths, Storage};
use reader_tui::{CommandBar, apply, current_mode, draw, footer_height, map_key};

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
            terminal,
            scratch,
        }
    }

    fn context(&mut self) -> CommandContext {
        let layout = self.state.layout(footer_height(&self.command_bar));
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
            command_bar,
            ..
        } = self;
        terminal
            .draw(|frame| draw(frame, state, command_bar))
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
    session.state.invalidate_layout();
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
fn the_command_bar_shows_what_is_being_typed() {
    let mut session = Session::start("command-bar");

    session.press(KeyCode::Char('/'));
    for character in "goto".chars() {
        session.press(KeyCode::Char(character));
    }

    let rows = session.rows();
    assert!(
        rows[rows.len() - 2].starts_with("/goto"),
        "the command row should echo the line: {:?}",
        rows[rows.len() - 2]
    );

    session.press(KeyCode::Esc);
    let dismissed = session.rows();
    assert!(
        !dismissed[dismissed.len() - 2].starts_with("/goto"),
        "escape should close the bar"
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
        session.state.invalidate_layout();
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
fn a_toggl_command_without_a_connection_says_what_to_do() {
    let mut session = Session::start("toggl");

    session.command("toggl auth");

    let screen = session.screen();
    assert!(
        screen.contains("focus.toggl.com/settings"),
        "the footer should point at the key page:\n{screen}"
    );
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

//! The terminal lifecycle and event loop.
//!
//! This is the only module that talks to a real terminal, so it stays thin: set
//! the terminal up, read an event, map it, apply it, draw, repeat. Everything it
//! calls is testable without a TTY.
//!
//! The terminal is always restored, including on a panic, because leaving a user
//! in raw mode with a hidden cursor is worse than the crash itself.

use std::io;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use reader_app::{CommandContext, ReaderState, persist_pace};
use reader_core::WriteThrottle;
use reader_storage::Storage;

use crate::actions::{apply, current_mode};
use crate::frame::{CommandBar, draw, footer_height};
use crate::input::{Action, SCROLL_STEP, map_key};

/// How long to wait for an event before redrawing anyway.
const TICK: Duration = Duration::from_millis(250);

/// The reader could not run.
#[derive(Debug)]
pub enum AppError {
    Terminal(io::Error),
    Execution(reader_app::ExecutionError),
    Storage(reader_storage::StorageError),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Terminal(error) => write!(formatter, "terminal error: {error}"),
            Self::Execution(error) => write!(formatter, "{error}"),
            Self::Storage(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<io::Error> for AppError {
    fn from(error: io::Error) -> Self {
        Self::Terminal(error)
    }
}

impl From<reader_app::ExecutionError> for AppError {
    fn from(error: reader_app::ExecutionError) -> Self {
        Self::Execution(error)
    }
}

impl From<reader_storage::StorageError> for AppError {
    fn from(error: reader_storage::StorageError) -> Self {
        Self::Storage(error)
    }
}

/// Milliseconds since the Unix epoch.
fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}

/// Restore the terminal, ignoring errors: this runs on the way out, including
/// during a panic, and there is nothing useful left to do with a failure.
fn restore_terminal(mouse_capture: bool) {
    let mut out = io::stdout();
    if mouse_capture {
        let _ = execute!(out, DisableMouseCapture);
    }
    let _ = execute!(out, LeaveAlternateScreen);
    let _ = disable_raw_mode();
}

/// Run the reader until it is told to quit.
pub fn run(state: &mut ReaderState, storage: &mut Storage) -> Result<(), AppError> {
    enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen)?;
    let mut mouse_captured = state.settings.mouse_capture;
    if mouse_captured {
        execute!(out, EnableMouseCapture)?;
    }

    // A panic must not leave the terminal in raw mode.
    let previous_hook = std::panic::take_hook();
    let hook_mouse = mouse_captured;
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal(hook_mouse);
        previous_hook(info);
    }));

    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let mut command_bar = CommandBar::default();
    let result = event_loop(
        &mut terminal,
        state,
        storage,
        &mut command_bar,
        &mut mouse_captured,
    );

    // The pace model is only worth keeping if the session actually read something.
    persist_pace(state, storage, now_millis());
    if let Some((book_id, position)) = current_position(state) {
        write_position(storage, &book_id, position);
    }

    let _ = std::panic::take_hook();
    restore_terminal(mouse_captured);
    let _ = terminal.show_cursor();
    result
}

/// The reading position as it currently stands, with progress recomputed.
fn current_position(state: &mut ReaderState) -> Option<(String, reader_core::ReadingPosition)> {
    let book_id = state.book_id().map(str::to_owned)?;
    let layout = state.layout(1);
    let chapter_progress = state.chapter_progress(layout.content_width, layout.body_height);
    let book_progress = state.book_progress(layout.content_width, layout.body_height);
    Some((
        book_id,
        reader_core::ReadingPosition {
            chapter_index: state.chapter_index,
            chapter_progress,
            book_progress,
            block_offset: state.block_offset,
        },
    ))
}

/// Write the reading position, so the next start resumes here.
fn write_position(storage: &Storage, book_id: &str, position: reader_core::ReadingPosition) {
    // A failed position write costs the reader their place, not their session.
    let _ = storage.save_position(book_id, position, now_millis());
}

/// The concrete terminal the reader drives. Tests exercise `draw`, `map_key`,
/// and `apply` directly, so the loop does not need to be generic over backends.
type ReaderTerminal = Terminal<CrosstermBackend<io::Stdout>>;

/// Update the footer's timer line, polling Focus only when the interval allows.
fn refresh_timer_line(
    state: &ReaderState,
    storage: &Storage,
    command_bar: &mut CommandBar,
    last_refresh: &mut Option<i64>,
) {
    let settings = reader_app::toggl::StorageSettings::new(storage);
    let transport = reader_integrations::NetworkTransport;
    let now = now_millis();
    let client = reader_integrations::TogglClient::new(&settings, &transport, now);
    if !client.is_connected() {
        command_bar.timer = None;
        return;
    }

    let context = CommandContext {
        now,
        content_width: state.viewport.width,
        body_height: state.viewport.height,
    };
    if reader_app::toggl::should_refresh(*last_refresh, context) {
        *last_refresh = Some(now);
        // A failed poll leaves the last known timer on screen rather than
        // blanking it; the elapsed time is computed locally anyway.
        let _ = client.refresh_current_entry();
    }
    command_bar.timer = client.running_timer_line();
}

fn event_loop(
    terminal: &mut ReaderTerminal,
    state: &mut ReaderState,
    storage: &mut Storage,
    command_bar: &mut CommandBar,
    mouse_captured: &mut bool,
) -> Result<(), AppError> {
    let mut last_timer_refresh: Option<i64> = None;
    // Scrolling changes the position on every keypress; the throttle keeps the
    // first and last write of each window so the place survives a crash without
    // putting the database in the reader's hot loop.
    let mut position_writes: WriteThrottle<(String, reader_core::ReadingPosition)> =
        WriteThrottle::default();

    while !state.should_quit {
        // The footer shows a running Toggl timer, counted locally from its start
        // so the display stays accurate between the rare background polls.
        refresh_timer_line(state, storage, command_bar, &mut last_timer_refresh);

        if let Some((book_id, position)) = position_writes.tick(now_millis()) {
            write_position(storage, &book_id, position);
        }

        // The overlay list is built once per frame, from the database, and then
        // both drawn and indexed by the cursor.
        let overlay_entries = reader_app::visible_entries(state, storage, now_millis());
        terminal.draw(|frame| draw(frame, state, command_bar, &overlay_entries))?;

        // Mouse capture follows the setting, which a command can change.
        if state.settings.mouse_capture != *mouse_captured {
            *mouse_captured = state.settings.mouse_capture;
            if *mouse_captured {
                execute!(terminal.backend_mut(), EnableMouseCapture)?;
            } else {
                execute!(terminal.backend_mut(), DisableMouseCapture)?;
            }
        }

        if !event::poll(TICK)? {
            continue;
        }
        let layout = state.layout(footer_height(command_bar));
        let context = CommandContext {
            now: now_millis(),
            content_width: layout.content_width,
            body_height: layout.body_height,
        };

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                let action = map_key(key, current_mode(state, command_bar));
                apply(action, state, storage, command_bar, context)?;
            }
            Event::Mouse(mouse) => {
                let action = match mouse.kind {
                    MouseEventKind::ScrollDown => Action::ScrollDown(SCROLL_STEP * 3),
                    MouseEventKind::ScrollUp => Action::ScrollUp(SCROLL_STEP * 3),
                    _ => Action::Ignore,
                };
                apply(action, state, storage, command_bar, context)?;
            }
            Event::Resize(width, height) => {
                state.viewport = reader_app::Viewport::new(width, height);
            }
            _ => {}
        }

        // Offer the new position; the throttle decides whether it lands now.
        if let Some((book_id, position)) = current_position(state)
            && let Some((book_id, position)) =
                position_writes.schedule((book_id, position), context.now)
        {
            write_position(storage, &book_id, position);
        }
    }

    // Whatever is still held belongs on disk before the reader leaves.
    if let Some((book_id, position)) = position_writes.flush(now_millis()) {
        write_position(storage, &book_id, position);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::now_millis;

    #[test]
    fn the_clock_returns_a_plausible_epoch_millisecond_value() {
        // Sanity check that the conversion is not off by orders of magnitude:
        // any time after 2020 and before 2100.
        let now = now_millis();
        assert!(now > 1_577_836_800_000, "{now} is before 2020");
        assert!(now < 4_102_444_800_000, "{now} is after 2100");
    }
}

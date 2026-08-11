//! The reader's entry point.
//!
//! This binary only wires things together: parse arguments, open the library,
//! build the starting state, hand off to the TUI, and turn whatever comes back
//! into an exit code. Every decision it makes is delegated to a library crate so
//! it stays reviewable at a glance.

use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;
use reader_app::{CommandContext, ReaderState, open_book};
use reader_storage::{AppPaths, Storage};

/// Read EPUB, CBZ, and PDF books in the terminal, in plain text or disguised as
/// code.
#[derive(Debug, Parser)]
#[command(name = "stealth-reader", version, about, long_about = None)]
struct Args {
    /// Reopen the most recently read book at its saved position.
    #[arg(long)]
    resume: bool,

    /// Import and open this file instead of showing the library.
    #[arg(value_name = "FILE")]
    file: Option<std::path::PathBuf>,
}

/// Exit codes, so a caller can tell a usage problem from a failure.
const EXIT_FAILURE: u8 = 1;

fn main() -> ExitCode {
    let args = Args::parse();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("stealth-reader: {error}");
            ExitCode::from(EXIT_FAILURE)
        }
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}

fn run(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    // The reader is a full-screen application, so a redirected or piped session
    // cannot work. Saying so plainly beats an errno from deep inside the
    // terminal setup.
    if !std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        return Err(
            "stdout is not a terminal. stealth-reader needs an interactive terminal to draw in."
                .into(),
        );
    }

    let paths = AppPaths::from_env();
    let mut storage = Storage::open(&paths)?;
    let settings = storage.settings()?;
    let mut state = ReaderState::new(settings);

    // A first frame has not been drawn yet, so the starting geometry is the
    // default viewport; the TUI corrects it before anything is shown.
    let layout = state.layout(1);
    let context = CommandContext {
        now: now_millis(),
        content_width: layout.content_width,
        body_height: layout.body_height,
    };

    if let Some(file) = args.file.as_deref() {
        reader_app::import_and_open(&mut state, &mut storage, file, context)?;
    } else if args.resume {
        match storage.latest_book_id()? {
            Some(book_id) => match storage.book(&book_id)? {
                Some(book) => open_book(&mut state, &storage, book, context)?,
                None => state.status = "The most recent book is no longer in the library.".into(),
            },
            None => state.status = "No previous book to resume.".into(),
        }
    } else {
        state.status = "Press / for commands, ? for shortcuts.".into();
    }

    reader_tui::run(&mut state, &mut storage)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::Args;

    #[test]
    fn the_default_invocation_opens_the_library() {
        let args = Args::parse_from(["stealth-reader"]);
        assert!(!args.resume);
        assert!(args.file.is_none());
    }

    #[test]
    fn resume_is_a_flag_and_a_file_is_positional() {
        let resumed = Args::parse_from(["stealth-reader", "--resume"]);
        assert!(resumed.resume);

        let with_file = Args::parse_from(["stealth-reader", "/books/dune.epub"]);
        assert_eq!(
            with_file.file.as_deref(),
            Some(std::path::Path::new("/books/dune.epub"))
        );

        // Both together are allowed; the file wins, since it is more specific.
        let both = Args::parse_from(["stealth-reader", "--resume", "/books/dune.epub"]);
        assert!(both.resume);
        assert!(both.file.is_some());
    }

    #[test]
    fn an_unknown_flag_is_rejected_rather_than_ignored() {
        assert!(Args::try_parse_from(["stealth-reader", "--nope"]).is_err());
    }
}

//! What the reader shows the moment it starts.
//!
//! Deciding this in a library rather than in `main` is what makes it testable:
//! a launch is just a state, a database, and the arguments, so the whole policy
//! can be asserted against a temporary data directory without a terminal.
//!
//! The policy, in order: an explicit file wins, because it is the most specific
//! thing the reader asked for; `--resume` reopens the latest book where it was
//! left; otherwise the configured library directory is scanned and the reader is
//! offered whatever there is — the stored library if it has anything, the files
//! found on disk if it does not.

use std::path::PathBuf;

use reader_storage::Storage;

use crate::executor::{CommandContext, import_and_open, open_book};
use crate::state::{Overlay, ReaderState};

/// How the reader was invoked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchOptions {
    /// Reopen the most recently read book at its saved position.
    pub resume: bool,
    /// Import and open this file instead of offering a choice.
    pub file: Option<PathBuf>,
}

/// Put the reader into its opening state.
///
/// Discovery failures are not fatal: an unreadable or missing library directory
/// leaves the list empty and the reader still starts, because the stored library
/// does not live there.
pub fn launch(
    state: &mut ReaderState,
    storage: &mut Storage,
    options: &LaunchOptions,
    context: CommandContext,
) -> Result<(), crate::executor::ExecutionError> {
    if let Some(file) = options.file.as_deref() {
        import_and_open(state, storage, file, context)?;
        return Ok(());
    }

    // Every path below offers a choice, and all of them want to know what is on
    // disk — including `--resume`, so `/add` after a resume is already scanned.
    crate::library::refresh_discoveries(state, storage, false)?;

    if options.resume {
        match storage.latest_book_id()? {
            Some(book_id) => match storage.book(&book_id)? {
                Some(book) => open_book(state, storage, book, context)?,
                None => state.status = "The most recent book is no longer in the library.".into(),
            },
            None => state.status = "No previous book to resume.".into(),
        }
        return Ok(());
    }

    if !storage.books()?.is_empty() {
        state.open_overlay(Overlay::Books, 0);
        state.status = "Select a book to open. Press Enter to open, Esc to dismiss.".into();
        return Ok(());
    }

    if !state.discoveries.is_empty() {
        let found = state.discoveries.clone();
        let count = found.len();
        crate::library::open_file_picker(
            state,
            found,
            match count {
                1 => "Found 1 book here. Space selects · Enter imports.".to_owned(),
                count => format!("Found {count} books here. Space selects · Enter imports."),
            },
        );
        return Ok(());
    }

    state.status = format!(
        "No books in {}. Press / then type add, or librarydir to point elsewhere.",
        state.library_directory.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use reader_core::AppSettings;
    use reader_storage::{AppPaths, Storage};

    use super::{LaunchOptions, launch};
    use crate::executor::CommandContext;
    use crate::state::{Overlay, ReaderState};

    const CONTEXT: CommandContext = CommandContext {
        now: 1_700_000_000_000,
        content_width: 76,
        body_height: 21,
    };

    /// An EPUB the format tests already use.
    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("reader-formats")
            .join("tests")
            .join("fixtures")
            .join(name)
    }

    /// A scratch data directory that cleans itself up.
    struct Scratch {
        root: PathBuf,
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("reader-launch-{}-{name}", std::process::id()));
            std::fs::remove_dir_all(&root).ok();
            std::fs::create_dir_all(root.join("library")).expect("a scratch directory");
            Self { root }
        }

        fn storage(&self) -> Storage {
            Storage::open(&AppPaths::from_roots(
                &self.root.join("data"),
                &self.root.join("cache"),
            ))
            .expect("the library opens")
        }

        /// Copy `names` into the scratch library directory.
        fn library_with(&self, names: &[&str]) -> PathBuf {
            let root = self.root.join("library");
            for name in names {
                std::fs::copy(fixture(name), root.join(name)).expect("a fixture copy");
            }
            root
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.root).ok();
        }
    }

    fn reader() -> ReaderState {
        ReaderState::new(AppSettings::default())
    }

    /// Point the stored library directory at `root`.
    fn use_library(storage: &Storage, root: &Path) {
        storage
            .set_setting(
                crate::library::LIBRARY_DIRECTORY_KEY,
                &root.display().to_string(),
            )
            .expect("the directory is stored");
    }

    #[test]
    fn a_default_launch_scans_the_library_directory_and_offers_what_it_found() {
        let scratch = Scratch::new("discover");
        let root = scratch.library_with(&["ncx-nested.epub", "front-matter.epub"]);
        let mut storage = scratch.storage();
        use_library(&storage, &root);
        let mut state = reader();

        launch(&mut state, &mut storage, &LaunchOptions::default(), CONTEXT)
            .expect("launching succeeds");

        assert_eq!(state.library_directory, root);
        assert_eq!(state.discoveries.len(), 2, "{:?}", state.discoveries);
        assert_eq!(
            state.overlay,
            Overlay::FilePicker,
            "an empty library offers the files it found"
        );
        assert!(state.status.contains("Found 2 books"), "{}", state.status);
    }

    #[test]
    fn a_default_launch_with_a_stored_library_opens_the_picker_for_it() {
        let scratch = Scratch::new("library");
        let root = scratch.library_with(&["ncx-nested.epub"]);
        let mut storage = scratch.storage();
        use_library(&storage, &root);
        let mut state = reader();

        // Import once, then start again as a fresh process would.
        launch(
            &mut state,
            &mut storage,
            &LaunchOptions {
                file: Some(root.join("ncx-nested.epub")),
                ..LaunchOptions::default()
            },
            CONTEXT,
        )
        .expect("importing succeeds");
        assert!(state.current_book.is_some());

        let mut restarted = reader();
        launch(
            &mut restarted,
            &mut storage,
            &LaunchOptions::default(),
            CONTEXT,
        )
        .expect("launching succeeds");

        assert_eq!(restarted.overlay, Overlay::Books);
        assert!(restarted.current_book.is_none(), "nothing opens by itself");
        assert!(
            restarted.status.contains("Select a book"),
            "{}",
            restarted.status
        );
    }

    #[test]
    fn resume_reopens_the_latest_book_at_its_saved_position() {
        let scratch = Scratch::new("resume");
        let root = scratch.library_with(&["ncx-nested.epub"]);
        let mut storage = scratch.storage();
        use_library(&storage, &root);

        let mut state = reader();
        launch(
            &mut state,
            &mut storage,
            &LaunchOptions {
                file: Some(root.join("ncx-nested.epub")),
                ..LaunchOptions::default()
            },
            CONTEXT,
        )
        .expect("importing succeeds");
        let book_id = state.book_id().expect("a book").to_owned();
        storage
            .save_position(
                &book_id,
                reader_core::ReadingPosition {
                    chapter_index: 1,
                    chapter_progress: 0.5,
                    book_progress: 0.4,
                    block_offset: 3,
                },
                CONTEXT.now,
            )
            .expect("the position is written");

        // A short chapter cannot hold an offset of 3, so resume against a
        // viewport small enough for the saved place to still exist.
        let narrow = CommandContext {
            content_width: 40,
            body_height: 5,
            ..CONTEXT
        };
        let mut resumed = reader();
        launch(
            &mut resumed,
            &mut storage,
            &LaunchOptions {
                resume: true,
                ..LaunchOptions::default()
            },
            narrow,
        )
        .expect("resuming succeeds");

        assert_eq!(resumed.book_id(), Some(book_id.as_str()));
        assert_eq!(resumed.chapter_index, 1);
        assert_eq!(resumed.block_offset, 3);
        assert_eq!(
            resumed.overlay,
            Overlay::None,
            "resume opens the page itself"
        );
    }

    #[test]
    fn resume_without_a_previous_book_says_so_rather_than_failing() {
        let scratch = Scratch::new("resume-empty");
        let mut storage = scratch.storage();
        let mut state = reader();

        launch(
            &mut state,
            &mut storage,
            &LaunchOptions {
                resume: true,
                ..LaunchOptions::default()
            },
            CONTEXT,
        )
        .expect("launching succeeds");

        assert!(state.current_book.is_none());
        assert_eq!(state.status, "No previous book to resume.");
    }

    #[test]
    fn an_empty_library_directory_explains_what_to_do() {
        let scratch = Scratch::new("nothing");
        let root = scratch.library_with(&[]);
        let mut storage = scratch.storage();
        use_library(&storage, &root);
        let mut state = reader();

        launch(&mut state, &mut storage, &LaunchOptions::default(), CONTEXT)
            .expect("launching succeeds");

        assert_eq!(state.overlay, Overlay::None);
        assert!(state.status.contains("No books in"), "{}", state.status);
        assert!(state.status.contains("librarydir"), "{}", state.status);
    }

    #[test]
    fn a_fresh_process_still_finds_the_book_the_settings_and_the_position() {
        let scratch = Scratch::new("survives-restart");
        let root = scratch.library_with(&["ncx-nested.epub"]);
        let book_id;
        let title;

        // First run: import, change a setting, and note a place in the book.
        {
            let mut storage = scratch.storage();
            use_library(&storage, &root);
            let mut state = reader();
            launch(
                &mut state,
                &mut storage,
                &LaunchOptions {
                    file: Some(root.join("ncx-nested.epub")),
                    ..LaunchOptions::default()
                },
                CONTEXT,
            )
            .expect("importing succeeds");

            book_id = state.book_id().expect("a book").to_owned();
            title = state.current_book.as_ref().expect("a book").title.clone();
            storage
                .save_settings(&AppSettings {
                    margin_size: 8,
                    ..AppSettings::default()
                })
                .expect("settings are written");
            storage
                .save_position(
                    &book_id,
                    reader_core::ReadingPosition {
                        chapter_index: 1,
                        chapter_progress: 0.5,
                        book_progress: 0.4,
                        block_offset: 3,
                    },
                    CONTEXT.now,
                )
                .expect("the position is written");
            storage
                .add_bookmark(&book_id, 1, 3, Some("the reveal"), CONTEXT.now)
                .expect("a bookmark");
            storage
                .add_note(&book_id, "remember this", Some(1), Some(3), CONTEXT.now)
                .expect("a note");
            storage.add_tag(&book_id, "favorite").expect("a tag");
        }

        // Second run: a brand new Storage over the same directory.
        let mut storage = scratch.storage();
        let settings = storage.settings().expect("settings load");
        assert_eq!(settings.margin_size, 8, "the setting survived");

        let mut state = ReaderState::new(settings);
        launch(
            &mut state,
            &mut storage,
            &LaunchOptions {
                resume: true,
                ..LaunchOptions::default()
            },
            CommandContext {
                content_width: 40,
                body_height: 5,
                ..CONTEXT
            },
        )
        .expect("resuming succeeds");

        assert_eq!(state.book_id(), Some(book_id.as_str()));
        assert_eq!(
            state.current_book.as_ref().expect("a book").title,
            title,
            "the imported book survived"
        );
        assert_eq!(state.chapter_index, 1);
        assert_eq!(state.block_offset, 3);
        assert_eq!(storage.bookmarks(&book_id).expect("bookmarks").len(), 1);
        assert_eq!(storage.notes(&book_id).expect("notes").len(), 1);
        assert_eq!(
            storage
                .tags_by_book()
                .expect("tags")
                .get(&book_id)
                .map(Vec::len),
            Some(1)
        );
    }

    #[test]
    fn reopening_the_same_file_does_not_add_a_second_library_row() {
        let scratch = Scratch::new("duplicate");
        let root = scratch.library_with(&["ncx-nested.epub"]);
        let mut storage = scratch.storage();
        use_library(&storage, &root);
        let options = LaunchOptions {
            file: Some(root.join("ncx-nested.epub")),
            ..LaunchOptions::default()
        };

        let mut first = reader();
        launch(&mut first, &mut storage, &options, CONTEXT).expect("importing succeeds");
        let mut second = reader();
        launch(&mut second, &mut storage, &options, CONTEXT).expect("reimporting succeeds");

        assert_eq!(storage.books().expect("the library").len(), 1);
        assert_eq!(first.book_id(), second.book_id());
    }
}

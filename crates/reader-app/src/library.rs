//! Commands that touch the filesystem: importing, the library directory, and
//! export/import of reading state.
//!
//! These are separated from the rest of the executor because they are the only
//! commands that can fail for reasons outside the reader — a missing file, a
//! directory that is not one, a malformed export — and each of those has to
//! become a status message rather than a crash.

use std::path::{Path, PathBuf};

use reader_core::command::ParsedCommand;
use reader_formats::{Discovery, discover_books, resolve_library_directory};
use reader_storage::{ExportData, Storage};

use crate::executor::{CommandContext, import_and_open};
use crate::state::{Overlay, ReaderState};

/// The settings key holding the configured library directory.
pub const LIBRARY_DIRECTORY_KEY: &str = "libraryDirectory";
/// Default file name for `/export` and `/import`.
pub const DEFAULT_EXPORT_FILE: &str = "stealth-reader-export.json";

/// Re-read the library directory setting and rescan it.
pub fn refresh_discoveries(
    state: &mut ReaderState,
    storage: &Storage,
    use_process_cwd: bool,
) -> Result<(), reader_storage::StorageError> {
    let configured = if use_process_cwd {
        None
    } else {
        storage.setting(LIBRARY_DIRECTORY_KEY)?
    };
    let cwd = std::env::current_dir().unwrap_or_default();
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| cwd.clone());
    state.library_directory = resolve_library_directory(configured.as_deref(), &cwd, &home);
    state.discoveries = discover_books(&state.library_directory).unwrap_or_default();
    Ok(())
}

/// Open the file picker over `items`.
pub fn open_file_picker(state: &mut ReaderState, items: Vec<Discovery>, status: String) {
    state.overlay = Overlay::FilePicker;
    state.overlay_cursor = 0;
    state.discoveries = items;
    state.status = status;
}

fn matching_discoveries(discoveries: &[Discovery], query: &str) -> Vec<Discovery> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return discoveries.to_vec();
    }
    discoveries
        .iter()
        .filter(|item| item.file_name.to_lowercase().contains(&needle))
        .cloned()
        .collect()
}

/// `/add [path] [--cwd] [--force]`
pub fn add(
    state: &mut ReaderState,
    storage: &mut Storage,
    parsed: &ParsedCommand,
    context: CommandContext,
) -> Result<(), reader_storage::StorageError> {
    refresh_discoveries(state, storage, parsed.has_flag("cwd"))?;

    if parsed.has_flag("cwd") || parsed.args.is_empty() {
        let items = state.discoveries.clone();
        let status = if items.is_empty() {
            "No books detected in the current directory.".to_owned()
        } else {
            "Opened file picker.".to_owned()
        };
        open_file_picker(state, items, status);
        return Ok(());
    }

    let target = parsed.joined_args();
    let explicit = state.library_directory.join(&target);
    if explicit.is_file() {
        let _ = import_and_open(state, storage, &explicit, context);
        return Ok(());
    }

    let matches = matching_discoveries(&state.discoveries, &target);
    if let [single] = matches.as_slice() {
        let path = single.path.clone();
        let _ = import_and_open(state, storage, &path, context);
        return Ok(());
    }

    let status = if matches.is_empty() {
        format!("No books matched \"{target}\".")
    } else {
        format!("Opened file picker for \"{target}\".")
    };
    open_file_picker(state, matches, status);
    Ok(())
}

/// `/librarydir [path] [--cwd]`
pub fn library_directory(
    state: &mut ReaderState,
    storage: &Storage,
    parsed: &ParsedCommand,
) -> Result<(), reader_storage::StorageError> {
    let reset = parsed.has_flag("cwd");
    let requested = parsed.joined_args();
    let requested = requested.trim();
    let cwd = std::env::current_dir().unwrap_or_default();
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| cwd.clone());

    if !reset && requested.is_empty() {
        let configured = storage.setting(LIBRARY_DIRECTORY_KEY)?;
        let active = resolve_library_directory(configured.as_deref(), &cwd, &home);
        state.status = format!("Library directory: {}", active.display());
        return Ok(());
    }

    let directory =
        resolve_library_directory(if reset { None } else { Some(requested) }, &cwd, &home);
    if !directory.exists() {
        state.status = format!("Library directory not found: {}", directory.display());
        return Ok(());
    }
    if !directory.is_dir() {
        state.status = format!("Library path is not a directory: {}", directory.display());
        return Ok(());
    }

    let discoveries = discover_books(&directory).unwrap_or_default();
    // Resetting stores an empty value, which resolves back to the working
    // directory on the next start.
    let stored = if reset {
        String::new()
    } else {
        directory.display().to_string()
    };
    storage.set_setting(LIBRARY_DIRECTORY_KEY, &stored)?;
    state.status = format!(
        "Library directory set to {} · {} book(s) found.",
        directory.display(),
        discoveries.len()
    );
    state.library_directory = directory;
    state.discoveries = discoveries;
    Ok(())
}

fn export_path(state: &ReaderState, parsed: &ParsedCommand) -> PathBuf {
    match parsed.args.first() {
        Some(argument) => state.library_directory.join(argument),
        None => state.library_directory.join(DEFAULT_EXPORT_FILE),
    }
}

/// Path shown to the reader: relative to the library directory when possible.
fn display_path(state: &ReaderState, path: &Path) -> String {
    path.strip_prefix(&state.library_directory)
        .unwrap_or(path)
        .display()
        .to_string()
}

/// `/export [path]`
pub fn export(
    state: &mut ReaderState,
    storage: &Storage,
    parsed: &ParsedCommand,
    context: CommandContext,
) -> Result<(), reader_storage::StorageError> {
    let path = export_path(state, parsed);
    let data = storage.export_all(context.now)?;
    let json = match serde_json::to_string_pretty(&data) {
        Ok(json) => json,
        Err(error) => {
            state.status = format!("Export failed: {error}");
            return Ok(());
        }
    };
    if let Err(error) = std::fs::write(&path, json) {
        state.status = format!("Export failed: {error}");
        return Ok(());
    }

    let books: std::collections::BTreeSet<&str> = data
        .positions
        .iter()
        .map(|position| position.book_import_hash.as_str())
        .collect();
    state.status = format!(
        "Exported {} book(s) — {} position(s), {} bookmark(s), {} note(s), {} tag(s) → {}",
        books.len(),
        data.positions.len(),
        data.bookmarks.len(),
        data.notes.len(),
        data.tags.len(),
        display_path(state, &path)
    );
    Ok(())
}

/// `/import [path]`
pub fn import(
    state: &mut ReaderState,
    storage: &mut Storage,
    parsed: &ParsedCommand,
) -> Result<(), reader_storage::StorageError> {
    let path = export_path(state, parsed);
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(_) => {
            state.status = format!("File not found: {}", path.display());
            return Ok(());
        }
    };
    let data: ExportData = match serde_json::from_str(&raw) {
        Ok(data) => data,
        Err(error) => {
            state.status = format!("Import failed: {error}");
            return Ok(());
        }
    };
    if data.version != 1 {
        state.status = "Unsupported export format version.".to_owned();
        return Ok(());
    }
    match storage.import_merge(&data) {
        Ok(summary) => {
            state.status = format!(
                "Imported: {} position(s) updated, {} bookmark(s) added, {} note(s) added, {} tag(s) added",
                summary.positions_updated,
                summary.bookmarks_added,
                summary.notes_added,
                summary.tags_added
            );
        }
        Err(error) => state.status = format!("Import failed: {error}"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use reader_core::command::parse_slash_command;
    use reader_core::{AppSettings, RenderMode};
    use reader_storage::Storage;

    use super::{add, export, import, library_directory};
    use crate::executor::CommandContext;
    use crate::state::{Overlay, ReaderState};

    const CONTEXT: CommandContext = CommandContext {
        now: 1_700_000_000_000,
        content_width: 80,
        body_height: 20,
    };

    fn scratch(name: &str) -> std::path::PathBuf {
        let directory =
            std::env::temp_dir().join(format!("reader-app-library-{}-{name}", std::process::id()));
        std::fs::remove_dir_all(&directory).ok();
        std::fs::create_dir_all(&directory).expect("scratch directory");
        directory
    }

    fn reader(directory: &std::path::Path) -> (ReaderState, Storage) {
        let mut state = ReaderState::new(AppSettings::default());
        state.library_directory = directory.to_path_buf();
        (state, Storage::open_in_memory().expect("database"))
    }

    fn run(
        state: &mut ReaderState,
        storage: &mut Storage,
        command: &str,
    ) -> Result<(), reader_storage::StorageError> {
        let parsed = parse_slash_command(command).expect("valid command");
        match parsed.name {
            "add" => add(state, storage, &parsed, CONTEXT),
            "librarydir" => library_directory(state, storage, &parsed),
            "export" => export(state, storage, &parsed, CONTEXT),
            "import" => import(state, storage, &parsed),
            other => panic!("unexpected command {other}"),
        }
    }

    #[test]
    fn the_library_directory_is_reported_configured_and_validated() {
        let directory = scratch("librarydir");
        let (mut state, mut storage) = reader(&directory);

        run(&mut state, &mut storage, "/librarydir").expect("report");
        assert!(
            state.status.starts_with("Library directory: "),
            "{}",
            state.status
        );

        let books = directory.join("books");
        std::fs::create_dir_all(&books).expect("books directory");
        std::fs::write(books.join("a.epub"), b"").expect("fixture");
        run(
            &mut state,
            &mut storage,
            &format!("/librarydir {}", books.display()),
        )
        .expect("configure");
        assert!(state.status.contains("1 book(s) found"), "{}", state.status);
        assert_eq!(state.library_directory, books);
        assert_eq!(
            storage.setting("libraryDirectory").expect("read"),
            Some(books.display().to_string())
        );

        run(&mut state, &mut storage, "/librarydir /nonexistent/place").expect("missing");
        assert!(
            state.status.starts_with("Library directory not found:"),
            "{}",
            state.status
        );

        let file = books.join("a.epub");
        run(
            &mut state,
            &mut storage,
            &format!("/librarydir {}", file.display()),
        )
        .expect("not a directory");
        assert!(
            state.status.starts_with("Library path is not a directory:"),
            "{}",
            state.status
        );
        std::fs::remove_dir_all(directory).ok();
    }

    #[test]
    fn add_without_a_path_opens_the_picker_and_reports_an_empty_directory() {
        let directory = scratch("add-empty");
        let (mut state, mut storage) = reader(&directory);
        storage
            .set_setting("libraryDirectory", &directory.display().to_string())
            .expect("configure");

        run(&mut state, &mut storage, "/add").expect("add");

        assert_eq!(state.overlay, Overlay::FilePicker);
        assert_eq!(state.status, "No books detected in the current directory.");
        std::fs::remove_dir_all(directory).ok();
    }

    #[test]
    fn add_with_a_query_that_matches_nothing_says_so() {
        let directory = scratch("add-nomatch");
        let (mut state, mut storage) = reader(&directory);
        storage
            .set_setting("libraryDirectory", &directory.display().to_string())
            .expect("configure");
        std::fs::write(directory.join("dune.epub"), b"").expect("fixture");

        run(&mut state, &mut storage, "/add blindsight").expect("add");

        assert_eq!(state.overlay, Overlay::FilePicker);
        assert_eq!(state.status, "No books matched \"blindsight\".");
        std::fs::remove_dir_all(directory).ok();
    }

    #[test]
    fn add_reports_a_failed_import_without_opening_anything() {
        let directory = scratch("add-broken");
        let (mut state, mut storage) = reader(&directory);
        storage
            .set_setting("libraryDirectory", &directory.display().to_string())
            .expect("configure");
        std::fs::write(directory.join("broken.epub"), b"not a zip").expect("fixture");

        run(&mut state, &mut storage, "/add broken.epub").expect("add");

        assert!(
            state.status.starts_with("Import failed:"),
            "{}",
            state.status
        );
        assert!(state.current_book.is_none());
        std::fs::remove_dir_all(directory).ok();
    }

    #[test]
    fn export_writes_a_file_and_import_merges_it_back() {
        let directory = scratch("export");
        let (mut state, mut storage) = reader(&directory);
        let book = reader_core::CanonicalBook {
            id: "b1".into(),
            title: "Book".into(),
            author: "Author".into(),
            source_path: "/books/b1.epub".into(),
            import_hash: "hash-1".into(),
            parser_version: Some(3),
            diagnostics: Vec::new(),
            chapters: Vec::new(),
            cover_path: None,
        };
        storage
            .save_book(&book, RenderMode::Code, 1_000)
            .expect("save");
        storage
            .save_position(
                "b1",
                reader_core::ReadingPosition {
                    chapter_index: 1,
                    chapter_progress: 0.5,
                    book_progress: 0.5,
                    block_offset: 3,
                },
                1_000,
            )
            .expect("position");
        storage.add_tag("b1", "fiction").expect("tag");

        run(&mut state, &mut storage, "/export").expect("export");
        let written = directory.join(super::DEFAULT_EXPORT_FILE);
        assert!(written.is_file());
        assert!(
            state.status.contains("Exported 1 book(s)"),
            "{}",
            state.status
        );
        assert!(
            state.status.ends_with(super::DEFAULT_EXPORT_FILE),
            "{}",
            state.status
        );

        run(&mut state, &mut storage, "/import").expect("import");
        assert!(state.status.starts_with("Imported: "), "{}", state.status);
        std::fs::remove_dir_all(directory).ok();
    }

    #[test]
    fn import_refuses_a_missing_malformed_or_future_file() {
        let directory = scratch("import-bad");
        let (mut state, mut storage) = reader(&directory);

        run(&mut state, &mut storage, "/import").expect("missing");
        assert!(
            state.status.starts_with("File not found:"),
            "{}",
            state.status
        );

        std::fs::write(directory.join("broken.json"), b"{not json").expect("fixture");
        run(&mut state, &mut storage, "/import broken.json").expect("malformed");
        assert!(
            state.status.starts_with("Import failed:"),
            "{}",
            state.status
        );

        std::fs::write(
            directory.join("future.json"),
            br#"{"version":2,"exportedAt":"2023-11-14T22:13:20.000Z","positions":[],"bookmarks":[],"notes":[],"tags":[]}"#,
        )
        .expect("fixture");
        run(&mut state, &mut storage, "/import future.json").expect("future");
        assert_eq!(state.status, "Unsupported export format version.");
        std::fs::remove_dir_all(directory).ok();
    }
}

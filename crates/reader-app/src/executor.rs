//! Running slash commands against the reader state.
//!
//! Every command reports through `state.status`; a command that cannot do what
//! was asked says so there rather than failing the frame. Navigation commands
//! bracket their move with history entries, so stepping back returns to where the
//! reader was rather than to where the command left them.
//!
//! Commands never read the clock or the terminal: both arrive as parameters, so
//! the whole executor is deterministic under test.

use reader_core::command::{CommandError, ParsedCommand, parse_slash_command};
use reader_core::theme::AppearanceThemeId;
use reader_core::{
    CanonicalBook, CodeDensity, CodeLanguage, ColorSchemeId, LibrarySortKey, ProgressVisibility,
    RenderMode,
};
use reader_storage::{Storage, StorageError};

use crate::state::{Overlay, ReaderState, SearchHit, SearchState};

/// What a command needs from outside the reader: the clock, and the geometry the
/// terminal currently has.
#[derive(Debug, Clone, Copy)]
pub struct CommandContext {
    pub now: i64,
    pub content_width: u16,
    pub body_height: u16,
}

/// A command failed in a way the reader cannot recover from.
#[derive(Debug)]
pub enum ExecutionError {
    Storage(StorageError),
}

impl std::fmt::Display for ExecutionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Storage(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for ExecutionError {}

impl From<StorageError> for ExecutionError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

type Result<T> = std::result::Result<T, ExecutionError>;

/// Open a book, restoring its saved position and pace.
pub fn open_book(
    state: &mut ReaderState,
    storage: &Storage,
    book: CanonicalBook,
    context: CommandContext,
) -> Result<()> {
    // The pace model of the book being closed is worth keeping.
    if state.current_book.is_some() {
        persist_pace(state, storage, context.now);
    }

    let position = storage.position(&book.id)?;
    state.chapter_index = position
        .map(|position| position.chapter_index)
        .unwrap_or(0)
        .min(book.chapters.len().saturating_sub(1));
    state.block_offset = position.map(|position| position.block_offset).unwrap_or(0);
    state.status = format!("Opened {}", book.title);
    state.focus_mode = false;
    state.focus_block_index = 0;
    state.search = None;
    state.nav_history.clear();
    state.nav_history_cursor = None;
    state.clear_render_caches();
    load_pace(state, storage, &book.id)?;
    state.current_book = Some(book);
    state.clamp_offset(context.content_width, context.body_height);
    Ok(())
}

/// Write the pace model back to storage. Failure is ignored: estimates stay
/// usable from memory, and a failed write must not interrupt reading.
pub fn persist_pace(state: &ReaderState, storage: &Storage, now: i64) {
    let _ = storage.set_setting("globalWpm", &state.pace.global_wpm.to_string());
    let _ = storage.set_setting("globalActiveMs", &state.pace.global_active_ms.to_string());
    if let Some(book_id) = state.pace.book_id.as_deref() {
        let _ = storage.save_reading_pace(
            book_id,
            reader_core::BookReadingPace {
                wpm: state.pace.book_wpm,
                active_ms: state.pace.book_active_ms,
                updated_at: now,
            },
        );
    }
}

fn load_pace(state: &mut ReaderState, storage: &Storage, book_id: &str) -> Result<()> {
    let global_wpm = storage
        .setting("globalWpm")?
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(reader_core::pace::DEFAULT_WPM);
    let global_active_ms = storage
        .setting("globalActiveMs")?
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(0.0);
    let book_pace = storage.reading_pace(book_id)?;

    state.pace = reader_core::PaceState {
        global_wpm,
        global_active_ms,
        book_id: Some(book_id.to_owned()),
        book_wpm: book_pace.map_or(reader_core::pace::DEFAULT_WPM, |pace| pace.wpm),
        book_active_ms: book_pace.map_or(0.0, |pace| pace.active_ms),
        last_word_cursor: None,
        last_sample_at: None,
    };
    Ok(())
}

/// Import a file and open it.
pub fn import_and_open(
    state: &mut ReaderState,
    storage: &mut Storage,
    path: &std::path::Path,
    context: CommandContext,
) -> Result<()> {
    import_book(state, storage, path, context).map(|_| ())
}

/// Import one file, reporting whether it parsed.
///
/// A file that will not parse is a status message rather than an error, because
/// the reader is still running and the next file might be fine — but a caller
/// importing a whole set needs to know which ones did not make it.
pub fn import_book(
    state: &mut ReaderState,
    storage: &mut Storage,
    path: &std::path::Path,
    context: CommandContext,
) -> Result<bool> {
    match reader_formats::import_file(path) {
        Ok(book) => {
            let stored_id = storage.save_book(&book, state.settings.render_mode, context.now)?;
            let book = CanonicalBook {
                id: stored_id,
                ..book
            };
            let title = book.title.clone();
            open_book(state, storage, book, context)?;
            state.status = format!("Imported {title}");
            Ok(true)
        }
        Err(error) => {
            state.status = format!("Import failed: {error}");
            Ok(false)
        }
    }
}

/// Move the reader to a search hit, scrolling the block into view.
pub fn apply_search_hit(state: &mut ReaderState, hit: SearchHit, context: CommandContext) {
    state.chapter_index = hit.chapter_index;
    let line_start = state.focus_index_to_offset(context.content_width, hit.block_index);
    let max_offset = state.chapter_max_offset(context.content_width, context.body_height);
    state.block_offset = line_start.min(max_offset);
    if state.focus_mode {
        state.focus_block_index = hit.block_index;
    }
}

fn collect_search_hits(state: &ReaderState, query: &str, global: bool) -> Vec<SearchHit> {
    let Some(book) = state.current_book.as_ref() else {
        return Vec::new();
    };
    let needle = query.to_lowercase();
    let chapters: Vec<usize> = if global {
        (0..book.chapters.len()).collect()
    } else {
        vec![state.chapter_index]
    };
    let mut hits: Vec<SearchHit> = Vec::new();
    for chapter_index in chapters {
        let Some(chapter) = book.chapters.get(chapter_index) else {
            continue;
        };
        for (block_index, block) in chapter.blocks.iter().enumerate() {
            if block.text().to_lowercase().contains(&needle) {
                hits.push(SearchHit {
                    chapter_index,
                    block_index,
                    line_index: 0,
                });
            }
        }
    }
    hits
}

fn automatic_bookmark_label(chapter_index: usize, block_offset: usize) -> String {
    format!("Ch.{} §{block_offset}", chapter_index + 1)
}

/// Resolve a bookmark by id, then by exact label, then by a unique partial label.
fn find_bookmark_id(state: &ReaderState, storage: &Storage, query: &str) -> Result<Option<String>> {
    let Some(book_id) = state.book_id() else {
        return Ok(None);
    };
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Ok(None);
    }
    let bookmarks = storage.bookmarks(book_id)?;
    if let Some(found) = bookmarks.iter().find(|item| item.id == query.trim()) {
        return Ok(Some(found.id.clone()));
    }
    if let Some(found) = bookmarks
        .iter()
        .find(|item| item.label.as_deref().map(str::to_lowercase) == Some(needle.clone()))
    {
        return Ok(Some(found.id.clone()));
    }
    let partial: Vec<&reader_core::Bookmark> = bookmarks
        .iter()
        .filter(|item| {
            item.label
                .as_deref()
                .is_some_and(|label| label.to_lowercase().contains(&needle))
        })
        .collect();
    Ok(match partial.as_slice() {
        [single] => Some(single.id.clone()),
        _ => None,
    })
}

/// A chapter's weight for whole-book percentage jumps.
///
/// Word count is the natural weight, but CBZ chapters have none, so text length
/// stands in and a chapter with neither still counts as one unit.
fn effective_word_count(chapter: &reader_core::CanonicalChapter) -> usize {
    if chapter.word_count > 0 {
        return chapter.word_count;
    }
    let characters: usize = chapter
        .blocks
        .iter()
        .map(|block| block.text().chars().count())
        .sum();
    characters.max(1)
}

/// Run a command line.
///
/// Parse and argument errors become status messages; only a storage failure is
/// returned, since that means the library itself is unavailable.
pub fn execute_command(
    state: &mut ReaderState,
    storage: &mut Storage,
    input: &str,
    context: CommandContext,
) -> Result<()> {
    // In focus mode the viewport offset is the focused block's offset while a
    // command runs, and is restored afterwards.
    let saved_offset = if state.focus_mode && state.current_book.is_some() {
        let saved = state.block_offset;
        state.block_offset =
            state.focus_index_to_offset(context.content_width, state.focus_block_index);
        Some(saved)
    } else {
        None
    };

    let parsed = match parse_slash_command(input) {
        Ok(parsed) => parsed,
        Err(error) => {
            state.status = error.to_string();
            if let Some(offset) = saved_offset {
                state.block_offset = offset;
            }
            return Ok(());
        }
    };
    storage.save_command_history(input, parsed.name, context.now)?;

    let outcome = dispatch(state, storage, &parsed, context);
    match outcome {
        Ok(()) => {}
        Err(DispatchError::Message(message)) => state.status = message,
        Err(DispatchError::Storage(error)) => {
            if let Some(offset) = saved_offset {
                state.block_offset = offset;
            }
            return Err(ExecutionError::Storage(error));
        }
    }

    if state.focus_mode && state.current_book.is_some() {
        state.focus_block_index =
            state.offset_to_focus_index(context.content_width, state.block_offset);
    }
    if let Some(offset) = saved_offset {
        state.block_offset = offset;
    }
    Ok(())
}

/// A command either finished, told the reader something, or hit the database.
enum DispatchError {
    Message(String),
    Storage(StorageError),
}

impl From<StorageError> for DispatchError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

impl From<ExecutionError> for DispatchError {
    fn from(error: ExecutionError) -> Self {
        match error {
            ExecutionError::Storage(error) => Self::Storage(error),
        }
    }
}

/// Shorthand for "tell the reader this and stop".
fn message<T>(text: impl Into<String>) -> std::result::Result<T, DispatchError> {
    Err(DispatchError::Message(text.into()))
}

const NO_BOOK: &str = "No book open.";

fn dispatch(
    state: &mut ReaderState,
    storage: &mut Storage,
    parsed: &ParsedCommand,
    context: CommandContext,
) -> std::result::Result<(), DispatchError> {
    match parsed.name {
        "prev" | "next" => {
            if state.current_book.is_none() {
                return Ok(());
            }
            let count = parsed
                .args
                .first()
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(1)
                .max(1);
            state.push_nav_history();
            state.chapter_index = if parsed.name == "prev" {
                state.chapter_index.saturating_sub(count)
            } else {
                (state.chapter_index + count).min(state.chapter_count().saturating_sub(1))
            };
            state.block_offset = 0;
            state.push_nav_history();
            state.status = format!("Moved to chapter {}", state.chapter_index + 1);
            Ok(())
        }

        "chapters" => {
            state.open_overlay(Overlay::Chapters, state.chapter_index);
            state.status = "Opened table of contents".to_owned();
            Ok(())
        }

        "mark" => {
            let Some(book_id) = state.book_id().map(str::to_owned) else {
                return message(NO_BOOK);
            };
            let label = parsed.joined_args();
            let label = if label.trim().is_empty() {
                automatic_bookmark_label(state.chapter_index, state.block_offset)
            } else {
                label.trim().to_owned()
            };
            let bookmark = storage.add_bookmark(
                &book_id,
                state.chapter_index,
                state.block_offset,
                Some(&label),
                context.now,
            )?;
            state.status = format!(
                "Bookmark saved ({})",
                bookmark.id.chars().take(8).collect::<String>()
            );
            Ok(())
        }

        "marks" => {
            let Some(book_id) = state.book_id().map(str::to_owned) else {
                return message(NO_BOOK);
            };
            let bookmarks = storage.bookmarks(&book_id)?;
            state.open_overlay(Overlay::Bookmarks, 0);
            state.status = if bookmarks.is_empty() {
                "No bookmarks in this book yet.".to_owned()
            } else {
                "Opened bookmarks.".to_owned()
            };
            Ok(())
        }

        "delmark" => {
            if state.current_book.is_none() {
                return message(NO_BOOK);
            }
            let query = parsed.joined_args();
            if query.trim().is_empty() {
                return message("Use /delmark <id|label>");
            }
            match find_bookmark_id(state, storage, &query)? {
                Some(id) => {
                    storage.delete_bookmark(&id)?;
                    state.status = "Bookmark deleted.".to_owned();
                }
                None => state.status = format!("No bookmark matched \"{}\".", query.trim()),
            }
            Ok(())
        }

        "goto" => goto(state, parsed, context),

        "search" => {
            if state.current_book.is_none() {
                return message(NO_BOOK);
            }
            let query = parsed.joined_args();
            let query = query.trim();
            if query.is_empty() {
                return message("Use /search [-g|--global] <term>");
            }
            let global = parsed.has_flag("global");
            let hits = collect_search_hits(state, query, global);
            if hits.is_empty() {
                state.search = None;
                state.status = if global {
                    format!("No matches for \"{query}\" in this book.")
                } else {
                    format!("No matches for \"{query}\" in this chapter.")
                };
                return Ok(());
            }
            let first = hits[0];
            let count = hits.len();
            state.search = Some(SearchState {
                query: query.to_owned(),
                global,
                results: hits,
                cursor: 0,
            });
            state.push_nav_history();
            apply_search_hit(state, first, context);
            state.push_nav_history();
            state.status = if global {
                format!("Search: {count} match(es) in book for \"{query}\".")
            } else {
                format!("Search: {count} match(es) in chapter for \"{query}\".")
            };
            Ok(())
        }

        "mode" => {
            let value = parsed.args.first().map(String::as_str).unwrap_or_default();
            match value {
                "plain" => {
                    state.settings.render_mode = RenderMode::Plain;
                    storage.set_setting("renderMode", "plain")?;
                    state.status = "Render mode: plain".to_owned();
                }
                // Legacy spelling: keep whichever language is selected.
                "code" => {
                    state.settings.render_mode = RenderMode::Code;
                    storage.set_setting("renderMode", "code")?;
                    state.status = format!(
                        "Render mode: code ({})",
                        state.settings.code_language.as_str()
                    );
                }
                other => {
                    let Some(language) = CodeLanguage::from_id(other) else {
                        return message("Mode must be plain, typescript, python, or rust");
                    };
                    state.settings.render_mode = RenderMode::Code;
                    state.settings.code_language = language;
                    storage.set_setting("renderMode", "code")?;
                    storage.set_setting("codeLanguage", language.as_str())?;
                    state.status = format!("Render mode: {}", language.as_str());
                }
            }
            Ok(())
        }

        "density" => {
            let Some(density) = parsed
                .args
                .first()
                .and_then(|value| value.parse::<u8>().ok())
                .and_then(CodeDensity::new)
            else {
                return message("Density must be a number between 1 and 5");
            };
            state.settings.code_density = density;
            storage.set_setting("codeDensity", &density.get().to_string())?;
            state.status = format!("Code density: {}", density.get());
            Ok(())
        }

        "highlight" => {
            let value = parsed
                .args
                .first()
                .map(|value| value.to_lowercase())
                .unwrap_or_default();
            if parsed.args.len() != 1 || (value != "on" && value != "off") {
                return message("Use /highlight <on|off>");
            }
            let enabled = value == "on";
            state.settings.plain_highlight = enabled;
            storage.set_setting("plainHighlight", &enabled.to_string())?;
            state.status = format!("Dialogue highlight: {}", if enabled { "on" } else { "off" });
            Ok(())
        }

        "mouse" => {
            match parsed
                .args
                .first()
                .map(|value| value.to_lowercase())
                .as_deref()
            {
                None => state.settings.mouse_capture = !state.settings.mouse_capture,
                Some("on") => state.settings.mouse_capture = true,
                Some("off") => state.settings.mouse_capture = false,
                Some(_) => return message("Use /mouse [on|off]"),
            }
            storage.set_setting("mouseCapture", &state.settings.mouse_capture.to_string())?;
            state.status = if state.settings.mouse_capture {
                "Mouse capture on: clicks and scrollbar drag enabled; use Shift-drag for terminal selection.".to_owned()
            } else {
                "Mouse capture off: native selection enabled; wheel and keyboard scrolling remain active.".to_owned()
            };
            Ok(())
        }

        "toggleprogress" => {
            state.settings.progress_visibility = match parsed
                .args
                .first()
                .and_then(|value| ProgressVisibility::from_id(value))
            {
                Some(explicit) => explicit,
                None => state.settings.progress_visibility.next(),
            };
            storage.set_setting(
                "progressVisibility",
                state.settings.progress_visibility.as_str(),
            )?;
            state.status = format!(
                "Progress mode: {}",
                state.settings.progress_visibility.as_str()
            );
            Ok(())
        }

        "colorscheme" => {
            if parsed.has_flag("list") || parsed.args.is_empty() {
                let selected = ColorSchemeId::ALL
                    .iter()
                    .position(|scheme| *scheme == state.settings.theme_id)
                    .unwrap_or(0);
                state.open_overlay(Overlay::ColorSchemes, selected);
                state.status = "Opened colorscheme picker".to_owned();
                return Ok(());
            }
            let requested = &parsed.args[0];
            let Some(scheme) = ColorSchemeId::from_id(requested) else {
                return message(format!("Unknown colorscheme {requested}"));
            };
            state.settings.theme_id = scheme;
            state.refresh_theme();
            storage.set_setting("themeId", scheme.as_str())?;
            state.status = format!("Colorscheme set to {}", scheme.label());
            Ok(())
        }

        "theme" => {
            if parsed.has_flag("list") || parsed.args.is_empty() {
                let selected = AppearanceThemeId::ALL
                    .iter()
                    .position(|theme| *theme == state.settings.appearance_theme_id)
                    .unwrap_or(0);
                state.open_overlay(Overlay::Themes, selected);
                state.status = "Opened theme picker".to_owned();
                return Ok(());
            }
            let requested = &parsed.args[0];
            let Some(appearance) = AppearanceThemeId::from_id(requested) else {
                return message(format!("Unknown theme {requested}"));
            };
            state.settings.appearance_theme_id = appearance;
            state.refresh_theme();
            storage.set_setting("appearanceThemeId", appearance.as_str())?;
            state.status = format!("Theme set to {}", appearance.label());
            Ok(())
        }

        "settings" => {
            state.open_overlay(Overlay::Settings, 0);
            // Opening remembers the current settings, so cancelling can restore
            // them after the panel has previewed something else.
            crate::settings_panel::open(state);
            state.status = "Space changes · ←/→ tabs · Enter saves · Esc cancels".to_owned();
            Ok(())
        }

        "keyboardshortcuts" => {
            state.open_overlay(Overlay::Keys, 0);
            // Twenty-nine bindings at once is a wall of text; only the six a
            // reader needs first are open, and the rest are one Enter away.
            crate::shortcuts_panel::open(state);
            state.status = "Enter folds a group · z folds all · / searches".to_owned();
            Ok(())
        }

        "help" => {
            state.open_overlay(Overlay::Help, 0);
            state.help_command = if parsed.has_flag("all") {
                None
            } else {
                parsed.args.first().cloned()
            };
            state.status = match state.help_command.as_deref() {
                Some(command) => format!("Opened help for /{command}"),
                None => "Opened command manual".to_owned(),
            };
            Ok(())
        }

        "resume" => resume(state, storage, parsed, context),

        "changebook" => change_book(state, storage, parsed, context),

        "removecurrent" => {
            let Some(book_id) = state.book_id().map(str::to_owned) else {
                return message("No current book to remove.");
            };
            storage.remove_book(&book_id)?;
            state.current_book = None;
            state.search = None;
            state.clear_render_caches();
            state.status = "Current book removed from the library.".to_owned();
            Ok(())
        }

        "remove" => {
            if parsed.has_flag("current") {
                let Some(book_id) = state.book_id().map(str::to_owned) else {
                    return message("No current book to remove.");
                };
                storage.remove_book(&book_id)?;
                state.current_book = None;
                state.search = None;
                state.clear_render_caches();
                state.status = "Current book removed from the library.".to_owned();
                return Ok(());
            }
            let query = parsed.joined_args().to_lowercase();
            let matched = storage
                .books()?
                .into_iter()
                .find(|book| book.title.to_lowercase().contains(&query));
            let Some(matched) = matched else {
                state.status = "No matching book found.".to_owned();
                return Ok(());
            };
            storage.remove_book(&matched.id)?;
            if state.book_id() == Some(matched.id.as_str()) {
                state.current_book = None;
                state.search = None;
                state.clear_render_caches();
            }
            state.status = format!("Removed {} from the library.", matched.title);
            Ok(())
        }

        "tag" | "tags" => tags(state, storage, parsed),

        "note" => notes(state, storage, parsed, context),

        "add" => Ok(crate::library::add(state, storage, parsed, context)?),
        "librarydir" => Ok(crate::library::library_directory(state, storage, parsed)?),
        "export" => Ok(crate::library::export(state, storage, parsed, context)?),
        "import" => Ok(crate::library::import(state, storage, parsed)?),

        "toggl" => {
            // The integration owns its HTTP client; the reader only supplies
            // storage and the clock.
            let settings = crate::toggl::StorageSettings::new(storage);
            let transport = reader_integrations::NetworkTransport;
            let client = reader_integrations::TogglClient::new(&settings, &transport, context.now);
            let outcome = crate::toggl::run(parsed, &client);
            if let Some(prefill) = crate::toggl::apply_outcome(state, outcome) {
                state.command_prefill = Some(prefill);
            }
            Ok(())
        }

        other => {
            state.status = format!("Command not implemented: {other}");
            Ok(())
        }
    }
}

fn goto(
    state: &mut ReaderState,
    parsed: &ParsedCommand,
    context: CommandContext,
) -> std::result::Result<(), DispatchError> {
    if state.current_book.is_none() {
        return message(NO_BOOK);
    }
    let raw = parsed.joined_args();
    let raw = raw.trim();
    if raw.is_empty() {
        return message("Use /goto <chapter>|<percent>%|…%c>");
    }
    let last_chapter = state.chapter_count().saturating_sub(1);

    // `50%c` and `50% --chapter` both mean "within this chapter".
    let chapter_percent = raw
        .strip_suffix(['c', 'C'])
        .and_then(|rest| rest.strip_suffix('%'))
        .and_then(|value| value.parse::<f64>().ok());
    let plain_percent = raw
        .strip_suffix('%')
        .and_then(|value| value.parse::<f64>().ok());

    if let Some(percent) =
        chapter_percent.or_else(|| plain_percent.filter(|_| parsed.has_flag("chapter")))
    {
        if !(0.0..=100.0).contains(&percent) {
            state.status = "Percentage must be between 0 and 100.".to_owned();
            return Ok(());
        }
        state.push_nav_history();
        let max_offset = state.chapter_max_offset(context.content_width, context.body_height);
        state.block_offset =
            ((percent / 100.0 * max_offset as f64).floor() as usize).min(max_offset);
        state.push_nav_history();
        state.status = format!(
            "Jumped to {} of chapter {} (offset {})",
            format_percent(percent),
            state.chapter_index + 1,
            state.block_offset
        );
        return Ok(());
    }

    if let Some(percent) = plain_percent {
        if !(0.0..=100.0).contains(&percent) {
            state.status = "Percentage must be between 0 and 100.".to_owned();
            return Ok(());
        }
        if percent <= 0.0 {
            state.push_nav_history();
            state.chapter_index = 0;
            state.block_offset = 0;
            state.push_nav_history();
            state.status = "Jumped to 0% (start of book)".to_owned();
            return Ok(());
        }
        if percent >= 100.0 {
            state.push_nav_history();
            state.chapter_index = last_chapter;
            state.block_offset =
                state.chapter_max_offset(context.content_width, context.body_height);
            state.push_nav_history();
            state.status = "Jumped to 100% (end of book)".to_owned();
            return Ok(());
        }

        state.push_nav_history();
        let weights: Vec<usize> = state
            .current_book
            .as_ref()
            .expect("a book is open")
            .chapters
            .iter()
            .map(effective_word_count)
            .collect();
        let total: usize = weights.iter().sum();
        let target = percent / 100.0 * total as f64;
        let mut accumulated = 0.0;
        let mut chapter_index = 0usize;
        while chapter_index < last_chapter && target >= accumulated + weights[chapter_index] as f64
        {
            accumulated += weights[chapter_index] as f64;
            chapter_index += 1;
        }
        state.chapter_index = chapter_index;
        let chapter_weight = weights[chapter_index] as f64;
        let local = (target - accumulated).clamp(0.0, chapter_weight);
        let ratio = if chapter_weight > 0.0 {
            local / chapter_weight
        } else {
            0.0
        };
        let max_offset = state.chapter_max_offset(context.content_width, context.body_height);
        state.block_offset = ((ratio * max_offset as f64).floor() as usize).min(max_offset);
        state.push_nav_history();
        state.status = format!(
            "Jumped to {} of book (Ch.{} · offset {})",
            format_percent(percent),
            chapter_index + 1,
            state.block_offset
        );
        return Ok(());
    }

    if let Ok(number) = raw.parse::<usize>() {
        let count = state.chapter_count();
        if number < 1 || number > count {
            state.status =
                format!("There is no chapter {number}. This book has {count} chapter(s).");
            return Ok(());
        }
        state.push_nav_history();
        state.chapter_index = number - 1;
        state.block_offset = 0;
        state.push_nav_history();
        state.status = format!("Jumped to chapter {number}");
        return Ok(());
    }

    state.status = format!(
        "Could not parse position \"{raw}\". Try /goto 42%, /goto 30%c, /goto 5, or /goto 10% --chapter."
    );
    Ok(())
}

/// Format a percentage the way `String(number)` did in v1: no trailing zeros.
fn format_percent(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{}%", value as i64)
    } else {
        format!("{value}%")
    }
}

fn resume(
    state: &mut ReaderState,
    storage: &mut Storage,
    parsed: &ParsedCommand,
    context: CommandContext,
) -> std::result::Result<(), DispatchError> {
    if parsed.has_flag("latest") {
        if let Some(book_id) = storage.latest_book_id()?
            && let Some(book) = storage.book(&book_id)?
        {
            open_book(state, storage, book, context)?;
        }
        return Ok(());
    }
    if !parsed.args.is_empty() {
        return change_book(state, storage, parsed, context);
    }
    match storage.latest_book_id()? {
        None => {
            state.status = "No previous book to resume.".to_owned();
            Ok(())
        }
        Some(book_id) => {
            if let Some(book) = storage.book(&book_id)? {
                open_book(state, storage, book, context)?;
            }
            Ok(())
        }
    }
}

fn change_book(
    state: &mut ReaderState,
    storage: &mut Storage,
    parsed: &ParsedCommand,
    context: CommandContext,
) -> std::result::Result<(), DispatchError> {
    if let Some(sort) = parsed.flag_value("sort") {
        match LibrarySortKey::from_id(sort) {
            Some(key) => state.library_sort_key = key,
            None => {
                return message(format!(
                    "Invalid sort key \"{sort}\". Use: lastOpened, title, author, progress"
                ));
            }
        }
    }
    let query = parsed.joined_args();
    let books = storage.books()?;

    if query.trim().is_empty() {
        state.books_tag_filter = None;
        state.open_overlay(Overlay::Books, 0);
        state.status = if books.is_empty() {
            "No books in the library yet.".to_owned()
        } else {
            "Opened library picker.".to_owned()
        };
        return Ok(());
    }

    let needle = query.trim().to_lowercase();
    if let Some(selected) = books.iter().find(|book| {
        book.title.to_lowercase().contains(&needle) || book.author.to_lowercase().contains(&needle)
    }) {
        if let Some(book) = storage.book(&selected.id)? {
            open_book(state, storage, book, context)?;
        }
        return Ok(());
    }

    // No title or author matched, so try tags.
    let tags_by_book = storage.tags_by_book()?;
    let tagged: Vec<&reader_core::LibraryEntry> = books
        .iter()
        .filter(|book| {
            tags_by_book
                .get(&book.id)
                .is_some_and(|tags| tags.iter().any(|tag| tag.to_lowercase().contains(&needle)))
        })
        .collect();
    if let [single] = tagged.as_slice()
        && let Some(book) = storage.book(&single.id)?
    {
        open_book(state, storage, book, context)?;
        return Ok(());
    }

    state.open_overlay(Overlay::Books, 0);
    if tagged.is_empty() {
        state.books_tag_filter = None;
        state.status = "No exact match. Opened library picker.".to_owned();
    } else {
        state.books_tag_filter = Some(query.trim().to_owned());
        state.status = format!(
            "Filtering by tag \"{}\". {} book(s) found.",
            query.trim(),
            tagged.len()
        );
    }
    Ok(())
}

fn tags(
    state: &mut ReaderState,
    storage: &Storage,
    parsed: &ParsedCommand,
) -> std::result::Result<(), DispatchError> {
    let Some(book_id) = state.book_id().map(str::to_owned) else {
        return message(NO_BOOK);
    };
    let describe = |tags: Vec<String>| {
        if tags.is_empty() {
            "No tags for this book.".to_owned()
        } else {
            format!(
                "Tags: {}",
                tags.iter()
                    .map(|tag| format!("#{tag}"))
                    .collect::<Vec<_>>()
                    .join("  ")
            )
        }
    };

    if parsed.name == "tags" {
        state.status = describe(storage.tags(&book_id)?);
        return Ok(());
    }
    let tag = parsed.args.first().cloned();
    if parsed.has_flag("delete") {
        let Some(tag) = tag else {
            return message("Use /tag -d <tag>");
        };
        storage.remove_tag(&book_id, &tag)?;
        state.status = format!("Tag removed: #{tag}");
    } else if let Some(tag) = tag {
        storage.add_tag(&book_id, &tag)?;
        state.status = format!("Tag added: #{tag}");
    } else {
        state.status = describe(storage.tags(&book_id)?);
    }
    Ok(())
}

fn notes(
    state: &mut ReaderState,
    storage: &Storage,
    parsed: &ParsedCommand,
    context: CommandContext,
) -> std::result::Result<(), DispatchError> {
    let Some(book_id) = state.book_id().map(str::to_owned) else {
        return message(NO_BOOK);
    };
    if parsed.has_flag("list") {
        let notes = storage.notes(&book_id)?;
        state.open_overlay(Overlay::Notes, 0);
        state.status = if notes.is_empty() {
            "No notes for this book yet.".to_owned()
        } else {
            "Opened notes.".to_owned()
        };
        return Ok(());
    }
    if parsed.has_flag("delete") {
        let Some(id) = parsed.args.first() else {
            return message("Use /note -d <id>");
        };
        let notes = storage.notes(&book_id)?;
        match notes.iter().find(|note| &note.id == id) {
            Some(found) => {
                storage.delete_note(&found.id)?;
                state.status = "Note deleted.".to_owned();
            }
            None => state.status = "Note not found in current book.".to_owned(),
        }
        return Ok(());
    }
    let content = parsed.joined_args();
    if content.trim().is_empty() {
        return message("Use /note <text>");
    }
    let note = storage.add_note(
        &book_id,
        content.trim(),
        Some(state.chapter_index),
        Some(state.block_offset),
        context.now,
    )?;
    state.status = format!(
        "Note saved ({})",
        note.id.chars().take(8).collect::<String>()
    );
    Ok(())
}

/// Parse errors surface as status text; this keeps the mapping in one place for
/// callers that want the message without running a command.
#[must_use]
pub fn describe_parse_error(error: &CommandError) -> String {
    error.to_string()
}

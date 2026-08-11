//! Deterministic domain model for the reader.
//!
//! This crate deliberately has no terminal, database, archive, PDF, or HTTP
//! dependencies. Adapters convert external data into these types.

pub mod book;
pub mod command;
pub mod fuzzy;
pub mod library;
pub mod locale;
pub mod pace;
pub mod render;
pub mod settings;
pub mod shortcuts;
pub mod style;
pub mod theme;

pub use book::{
    BlockKind, CanonicalBlock, CanonicalBook, CanonicalChapter, DiagnosticSeverity,
    ImportDiagnostic,
};
pub use command::{COMMANDS, ParsedCommand, parse_slash_command};
pub use library::{
    BookReadingPace, Bookmark, LibraryEntry, LibraryEntryWithProgress, LibrarySortKey, Note,
    ReadingPosition, SortDirection,
};
pub use locale::{compare_text, format_relative_time};
pub use pace::{ChapterWords, PaceSample, PaceState};
pub use render::{RenderOptions, render_block, render_blocks};
pub use settings::{
    AppSettings, CodeDensity, CodeLanguage, LineSpacing, ProgressVisibility, RenderMode,
    SettingsTab,
};
pub use shortcuts::{KEYBOARD_SHORTCUTS, Shortcut, ShortcutCategory};
pub use style::{AnsiColor, Color, Span, Style, StyledLine};
pub use theme::{AppearanceThemeId, ColorSchemeId, Palette, Theme};

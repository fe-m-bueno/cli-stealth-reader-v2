//! Deterministic domain model for the reader.
//!
//! This crate deliberately has no terminal, database, archive, PDF, or HTTP
//! dependencies. Adapters convert external data into these types.

mod book;
pub mod fuzzy;
pub mod locale;
pub mod pace;
pub mod settings;
pub mod shortcuts;
pub mod style;
pub mod theme;

pub use book::{
    BlockKind, CanonicalBlock, CanonicalBook, CanonicalChapter, DiagnosticSeverity,
    ImportDiagnostic,
};
pub use locale::{compare_text, format_relative_time};
pub use pace::{ChapterWords, PaceSample, PaceState};
pub use settings::{
    AppSettings, CodeDensity, CodeLanguage, LineSpacing, ProgressVisibility, RenderMode,
    SettingsTab,
};
pub use shortcuts::{KEYBOARD_SHORTCUTS, Shortcut, ShortcutCategory};
pub use style::{AnsiColor, Color, Span, Style, StyledLine};
pub use theme::{AppearanceThemeId, ColorSchemeId, Palette, Theme};

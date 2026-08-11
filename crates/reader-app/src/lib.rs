//! Reader use cases: state, layout, and command execution.
//!
//! This crate coordinates the domain (`reader-core`), the library
//! (`reader-storage`), and the importers (`reader-formats`). It knows nothing
//! about terminals: the TUI passes in the viewport and the clock, and reads the
//! resulting state back out.

pub mod executor;
pub mod layout;
pub mod library;
pub mod overlay;
pub mod render_cache;
pub mod settings_panel;
pub mod shortcuts_panel;
pub mod startup;
pub mod state;
pub mod toggl;

pub use executor::{
    CommandContext, ExecutionError, apply_search_hit, execute_command, import_and_open,
    import_book, open_book, persist_pace,
};
pub use layout::{OverlayLayout, Viewport, ViewportLayout, compute_layout};
pub use overlay::{EntryStyle, EntryTarget, OverlayEntry, OverlaySearch, visible_entries};
pub use render_cache::ChapterRenderCache;
pub use startup::{LaunchOptions, launch};
pub use state::{MAX_NAV_HISTORY, NavEntry, Overlay, ReaderState, SearchHit, SearchState};

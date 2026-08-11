//! Terminal presentation for the reader.
//!
//! State updates happen outside rendering: the frame is a pure function of
//! [`reader_app::ReaderState`], and input is mapped to actions before anything
//! is drawn. That split is what makes the whole surface testable against a
//! Ratatui `TestBackend`.

pub mod style;

pub use style::{to_tui_color, to_tui_line, to_tui_lines, to_tui_style};

//! Format adapters: EPUB, CBZ, and PDF files become canonical books.
//!
//! Each adapter owns its parsing dependencies and reports problems as
//! diagnostics rather than failing, so a partly broken file still opens. Only an
//! unreadable container aborts the import.

pub mod cbz;
pub mod discovery;
pub mod epub;
pub mod html;
pub mod ids;
pub mod pdf;
pub mod xml;

use std::path::Path;

use reader_core::CanonicalBook;

pub use cbz::import_cbz;
pub use discovery::{Discovery, discover_books, resolve_library_directory};
pub use epub::{EPUB_PARSER_VERSION, import_epub};
pub use pdf::import_pdf;

/// Why an import could not produce a book at all.
#[derive(Debug)]
pub enum ImportError {
    /// The file could not be read.
    Io(std::io::Error),
    /// The extension is not one the reader supports.
    UnsupportedFormat(String),
    /// The container exists but is not usable (bad zip, missing OPF, …).
    Malformed(String),
    /// The file parsed, but nothing readable came out of it.
    NoReadableContent(String),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::UnsupportedFormat(path) => write!(formatter, "Unsupported format: {path}"),
            Self::Malformed(detail) => write!(formatter, "{detail}"),
            Self::NoReadableContent(detail) => write!(formatter, "{detail}"),
        }
    }
}

impl std::error::Error for ImportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ImportError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Import any supported file, chosen by extension.
pub fn import_file(path: &Path) -> Result<CanonicalBook, ImportError> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_lowercase();
    match extension.as_str() {
        "epub" => import_epub(path),
        "cbz" => import_cbz(path),
        "pdf" => import_pdf(path),
        _ => Err(ImportError::UnsupportedFormat(path.display().to_string())),
    }
}

/// Extensions the reader can open.
pub const SUPPORTED_EXTENSIONS: [&str; 3] = ["epub", "cbz", "pdf"];

/// Whether `path` names a file the reader can open.
#[must_use]
pub fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_lowercase)
        .is_some_and(|extension| SUPPORTED_EXTENSIONS.contains(&extension.as_str()))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{ImportError, import_file, is_supported};

    #[test]
    fn supported_extensions_are_matched_case_insensitively() {
        assert!(is_supported(Path::new("/books/dune.EPUB")));
        assert!(is_supported(Path::new("comic.cbz")));
        assert!(is_supported(Path::new("paper.pdf")));
        assert!(!is_supported(Path::new("notes.txt")));
        assert!(!is_supported(Path::new("no-extension")));
    }

    #[test]
    fn an_unsupported_file_is_rejected_by_name_before_being_read() {
        let error = import_file(Path::new("/nonexistent/notes.txt")).expect_err("unsupported");
        assert!(matches!(error, ImportError::UnsupportedFormat(_)));
        assert_eq!(
            error.to_string(),
            "Unsupported format: /nonexistent/notes.txt"
        );
    }
}

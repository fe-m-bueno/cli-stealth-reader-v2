//! Recursive discovery of readable files under the library directory.

use std::path::{Path, PathBuf};

use reader_core::compare_text;

use crate::is_supported;

/// One discovered file: its absolute path plus the path shown in the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Discovery {
    pub path: PathBuf,
    /// Path relative to the library root, so two books with the same file name
    /// in different folders stay distinguishable.
    pub file_name: String,
}

/// Expand a configured library directory.
///
/// An empty or missing value means the current directory; `~` and `~/…` expand
/// against the home directory; everything else resolves against `cwd`.
#[must_use]
pub fn resolve_library_directory(configured: Option<&str>, cwd: &Path, home: &Path) -> PathBuf {
    let value = configured.map(str::trim).unwrap_or_default();
    if value.is_empty() {
        return cwd.to_path_buf();
    }
    let expanded: PathBuf = if value == "~" {
        home.to_path_buf()
    } else if let Some(rest) = value
        .strip_prefix("~/")
        .or_else(|| value.strip_prefix("~\\"))
    {
        home.join(rest)
    } else {
        PathBuf::from(value)
    };
    if expanded.is_absolute() {
        expanded
    } else {
        cwd.join(expanded)
    }
}

/// Find every supported book under `root`, sorted by display path.
///
/// Unreadable subdirectories are skipped; an unreadable root is an error, since
/// silently showing an empty library would look like the books had vanished.
pub fn discover_books(root: &Path) -> std::io::Result<Vec<Discovery>> {
    let mut found: Vec<Discovery> = Vec::new();
    let entries = std::fs::read_dir(root)?;
    visit(root, entries, &mut found);
    found.sort_by(|left, right| compare_text(&left.file_name, &right.file_name));
    Ok(found)
}

fn visit(root: &Path, entries: std::fs::ReadDir, found: &mut Vec<Discovery>) {
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            if let Ok(nested) = std::fs::read_dir(&path) {
                visit(root, nested, found);
            }
        } else if file_type.is_file() && is_supported(&path) {
            let relative = path.strip_prefix(root).unwrap_or(&path);
            found.push(Discovery {
                file_name: relative.display().to_string(),
                path,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{discover_books, resolve_library_directory};

    fn scratch(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "reader-formats-discovery-{}-{name}",
            std::process::id()
        ));
        std::fs::remove_dir_all(&directory).ok();
        std::fs::create_dir_all(&directory).expect("temp dir");
        directory
    }

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().expect("has a parent")).expect("parent");
        std::fs::write(path, b"").expect("file");
    }

    #[test]
    fn only_supported_extensions_are_discovered() {
        let root = scratch("supported");
        for name in [
            "book.epub",
            "comic.cbz",
            "doc.pdf",
            "image.jpg",
            "readme.txt",
        ] {
            touch(&root.join(name));
        }

        let found = discover_books(&root).expect("root should be readable");

        assert_eq!(
            found
                .iter()
                .map(|entry| entry.file_name.as_str())
                .collect::<Vec<_>>(),
            vec!["book.epub", "comic.cbz", "doc.pdf"]
        );
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn discovery_recurses_and_keeps_relative_paths_distinguishable() {
        let root = scratch("recursive");
        touch(&root.join("fiction/classics/dune.epub"));
        touch(&root.join("comics/dune.cbz"));
        touch(&root.join("fiction/notes.txt"));

        let found = discover_books(&root).expect("root should be readable");

        assert_eq!(
            found
                .iter()
                .map(|entry| entry.file_name.as_str())
                .collect::<Vec<_>>(),
            vec!["comics/dune.cbz", "fiction/classics/dune.epub"]
        );
        assert!(found[0].path.ends_with("comics/dune.cbz"));
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn results_are_sorted_with_ui_collation() {
        let root = scratch("sorted");
        for name in ["Book 10.epub", "book 2.epub", "apple.pdf"] {
            touch(&root.join(name));
        }

        let found = discover_books(&root).expect("root should be readable");

        assert_eq!(
            found
                .iter()
                .map(|entry| entry.file_name.as_str())
                .collect::<Vec<_>>(),
            vec!["apple.pdf", "book 2.epub", "Book 10.epub"]
        );
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn an_empty_library_is_not_an_error_but_a_missing_root_is() {
        let root = scratch("empty");
        assert!(discover_books(&root).expect("readable").is_empty());
        assert!(discover_books(&root.join("missing")).is_err());
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn library_directories_expand_tildes_and_relative_paths() {
        let cwd = Path::new("/work");
        let home = Path::new("/home/reader");

        assert_eq!(resolve_library_directory(None, cwd, home), cwd);
        assert_eq!(resolve_library_directory(Some("  "), cwd, home), cwd);
        assert_eq!(resolve_library_directory(Some("~"), cwd, home), home);
        assert_eq!(
            resolve_library_directory(Some("~/Books"), cwd, home),
            Path::new("/home/reader/Books")
        );
        assert_eq!(
            resolve_library_directory(Some("books/epub"), cwd, home),
            Path::new("/work/books/epub")
        );
        assert_eq!(
            resolve_library_directory(Some("/srv/library"), cwd, home),
            Path::new("/srv/library")
        );
    }
}

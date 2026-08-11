//! Where the reader keeps its data.
//!
//! The layout is the one v1 established, so a v2 binary opens the same library
//! the user already has: the database and settings live under
//! `$XDG_DATA_HOME/cli-stealth-reader`, and the rebuildable book cache under
//! `$XDG_CACHE_HOME/cli-stealth-reader`.

use std::path::{Path, PathBuf};

/// Directory name used under both XDG roots.
pub const APP_DIRECTORY: &str = "cli-stealth-reader";
/// Database file name inside the data directory.
pub const DATABASE_FILE: &str = "library.db";

/// Resolved application directories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub db_path: PathBuf,
}

impl AppPaths {
    /// Build paths from explicit roots, without touching the filesystem.
    #[must_use]
    pub fn from_roots(xdg_data_home: &Path, xdg_cache_home: &Path) -> Self {
        let data_dir = xdg_data_home.join(APP_DIRECTORY);
        Self {
            db_path: data_dir.join(DATABASE_FILE),
            data_dir,
            cache_dir: xdg_cache_home.join(APP_DIRECTORY),
        }
    }

    /// Resolve from the environment, matching v1's fallbacks: `$XDG_DATA_HOME`
    /// then `~/.local/share`, and `$XDG_CACHE_HOME` then `~/.cache`.
    ///
    /// An unset `HOME` falls back to the current directory rather than failing,
    /// so the reader still starts in a bare container.
    #[must_use]
    pub fn from_env() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let data_root =
            non_empty_var("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local").join("share"));
        let cache_root = non_empty_var("XDG_CACHE_HOME").unwrap_or_else(|| home.join(".cache"));
        Self::from_roots(&data_root, &cache_root)
    }

    /// The directory holding cached book JSON, one subdirectory per book.
    #[must_use]
    pub fn chapter_cache_dir(&self) -> PathBuf {
        self.cache_dir.join("books")
    }

    /// Create the directories the reader writes to.
    pub fn ensure(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::create_dir_all(self.chapter_cache_dir())?;
        Ok(())
    }
}

fn non_empty_var(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::AppPaths;

    #[test]
    fn paths_follow_the_v1_layout() {
        let paths = AppPaths::from_roots(Path::new("/data"), Path::new("/cache"));
        assert_eq!(paths.data_dir, Path::new("/data/cli-stealth-reader"));
        assert_eq!(paths.cache_dir, Path::new("/cache/cli-stealth-reader"));
        assert_eq!(
            paths.db_path,
            Path::new("/data/cli-stealth-reader/library.db")
        );
        assert_eq!(
            paths.chapter_cache_dir(),
            Path::new("/cache/cli-stealth-reader/books")
        );
    }

    #[test]
    fn ensure_creates_the_data_and_cache_directories() {
        let root =
            std::env::temp_dir().join(format!("reader-storage-paths-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let paths = AppPaths::from_roots(&root.join("data"), &root.join("cache"));

        paths.ensure().expect("directories should be creatable");

        assert!(paths.data_dir.is_dir());
        assert!(paths.chapter_cache_dir().is_dir());
        std::fs::remove_dir_all(root).ok();
    }
}

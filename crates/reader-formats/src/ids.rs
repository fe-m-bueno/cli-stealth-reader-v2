//! Identity of imported books.
//!
//! v1 derived ids differently per format, and the values are stored in the user
//! database, so the derivations are reproduced exactly: changing one would
//! orphan every saved position, bookmark, and note for that book.

use std::path::Path;

use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256};

fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Content hash identifying a file regardless of where it lives.
///
/// Two copies of the same book share it, which is how the library recognizes a
/// renamed or moved file and how export/import matches books across machines.
#[must_use]
pub fn hash_bytes(bytes: &[u8]) -> String {
    hex(Sha256::digest(bytes))
}

/// EPUB book id: SHA-1 of the absolute source path.
#[must_use]
pub fn epub_book_id(path: &Path) -> String {
    hex(Sha1::digest(path.display().to_string().as_bytes()))
}

/// CBZ and PDF book id: the first 16 hex characters of the path's SHA-256.
#[must_use]
pub fn short_book_id(path: &Path) -> String {
    hex(Sha256::digest(path.display().to_string().as_bytes()))
        .chars()
        .take(16)
        .collect()
}

/// EPUB chapter id: SHA-1 of `<href>:<index>`.
#[must_use]
pub fn chapter_id(href: &str, index: usize) -> String {
    hex(Sha1::digest(format!("{href}:{index}").as_bytes()))
}

/// Block-id prefix for one archive entry: the first 8 hex characters of its MD5.
///
/// This keeps ids stable for a file whose blocks are shared by several chapters.
#[must_use]
pub fn block_prefix(key: &str) -> String {
    hex(Md5::digest(key.as_bytes())).chars().take(8).collect()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{block_prefix, chapter_id, epub_book_id, hash_bytes, short_book_id};

    #[test]
    fn hashes_match_the_v1_derivations() {
        // Values produced by the v1 implementation for the same inputs.
        assert_eq!(
            hash_bytes(b"hello"),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        assert_eq!(
            epub_book_id(Path::new("/books/dune.epub")),
            "9012eb15cae36e988721760a7fb1414c7b41faca"
        );
        assert_eq!(short_book_id(Path::new("/books/comic.cbz")).len(), 16);
        assert_eq!(chapter_id("text/ch1.xhtml", 0).len(), 40);
        assert_eq!(block_prefix("text/ch1.xhtml").len(), 8);
    }

    #[test]
    fn ids_are_stable_and_input_sensitive() {
        assert_eq!(chapter_id("a", 1), chapter_id("a", 1));
        assert_ne!(chapter_id("a", 1), chapter_id("a", 2));
        assert_ne!(block_prefix("a"), block_prefix("b"));
        assert_ne!(
            short_book_id(Path::new("/a.cbz")),
            short_book_id(Path::new("/b.cbz"))
        );
    }
}

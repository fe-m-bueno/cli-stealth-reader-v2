//! CBZ (comic archive) import.
//!
//! A CBZ carries no text, so every image becomes a one-block chapter whose text
//! is a placeholder. The reader still gets navigation, positions, and bookmarks;
//! it just has nothing to read aloud. That limitation is reported as a
//! diagnostic on every import rather than silently.

use std::io::Cursor;
use std::path::Path;

use reader_core::{
    CanonicalBlock, CanonicalBook, CanonicalChapter, DiagnosticSeverity, ImportDiagnostic,
    compare_text,
};
use zip::ZipArchive;

use crate::ImportError;
use crate::ids::{hash_bytes, short_book_id};

const IMAGE_EXTENSIONS: [&str; 6] = ["jpg", "jpeg", "png", "gif", "webp", "bmp"];

fn is_image(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_lowercase)
        .is_some_and(|extension| IMAGE_EXTENSIONS.contains(&extension.as_str()))
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_owned()
}

fn base_name(name: &str) -> String {
    name.rsplit('/').next().unwrap_or(name).to_owned()
}

/// Import a CBZ archive.
pub fn import_cbz(path: &Path) -> Result<CanonicalBook, ImportError> {
    let bytes = std::fs::read(path)?;
    let title = file_stem(path);
    let book_id = short_book_id(path);
    let import_hash = hash_bytes(&bytes);
    let mut diagnostics: Vec<ImportDiagnostic> = Vec::new();

    let empty_book = |diagnostics: Vec<ImportDiagnostic>| CanonicalBook {
        id: book_id.clone(),
        title: title.clone(),
        author: "Unknown".to_owned(),
        source_path: path.display().to_string(),
        import_hash: import_hash.clone(),
        parser_version: Some(1),
        diagnostics,
        chapters: Vec::new(),
        cover_path: None,
    };

    let Ok(mut archive) = ZipArchive::new(Cursor::new(&bytes)) else {
        diagnostics.push(ImportDiagnostic {
            severity: DiagnosticSeverity::Error,
            message: "Failed to read CBZ archive: not a valid zip archive".to_owned(),
            context: None,
        });
        return Ok(empty_book(diagnostics));
    };

    let mut images: Vec<String> = Vec::new();
    for index in 0..archive.len() {
        let Ok(entry) = archive.by_index(index) else {
            continue;
        };
        if !entry.is_dir() && is_image(entry.name()) {
            images.push(entry.name().to_owned());
        }
    }
    images.sort_by(|left, right| compare_text(left, right));

    if images.is_empty() {
        diagnostics.push(ImportDiagnostic {
            severity: DiagnosticSeverity::Warning,
            message: "No images found in CBZ archive.".to_owned(),
            context: None,
        });
    } else {
        diagnostics.push(ImportDiagnostic {
            severity: DiagnosticSeverity::Warning,
            message: format!(
                "CBZ imported without OCR — {} page(s) shown as image placeholders. No text available.",
                images.len()
            ),
            context: None,
        });
    }

    let total = images.len();
    let chapters: Vec<CanonicalChapter> = images
        .into_iter()
        .enumerate()
        .map(|(index, image)| {
            let page = index + 1;
            CanonicalChapter {
                id: format!("{book_id}-ch{page}"),
                index,
                title: format!("Page {page}"),
                href: image.clone(),
                depth: 0,
                blocks: vec![CanonicalBlock::Image {
                    id: format!("{book_id}-p{page}"),
                    text: format!("[Page {page}/{total}: {}]", base_name(&image)),
                    image_source: Some(image),
                }],
                word_count: 0,
            }
        })
        .collect();

    Ok(CanonicalBook {
        chapters,
        diagnostics,
        ..empty_book(Vec::new())
    })
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use reader_core::DiagnosticSeverity;

    use super::import_cbz;

    /// Write a CBZ containing `names`, plus a stray non-image entry.
    fn write_cbz(directory: &std::path::Path, name: &str, names: &[&str]) -> std::path::PathBuf {
        let path = directory.join(name);
        let file = std::fs::File::create(&path).expect("fixture should be writable");
        let mut zip = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        for entry in names {
            zip.start_file(*entry, options).expect("entry");
            zip.write_all(format!("fake-image-data-{entry}").as_bytes())
                .expect("entry data");
        }
        zip.finish().expect("archive should close");
        path
    }

    fn temp_dir() -> std::path::PathBuf {
        let base = std::env::temp_dir().join(format!(
            "reader-formats-cbz-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&base).expect("temp dir");
        base
    }

    #[test]
    fn images_become_numerically_ordered_single_block_chapters() {
        let directory = temp_dir();
        let path = write_cbz(
            &directory,
            "ordered.cbz",
            &[
                "page-10.jpg",
                "page-2.jpg",
                "page-1.jpg",
                "ComicInfo.xml",
                "cover/page-3.png",
            ],
        );

        let book = import_cbz(&path).expect("import should succeed");

        assert_eq!(book.title, "ordered");
        assert_eq!(book.author, "Unknown");
        assert_eq!(book.parser_version, Some(1));
        assert_eq!(
            book.chapters
                .iter()
                .map(|chapter| chapter.href.as_str())
                .collect::<Vec<_>>(),
            vec![
                "cover/page-3.png",
                "page-1.jpg",
                "page-2.jpg",
                "page-10.jpg"
            ]
        );
        assert_eq!(book.chapters[0].title, "Page 1");
        assert_eq!(book.chapters[0].word_count, 0);
        assert_eq!(book.chapters[0].blocks.len(), 1);
        assert_eq!(book.chapters[0].blocks[0].text(), "[Page 1/4: page-3.png]");
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn every_import_warns_that_there_is_no_text() {
        let directory = temp_dir();
        let path = write_cbz(&directory, "warned.cbz", &["a.jpg"]);
        let book = import_cbz(&path).expect("import should succeed");
        assert_eq!(book.diagnostics.len(), 1);
        assert_eq!(book.diagnostics[0].severity, DiagnosticSeverity::Warning);
        assert!(book.diagnostics[0].message.contains("without OCR"));
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn an_archive_without_images_imports_empty_with_a_warning() {
        let directory = temp_dir();
        let path = write_cbz(&directory, "empty.cbz", &["readme.txt"]);
        let book = import_cbz(&path).expect("import should succeed");
        assert!(book.chapters.is_empty());
        assert_eq!(
            book.diagnostics[0].message,
            "No images found in CBZ archive."
        );
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn a_corrupt_archive_reports_an_error_diagnostic_instead_of_failing() {
        let directory = temp_dir();
        let path = directory.join("corrupt.cbz");
        std::fs::write(&path, b"this is not a zip file").expect("fixture");
        let book = import_cbz(&path).expect("import should still return a book");
        assert!(book.chapters.is_empty());
        assert_eq!(book.diagnostics[0].severity, DiagnosticSeverity::Error);
        assert!(
            book.diagnostics[0]
                .message
                .starts_with("Failed to read CBZ archive")
        );
        std::fs::remove_file(path).ok();
    }
}

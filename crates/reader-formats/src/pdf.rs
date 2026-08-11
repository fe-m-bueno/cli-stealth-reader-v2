//! PDF import.
//!
//! Each page becomes a chapter and each blank-line-separated run becomes a
//! paragraph. A page with no extractable text is almost always a scan, so it
//! gets a placeholder block and a diagnostic instead of vanishing — the reader
//! can still navigate a mixed book.
//!
//! Text extraction uses `pdf-extract`; v1 used pdf.js. Both decode content
//! streams rather than performing OCR, so the same pages yield text, but line
//! and paragraph breaks can differ on complex layouts. That is the one
//! deliberate departure from byte-level v1 parity.

use std::path::Path;

use reader_core::{
    CanonicalBlock, CanonicalBook, CanonicalChapter, DiagnosticSeverity, ImportDiagnostic,
};

use crate::ImportError;
use crate::ids::{hash_bytes, short_book_id};

/// Split extracted page text into paragraphs on blank lines, joining the lines
/// within each paragraph back into a single run.
fn paragraphs(text: &str) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    let mut blank_run = 0usize;

    for line in text.lines() {
        if line.trim().is_empty() {
            blank_run += 1;
            // v1 split on two or more consecutive newlines.
            if blank_run >= 1 && !current.is_empty() {
                result.push(
                    current
                        .join(" ")
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" "),
                );
                current.clear();
            }
            continue;
        }
        blank_run = 0;
        current.push(line);
    }
    if !current.is_empty() {
        result.push(
            current
                .join(" ")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
        );
    }
    result.into_iter().filter(|item| !item.is_empty()).collect()
}

/// Decode a PDF text string, which is either UTF-16BE with a byte-order mark or
/// PDFDocEncoding (close enough to Latin-1 for titles).
fn decode_pdf_string(bytes: &[u8]) -> String {
    if let Some(body) = bytes.strip_prefix(&[0xfe, 0xff]) {
        return body
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect::<Vec<u16>>()
            .into_iter()
            .map(|unit| char::from_u32(u32::from(unit)).unwrap_or('\u{fffd}'))
            .collect();
    }
    bytes.iter().map(|byte| char::from(*byte)).collect()
}

/// The `/Info` dictionary's title and author, when present and non-empty.
fn document_info(bytes: &[u8]) -> (Option<String>, Option<String>) {
    let Ok(document) = lopdf::Document::load_mem(bytes) else {
        return (None, None);
    };
    let info = document
        .trailer
        .get(b"Info")
        .ok()
        .and_then(|object| match object {
            lopdf::Object::Reference(id) => document.get_object(*id).ok(),
            other => Some(other),
        })
        .and_then(|object| object.as_dict().ok());
    let Some(info) = info else {
        return (None, None);
    };
    let field = |key: &[u8]| {
        info.get(key)
            .ok()
            .and_then(|object| object.as_str().ok())
            .map(decode_pdf_string)
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    };
    (field(b"Title"), field(b"Author"))
}

/// Import a PDF file.
pub fn import_pdf(path: &Path) -> Result<CanonicalBook, ImportError> {
    let bytes = std::fs::read(path)?;
    let base_name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_owned();
    let book_id = short_book_id(path);
    let import_hash = hash_bytes(&bytes);
    let mut diagnostics: Vec<ImportDiagnostic> = Vec::new();

    let pages = match pdf_extract::extract_text_from_mem_by_pages(&bytes) {
        Ok(pages) => pages,
        Err(error) => {
            diagnostics.push(ImportDiagnostic {
                severity: DiagnosticSeverity::Error,
                message: format!("Failed to parse PDF: {error}"),
                context: None,
            });
            return Ok(CanonicalBook {
                id: book_id,
                title: base_name,
                author: "Unknown".to_owned(),
                source_path: path.display().to_string(),
                import_hash,
                parser_version: Some(1),
                diagnostics,
                chapters: Vec::new(),
                cover_path: None,
            });
        }
    };

    let (document_title, document_author) = document_info(&bytes);
    let title = document_title.unwrap_or_else(|| base_name.clone());
    let author = document_author.unwrap_or_else(|| "Unknown".to_owned());

    let chapters: Vec<CanonicalChapter> = pages
        .iter()
        .enumerate()
        .map(|(index, raw)| {
            let page = index + 1;
            let text = raw.trim();
            let blocks = if text.is_empty() {
                diagnostics.push(ImportDiagnostic {
                    severity: DiagnosticSeverity::Warning,
                    message: format!(
                        "Page {page} has no extractable text (may be an image-only page)."
                    ),
                    context: None,
                });
                vec![CanonicalBlock::Paragraph {
                    id: format!("{book_id}-p{page}-empty"),
                    text: format!("[Page {page}: no text content]"),
                }]
            } else {
                paragraphs(text)
                    .into_iter()
                    .enumerate()
                    .map(|(offset, paragraph)| CanonicalBlock::Paragraph {
                        id: format!("{book_id}-p{page}-b{offset}"),
                        text: paragraph,
                    })
                    .collect()
            };
            CanonicalChapter {
                id: format!("{book_id}-ch{page}"),
                index,
                title: format!("Page {page}"),
                href: format!("page-{page}"),
                depth: 0,
                word_count: text.split_whitespace().count(),
                blocks,
            }
        })
        .collect();

    Ok(CanonicalBook {
        id: book_id,
        title,
        author,
        source_path: path.display().to_string(),
        import_hash,
        parser_version: Some(1),
        diagnostics,
        chapters,
        cover_path: None,
    })
}

#[cfg(test)]
mod tests {
    use reader_core::DiagnosticSeverity;

    use super::{import_pdf, paragraphs};

    /// A minimal single-page PDF whose content stream draws `lines`.
    fn write_pdf(path: &std::path::Path, lines: &[&str]) {
        let content = lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                format!("BT /F1 12 Tf 72 {} Td ({line}) Tj ET\n", 720 - index * 24)
            })
            .collect::<String>();
        let objects = [
            "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj".to_owned(),
            "2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj".to_owned(),
            "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>\nendobj".to_owned(),
            format!("4 0 obj\n<< /Length {} >>\nstream\n{content}endstream\nendobj", content.len()),
            "5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj".to_owned(),
        ];
        let mut pdf = String::from("%PDF-1.4\n");
        let mut offsets = Vec::new();
        for object in &objects {
            offsets.push(pdf.len());
            pdf.push_str(object);
            pdf.push('\n');
        }
        let xref_offset = pdf.len();
        pdf.push_str(&format!(
            "xref\n0 {}\n0000000000 65535 f \n",
            objects.len() + 1
        ));
        for offset in offsets {
            pdf.push_str(&format!("{offset:010} 00000 n \n"));
        }
        pdf.push_str(&format!(
            "trailer<</Size {}/Root 1 0 R>>\nstartxref\n{xref_offset}\n%%EOF\n",
            objects.len() + 1
        ));
        std::fs::write(path, pdf).expect("fixture should be writable");
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        let directory =
            std::env::temp_dir().join(format!("reader-formats-pdf-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("temp dir");
        directory.join(name)
    }

    #[test]
    fn paragraph_splitting_joins_wrapped_lines_and_breaks_on_blank_lines() {
        assert_eq!(
            paragraphs("first line\nsecond line\n\nnew paragraph"),
            vec!["first line second line", "new paragraph"]
        );
        assert_eq!(paragraphs("  \n \n"), Vec::<String>::new());
        assert_eq!(paragraphs("single"), vec!["single"]);
    }

    #[test]
    fn a_text_page_becomes_a_chapter_of_paragraphs() {
        let path = temp_path("text.pdf");
        write_pdf(&path, &["Hello World", "Second line"]);

        let book = import_pdf(&path).expect("import should succeed");

        assert_eq!(book.title, "text");
        assert_eq!(book.author, "Unknown");
        assert_eq!(book.parser_version, Some(1));
        assert_eq!(book.chapters.len(), 1);
        assert_eq!(book.chapters[0].title, "Page 1");
        assert_eq!(book.chapters[0].href, "page-1");
        let text: String = book.chapters[0]
            .blocks
            .iter()
            .map(reader_core::CanonicalBlock::text)
            .collect::<Vec<_>>()
            .join(" ");
        assert!(text.contains("Hello World"), "extracted {text:?}");
        assert!(book.chapters[0].word_count > 0);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn a_page_without_text_gets_a_placeholder_and_a_warning() {
        let path = temp_path("blank.pdf");
        write_pdf(&path, &[]);

        let book = import_pdf(&path).expect("import should succeed");

        assert_eq!(book.chapters.len(), 1);
        assert_eq!(
            book.chapters[0].blocks[0].text(),
            "[Page 1: no text content]"
        );
        assert_eq!(book.chapters[0].word_count, 0);
        assert!(book.diagnostics.iter().any(|diagnostic| diagnostic.severity
            == DiagnosticSeverity::Warning
            && diagnostic.message.contains("image-only page")));
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn an_unreadable_pdf_reports_an_error_diagnostic_instead_of_failing() {
        let path = temp_path("broken.pdf");
        std::fs::write(&path, b"%PDF-1.4\nnot really a pdf").expect("fixture");

        let book = import_pdf(&path).expect("import should still return a book");

        assert!(book.chapters.is_empty());
        assert_eq!(book.diagnostics.len(), 1);
        assert_eq!(book.diagnostics[0].severity, DiagnosticSeverity::Error);
        assert!(
            book.diagnostics[0]
                .message
                .starts_with("Failed to parse PDF:")
        );
        std::fs::remove_file(path).ok();
    }
}

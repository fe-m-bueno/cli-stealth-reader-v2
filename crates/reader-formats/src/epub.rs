//! EPUB import.
//!
//! The pipeline follows the v1 contract: validate the container, read the OPF,
//! resolve a table of contents (EPUB3 nav, then NCX, then spine order), and turn
//! each entry into a chapter. A chapter's content comes from a fragment slice, a
//! run of spine files, or a whole file, in that order of preference.
//!
//! Two cleanups make real books readable: front matter (cover, copyright,
//! contents) is dropped, and a chapter that opens with short non-sentence
//! paragraphs has them promoted to headings, since many publishers mark titles
//! with styling instead of `<h1>`.

use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::Path;

use reader_core::{
    CanonicalBlock, CanonicalBook, CanonicalChapter, DiagnosticSeverity, ImportDiagnostic,
};
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};
use zip::ZipArchive;

use crate::ImportError;
use crate::html::{
    NavEntry, extract_blocks_from_html, find_first_chapter_anchor, parse_nav_toc,
    slice_blocks_by_anchors,
};
use crate::ids::{block_prefix, chapter_id, epub_book_id, hash_bytes};
use crate::xml::{parse_container, parse_ncx, parse_package};

/// Bumped when extraction changes enough that stored books should be reimported.
pub const EPUB_PARSER_VERSION: u32 = 3;

type Archive = ZipArchive<Cursor<Vec<u8>>>;

/// One resolved table-of-contents entry.
#[derive(Debug, Clone)]
struct TocItem {
    label: String,
    href: String,
    depth: usize,
}

fn read_text(archive: &mut Archive, path: &str) -> Result<String, ImportError> {
    let mut entry = archive
        .by_name(path)
        .map_err(|_| ImportError::Malformed(format!("Missing archive entry: {path}")))?;
    let mut buffer = Vec::new();
    entry.read_to_end(&mut buffer)?;
    Ok(String::from_utf8_lossy(&buffer).into_owned())
}

/// Resolve `href` against `base_dir` and collapse `.` and `..`, as EPUB paths are
/// always POSIX-style inside the archive.
fn normalize_href(base_dir: &str, href: &str) -> String {
    let combined = if base_dir.is_empty() || base_dir == "." {
        href.to_owned()
    } else {
        format!("{base_dir}/{href}")
    };
    let mut parts: Vec<&str> = Vec::new();
    for segment in combined.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

fn parent_dir(path: &str) -> String {
    match path.rfind('/') {
        Some(index) => path[..index].to_owned(),
        None => String::new(),
    }
}

fn split_href(href: &str) -> (String, Option<String>) {
    match href.split_once('#') {
        Some((base, fragment)) => (base.to_owned(), Some(fragment.to_owned())),
        None => (href.to_owned(), None),
    }
}

fn word_count(blocks: &[CanonicalBlock]) -> usize {
    blocks.iter().map(CanonicalBlock::word_count).sum()
}

fn strip_anchors(blocks: &[CanonicalBlock]) -> Vec<CanonicalBlock> {
    blocks
        .iter()
        .filter(|block| !matches!(block, CanonicalBlock::Anchor { .. }))
        .cloned()
        .collect()
}

fn relabel_ids(blocks: Vec<CanonicalBlock>, prefix: &str) -> Vec<CanonicalBlock> {
    blocks
        .into_iter()
        .enumerate()
        .map(|(index, block)| set_id(block, format!("{prefix}-{index}")))
        .collect()
}

fn set_id(block: CanonicalBlock, id: String) -> CanonicalBlock {
    match block {
        CanonicalBlock::Heading { text, level, .. } => CanonicalBlock::Heading { id, text, level },
        CanonicalBlock::Paragraph { text, .. } => CanonicalBlock::Paragraph { id, text },
        CanonicalBlock::Blockquote { text, .. } => CanonicalBlock::Blockquote { id, text },
        CanonicalBlock::ListItem { text, .. } => CanonicalBlock::ListItem { id, text },
        CanonicalBlock::SceneBreak { text, .. } => CanonicalBlock::SceneBreak { id, text },
        CanonicalBlock::Image {
            text, image_source, ..
        } => CanonicalBlock::Image {
            id,
            text,
            image_source,
        },
        CanonicalBlock::Anchor {
            text, anchor_id, ..
        } => CanonicalBlock::Anchor {
            id,
            text,
            anchor_id,
        },
    }
}

/// A paragraph long enough to be body text rather than a title.
fn looks_like_body_paragraph(block: &CanonicalBlock) -> bool {
    matches!(block, CanonicalBlock::Paragraph { .. }) && block.word_count() >= 12
}

/// A short paragraph with no terminal punctuation — how many publishers mark up
/// chapter titles.
fn looks_like_heading_candidate(block: &CanonicalBlock) -> bool {
    if !matches!(block, CanonicalBlock::Paragraph { .. }) {
        return false;
    }
    let words = block.word_count();
    let text = block.text();
    words > 0 && words <= 12 && text.chars().count() <= 90 && !text.ends_with(['.', '!', '?'])
}

fn promote_leading_headings(blocks: Vec<CanonicalBlock>) -> Vec<CanonicalBlock> {
    let Some(first_body) = blocks.iter().position(looks_like_body_paragraph) else {
        return blocks;
    };
    if first_body == 0 {
        return blocks;
    }
    let limit = first_body.min(3);
    blocks
        .into_iter()
        .enumerate()
        .map(|(index, block)| {
            if index < limit && looks_like_heading_candidate(&block) {
                CanonicalBlock::Heading {
                    id: block.id().to_owned(),
                    text: block.text().to_owned(),
                    level: Some(2),
                }
            } else {
                block
            }
        })
        .collect()
}

fn finalize_chapter_blocks(blocks: Vec<CanonicalBlock>) -> Vec<CanonicalBlock> {
    // A leading image is a chapter ornament, not content.
    let trimmed: Vec<CanonicalBlock> = blocks
        .into_iter()
        .filter(|block| !matches!(block, CanonicalBlock::Anchor { .. }))
        .enumerate()
        .filter(|(index, block)| !(*index == 0 && matches!(block, CanonicalBlock::Image { .. })))
        .map(|(_, block)| block)
        .collect();
    promote_leading_headings(trimmed)
}

/// Strip accents and case so labels compare the way a reader would read them.
fn normalize_label(label: &str) -> String {
    label
        .nfd()
        .filter(|char| !is_combining_mark(*char))
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .trim()
        .to_owned()
}

/// Give a chapter its own title when the source file had none.
fn with_synthetic_heading(
    blocks: Vec<CanonicalBlock>,
    label: &str,
    prefix: &str,
) -> Vec<CanonicalBlock> {
    let normalized = normalize_label(label);
    if normalized.is_empty() {
        return blocks;
    }
    if let Some(CanonicalBlock::Heading { text, .. }) = blocks.first()
        && normalize_label(text) == normalized
    {
        return blocks;
    }
    let mut result = Vec::with_capacity(blocks.len() + 1);
    result.push(CanonicalBlock::Heading {
        id: format!("{prefix}-heading"),
        text: label.to_owned(),
        level: Some(1),
    });
    result.extend(blocks);
    result
}

/// Titles that name front matter rather than a chapter, in English and
/// Portuguese, since the reader's own library mixes both.
fn is_front_matter_label(label: &str) -> bool {
    let normalized = normalize_label(label);
    const EXACT: [&str; 14] = [
        "capa",
        "cover",
        "pagina de titulo",
        "title page",
        "folha de rosto",
        "pagina de creditos",
        "creditos",
        "copyright",
        "sumario",
        "contents",
        "content",
        "indice",
        "rosto",
        "dedicatoria",
    ];
    EXACT.contains(&normalized.as_str())
        || normalized == "epigrafe"
        || normalized.starts_with("edicoes")
}

/// Read the table of contents, preferring EPUB3 nav and falling back to NCX.
fn read_toc(
    archive: &mut Archive,
    package: &crate::xml::Package,
    opf_dir: &str,
) -> (Vec<TocItem>, bool) {
    if let Some(nav) = package.nav_item()
        && let Ok(source) = read_text(archive, &normalize_href(opf_dir, &nav.href))
    {
        let entries: Vec<NavEntry> = parse_nav_toc(&source);
        if !entries.is_empty() {
            return (
                entries
                    .into_iter()
                    .map(|entry| TocItem {
                        label: entry.label,
                        href: entry.href,
                        depth: entry.depth,
                    })
                    .collect(),
                true,
            );
        }
    }
    if let Some(ncx) = package.ncx_item()
        && let Ok(source) = read_text(archive, &normalize_href(opf_dir, &ncx.href))
        && let Ok(entries) = parse_ncx(&source)
        && !entries.is_empty()
    {
        return (
            entries
                .into_iter()
                .map(|entry| TocItem {
                    label: entry.label,
                    href: entry.href,
                    depth: entry.depth,
                })
                .collect(),
            true,
        );
    }
    (Vec::new(), false)
}

/// Import an EPUB file.
///
/// Returns [`ImportError::NoReadableContent`] when nothing survives extraction;
/// every lesser problem becomes a diagnostic on the returned book.
pub fn import_epub(path: &Path) -> Result<CanonicalBook, ImportError> {
    let bytes = std::fs::read(path)?;
    let import_hash = hash_bytes(&bytes);
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| ImportError::Malformed(format!("Failed to read EPUB archive: {error}")))?;
    let mut diagnostics: Vec<ImportDiagnostic> = Vec::new();

    let mimetype = read_text(&mut archive, "mimetype").unwrap_or_default();
    if mimetype.trim() != "application/epub+zip" {
        diagnostics.push(ImportDiagnostic {
            severity: DiagnosticSeverity::Warning,
            message: "Archive mimetype is missing or not application/epub+zip.".to_owned(),
            context: Some("mimetype".to_owned()),
        });
    }

    let container = read_text(&mut archive, "META-INF/container.xml")?;
    let opf_path =
        parse_container(&container).map_err(|error| ImportError::Malformed(error.detail))?;
    let opf_dir = parent_dir(&opf_path);
    let package = parse_package(&read_text(&mut archive, &opf_path)?)
        .map_err(|error| ImportError::Malformed(error.detail))?;

    let title = package.title.clone().unwrap_or_else(|| {
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default()
            .to_owned()
    });
    let author = package
        .creator
        .clone()
        .unwrap_or_else(|| "Unknown author".to_owned());

    // Spine paths, in reading order, for chapters that span several files.
    let spine_paths: Vec<String> = package
        .spine
        .iter()
        .filter_map(|idref| package.item(idref))
        .map(|item| normalize_href(&opf_dir, &item.href))
        .collect();
    let spine_index: HashMap<&str, usize> = spine_paths
        .iter()
        .enumerate()
        .map(|(index, path)| (path.as_str(), index))
        .collect();

    let (mut toc, found_navigation) = read_toc(&mut archive, &package, &opf_dir);
    if !found_navigation {
        diagnostics.push(ImportDiagnostic {
            severity: DiagnosticSeverity::Warning,
            message: "Navigation document is missing or unreadable. Falling back to spine order."
                .to_owned(),
            context: Some("navigation".to_owned()),
        });
        toc = package
            .spine
            .iter()
            .enumerate()
            .filter_map(|(index, idref)| {
                package.item(idref).map(|item| TocItem {
                    label: format!("Chapter {}", index + 1),
                    href: item.href.clone(),
                    depth: 0,
                })
            })
            .collect();
    }
    for item in &mut toc {
        item.href = normalize_href(&opf_dir, &item.href);
    }

    // Files are parsed once even when several chapters slice into them.
    let mut file_blocks: HashMap<String, Vec<CanonicalBlock>> = HashMap::new();
    let mut chapters: Vec<CanonicalChapter> = Vec::new();

    for index in 0..toc.len() {
        let item = toc[index].clone();
        let (base_path, fragment) = split_href(&item.href);
        let next = toc.get(index + 1).map(|next| split_href(&next.href));
        let prefix = block_prefix(&format!("{}:{index}", item.href));

        let mut inject_heading = false;
        let mut blocks: Vec<CanonicalBlock>;

        let next_shares_file = next.as_ref().is_some_and(|(next_base, next_fragment)| {
            *next_base == base_path && next_fragment.is_some()
        });

        if fragment.is_some() || next_shares_file {
            let base_blocks =
                blocks_for_file(&mut archive, &mut file_blocks, &base_path).unwrap_or_default();
            let end_anchor = next
                .as_ref()
                .filter(|(next_base, _)| *next_base == base_path)
                .and_then(|(_, next_fragment)| next_fragment.clone());
            let start_anchor = fragment.clone().or_else(|| {
                end_anchor
                    .as_ref()
                    .and_then(|_| find_first_chapter_anchor(base_blocks))
            });
            blocks = slice_blocks_by_anchors(
                base_blocks,
                start_anchor.as_deref(),
                end_anchor.as_deref(),
            );
        } else if let Some(current_index) = spine_index.get(base_path.as_str()).copied() {
            // A table-of-contents entry can stand for a run of spine files.
            let next_index = next
                .as_ref()
                .and_then(|(next_base, _)| spine_index.get(next_base.as_str()).copied());
            let range_end = match next_index {
                Some(next_index) if next_index > current_index => next_index,
                _ => spine_paths.len(),
            };
            let chapter_paths = &spine_paths[current_index..range_end];
            let mut collected: Vec<CanonicalBlock> = Vec::new();
            for (offset, chapter_path) in chapter_paths.iter().enumerate() {
                let parsed = blocks_for_file(&mut archive, &mut file_blocks, chapter_path)
                    .unwrap_or_default();
                let without_anchors = strip_anchors(parsed);
                // A title-only opening file leaves the chapter without a heading.
                if offset == 0 && without_anchors.is_empty() && chapter_paths.len() > 1 {
                    inject_heading = true;
                }
                collected.extend(without_anchors);
            }
            blocks = collected;
        } else {
            let base_blocks =
                blocks_for_file(&mut archive, &mut file_blocks, &base_path).unwrap_or_default();
            blocks = strip_anchors(base_blocks)
                .into_iter()
                .enumerate()
                .map(|(block_index, block)| {
                    let id = format!("{}-{block_index}", block.id());
                    set_id(block, id)
                })
                .collect();
        }

        blocks = finalize_chapter_blocks(blocks);
        if inject_heading {
            blocks = with_synthetic_heading(blocks, &item.label, &prefix);
        }
        blocks = relabel_ids(blocks, &prefix);

        if blocks.is_empty() {
            diagnostics.push(ImportDiagnostic {
                severity: DiagnosticSeverity::Warning,
                message: format!("Chapter \"{}\" resolved to empty content.", item.label),
                context: Some(item.href.clone()),
            });
        }
        chapters.push(CanonicalChapter {
            id: chapter_id(&item.href, index),
            index,
            title: item.label.clone(),
            href: item.href.clone(),
            depth: item.depth,
            word_count: word_count(&blocks),
            blocks,
        });
    }

    let readable: Vec<CanonicalChapter> = chapters
        .into_iter()
        .filter(|chapter| !chapter.blocks.is_empty() && !is_front_matter_label(&chapter.title))
        .enumerate()
        .map(|(index, chapter)| CanonicalChapter { index, ..chapter })
        .collect();

    if readable.is_empty() {
        return Err(ImportError::NoReadableContent(
            "Failed to extract readable chapters from EPUB".to_owned(),
        ));
    }

    let absolute = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    Ok(CanonicalBook {
        id: epub_book_id(&absolute),
        title,
        author,
        source_path: absolute.display().to_string(),
        import_hash,
        parser_version: Some(EPUB_PARSER_VERSION),
        diagnostics,
        chapters: readable,
        cover_path: None,
    })
}

fn blocks_for_file<'cache>(
    archive: &mut Archive,
    cache: &'cache mut HashMap<String, Vec<CanonicalBlock>>,
    path: &str,
) -> Option<&'cache [CanonicalBlock]> {
    if !cache.contains_key(path) {
        let source = read_text(archive, path).ok()?;
        let blocks = extract_blocks_from_html(&source, &block_prefix(path));
        cache.insert(path.to_owned(), blocks);
    }
    cache.get(path).map(Vec::as_slice)
}

#[cfg(test)]
mod tests {
    use super::{is_front_matter_label, normalize_href, normalize_label, split_href};

    #[test]
    fn hrefs_resolve_against_the_package_directory() {
        assert_eq!(
            normalize_href("OEBPS", "text/ch1.xhtml"),
            "OEBPS/text/ch1.xhtml"
        );
        assert_eq!(
            normalize_href("OEBPS/text", "../images/a.png"),
            "OEBPS/images/a.png"
        );
        assert_eq!(normalize_href("", "content.opf"), "content.opf");
        assert_eq!(normalize_href(".", "content.opf"), "content.opf");
        assert_eq!(normalize_href("OEBPS", "./a/./b.xhtml"), "OEBPS/a/b.xhtml");
    }

    #[test]
    fn fragments_split_off_the_base_path() {
        assert_eq!(
            split_href("text/ch1.xhtml#start"),
            ("text/ch1.xhtml".to_owned(), Some("start".to_owned()))
        );
        assert_eq!(
            split_href("text/ch1.xhtml"),
            ("text/ch1.xhtml".to_owned(), None)
        );
    }

    #[test]
    fn labels_normalize_away_case_and_accents() {
        assert_eq!(normalize_label("  Sumário "), "sumario");
        assert_eq!(normalize_label("DEDICATÓRIA"), "dedicatoria");
    }

    #[test]
    fn front_matter_titles_are_recognized_in_both_languages() {
        for label in [
            "Capa",
            "cover",
            "Título",
            "Sumário",
            "Contents",
            "Copyright",
            "Folha de Rosto",
            "Edições anteriores",
        ] {
            let expected = !matches!(label, "Título");
            assert_eq!(
                is_front_matter_label(label),
                expected,
                "{label:?} was classified wrongly"
            );
        }
        assert!(!is_front_matter_label("Chapter One"));
        assert!(!is_front_matter_label("Capítulo 1"));
    }
}

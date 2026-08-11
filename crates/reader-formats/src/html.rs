//! HTML to canonical blocks.
//!
//! Parsing uses html5ever, the same spec-compliant tree builder family v1 used
//! through parse5, so malformed markup recovers identically. Block extraction
//! walks the body in document order and keeps anchors as zero-width markers so an
//! EPUB chapter can later be sliced at a fragment.

use html5ever::driver::ParseOpts;
use html5ever::parse_document;
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use reader_core::CanonicalBlock;
use unicode_general_category::{GeneralCategory, get_general_category};

/// Elements that become a block of their own.
const BLOCK_NAMES: [&str; 11] = [
    "p",
    "blockquote",
    "li",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "img",
    "hr",
];

/// Elements whose text is never part of the reading content.
const SKIP_TEXT_TAGS: [&str; 6] = ["script", "style", "svg", "title", "head", "noscript"];

/// Collapse runs of whitespace and trim, as v1's `normalizeText` did.
fn normalize_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_space = false;
    for char in text.chars() {
        if char.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        out.push(char);
    }
    out
}

/// Whether every character is punctuation, a symbol, or a private-use glyph —
/// the ornaments and dingbats that carry no reading content.
fn is_decorative_text(text: &str) -> bool {
    let normalized = normalize_text(text);
    if normalized.is_empty() {
        return true;
    }
    normalized.chars().all(|char| {
        matches!(
            get_general_category(char),
            GeneralCategory::ConnectorPunctuation
                | GeneralCategory::DashPunctuation
                | GeneralCategory::OpenPunctuation
                | GeneralCategory::ClosePunctuation
                | GeneralCategory::InitialPunctuation
                | GeneralCategory::FinalPunctuation
                | GeneralCategory::OtherPunctuation
                | GeneralCategory::MathSymbol
                | GeneralCategory::CurrencySymbol
                | GeneralCategory::ModifierSymbol
                | GeneralCategory::OtherSymbol
        ) || ('\u{e000}'..='\u{f8ff}').contains(&char)
    })
}

/// Whether an `alt` attribute only names the file rather than describing it.
fn is_decorative_image_alt(text: &str) -> bool {
    let normalized = normalize_text(text);
    if normalized.is_empty() {
        return true;
    }
    let lowered = normalized.to_lowercase();
    if lowered == "image" || lowered == "cover" {
        return true;
    }
    if let Some(rest) = lowered.strip_prefix("img")
        && (rest.is_empty() || rest.chars().all(|char| char.is_ascii_digit()))
    {
        return true;
    }
    // A bare file name, e.g. `plate-01.jpeg`, with at least one character before
    // the extension.
    if let Some((stem, extension)) = lowered.rsplit_once('.')
        && !stem.is_empty()
        && matches!(extension, "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp")
    {
        return true;
    }
    false
}

fn tag_name(node: &Handle) -> Option<String> {
    match &node.data {
        NodeData::Element { name, .. } => Some(name.local.to_string()),
        _ => None,
    }
}

fn attribute(node: &Handle, wanted: &str) -> Option<String> {
    let NodeData::Element { attrs, .. } = &node.data else {
        return None;
    };
    attrs
        .borrow()
        .iter()
        .find(|attribute| {
            // Match on the qualified name so `epub:type` is found by that name.
            let name = match &attribute.name.prefix {
                Some(prefix) => format!("{prefix}:{}", attribute.name.local),
                None => attribute.name.local.to_string(),
            };
            name == wanted
        })
        .map(|attribute| attribute.value.to_string())
}

fn children(node: &Handle) -> Vec<Handle> {
    node.children.borrow().clone()
}

/// Concatenate the text of a subtree, turning `<br>` into a newline and skipping
/// non-content elements.
fn collect_text(node: &Handle) -> String {
    if let Some(name) = tag_name(node) {
        if SKIP_TEXT_TAGS.contains(&name.as_str()) {
            return String::new();
        }
        if name == "br" {
            return "\n".to_owned();
        }
    }
    if let NodeData::Text { contents } = &node.data {
        return contents.borrow().to_string();
    }
    children(node).iter().map(collect_text).collect()
}

/// Sequential block-id counter, shared across one document.
struct Counter(usize);

impl Counter {
    fn next(&mut self) -> usize {
        let value = self.0;
        self.0 += 1;
        value
    }
}

fn push_anchor(
    blocks: &mut Vec<CanonicalBlock>,
    node: &Handle,
    prefix: &str,
    counter: &mut Counter,
) {
    if let Some(id) = attribute(node, "id") {
        blocks.push(CanonicalBlock::Anchor {
            id: format!("{prefix}-anchor-{}", counter.next()),
            text: String::new(),
            anchor_id: Some(id),
        });
    }
}

/// Anchors inside a block element still need to be reachable as slice points.
fn push_descendant_anchors(
    blocks: &mut Vec<CanonicalBlock>,
    node: &Handle,
    prefix: &str,
    counter: &mut Counter,
) {
    for child in children(node) {
        push_anchor(blocks, &child, prefix, counter);
        push_descendant_anchors(blocks, &child, prefix, counter);
    }
}

fn visit(node: &Handle, blocks: &mut Vec<CanonicalBlock>, prefix: &str, counter: &mut Counter) {
    let Some(name) = tag_name(node) else {
        return;
    };
    if SKIP_TEXT_TAGS.contains(&name.as_str()) {
        return;
    }
    push_anchor(blocks, node, prefix, counter);

    if !BLOCK_NAMES.contains(&name.as_str()) {
        for child in children(node) {
            visit(&child, blocks, prefix, counter);
        }
        return;
    }

    push_descendant_anchors(blocks, node, prefix, counter);

    if name == "img" {
        let alt = normalize_text(&attribute(node, "alt").unwrap_or_default());
        if is_decorative_image_alt(&alt) {
            return;
        }
        blocks.push(CanonicalBlock::Image {
            id: format!("{prefix}-block-{}", counter.next()),
            text: alt,
            image_source: attribute(node, "src"),
        });
        return;
    }
    if name == "hr" {
        blocks.push(CanonicalBlock::SceneBreak {
            id: format!("{prefix}-block-{}", counter.next()),
            text: "Scene break".to_owned(),
        });
        return;
    }

    let text = normalize_text(&collect_text(node));
    if is_decorative_text(&text) {
        return;
    }
    let id = format!("{prefix}-block-{}", counter.next());
    blocks.push(match name.as_str() {
        "blockquote" => CanonicalBlock::Blockquote { id, text },
        "li" => CanonicalBlock::ListItem { id, text },
        heading if heading.starts_with('h') => CanonicalBlock::Heading {
            id,
            text,
            level: heading[1..].parse().ok(),
        },
        _ => CanonicalBlock::Paragraph { id, text },
    });
}

fn parse_html(source: &str) -> RcDom {
    parse_document(RcDom::default(), ParseOpts::default()).one(source)
}

fn find_child(node: &Handle, wanted: &str) -> Option<Handle> {
    children(node)
        .into_iter()
        .find(|child| tag_name(child).is_some_and(|name| name == wanted))
}

/// The `<body>`, falling back to `<html>` and then the document itself, so a
/// fragment without a body still yields blocks.
fn find_body(document: &Handle) -> Handle {
    let Some(html) = find_child(document, "html") else {
        return document.clone();
    };
    find_child(&html, "body").unwrap_or(html)
}

/// Extract canonical blocks from an HTML document.
///
/// `prefix` namespaces the generated block ids; v1 derived it from the archive
/// path so ids stay stable across imports.
#[must_use]
pub fn extract_blocks_from_html(source: &str, prefix: &str) -> Vec<CanonicalBlock> {
    let dom = parse_html(source);
    let body = find_body(&dom.document);
    let mut blocks: Vec<CanonicalBlock> = Vec::new();
    // v1 started its counter at 1.
    let mut counter = Counter(1);
    for child in children(&body) {
        visit(&child, &mut blocks, prefix, &mut counter);
    }
    blocks
}

/// One entry of an EPUB3 navigation document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavEntry {
    pub href: String,
    pub label: String,
    /// Nesting depth, counted through nested `<ol>` inside `<li>`.
    pub depth: usize,
}

fn find_nav_toc(node: &Handle) -> Option<Handle> {
    if tag_name(node).is_some_and(|name| name == "nav") {
        let epub_type = attribute(node, "epub:type")
            .or_else(|| attribute(node, "type"))
            .unwrap_or_default();
        let id = attribute(node, "id").unwrap_or_default();
        if epub_type.contains("toc") || id == "toc" {
            return Some(node.clone());
        }
    }
    children(node).iter().find_map(find_nav_toc)
}

fn collect_nav(node: &Handle, depth: usize, items: &mut Vec<NavEntry>) {
    if tag_name(node).is_some_and(|name| name == "a") {
        if let Some(href) = attribute(node, "href") {
            let label = normalize_text(&collect_text(node));
            items.push(NavEntry {
                href,
                label: if label.is_empty() {
                    "Untitled chapter".to_owned()
                } else {
                    label
                },
                depth,
            });
        }
        return;
    }
    let is_list_item = tag_name(node).is_some_and(|name| name == "li");
    for child in children(node) {
        let nested = is_list_item && tag_name(&child).is_some_and(|name| name == "ol");
        collect_nav(&child, if nested { depth + 1 } else { depth }, items);
    }
}

/// Parse an EPUB3 navigation document's table of contents.
#[must_use]
pub fn parse_nav_toc(source: &str) -> Vec<NavEntry> {
    let dom = parse_html(source);
    let mut items: Vec<NavEntry> = Vec::new();
    if let Some(nav) = find_nav_toc(&dom.document) {
        collect_nav(&nav, 0, &mut items);
    }
    items
}

fn anchor_id(block: &CanonicalBlock) -> Option<&str> {
    match block {
        CanonicalBlock::Anchor { anchor_id, .. } => anchor_id.as_deref(),
        _ => None,
    }
}

fn block_id_mut(block: &mut CanonicalBlock) -> &mut String {
    match block {
        CanonicalBlock::Heading { id, .. }
        | CanonicalBlock::Paragraph { id, .. }
        | CanonicalBlock::Blockquote { id, .. }
        | CanonicalBlock::ListItem { id, .. }
        | CanonicalBlock::SceneBreak { id, .. }
        | CanonicalBlock::Image { id, .. }
        | CanonicalBlock::Anchor { id, .. } => id,
    }
}

/// Take the blocks between two anchors, dropping the anchors themselves.
///
/// A missing anchor is ignored rather than fatal: the slice then runs from the
/// start or to the end, which is what v1 did when a fragment did not resolve.
#[must_use]
pub fn slice_blocks_by_anchors(
    blocks: &[CanonicalBlock],
    start_anchor: Option<&str>,
    end_anchor: Option<&str>,
) -> Vec<CanonicalBlock> {
    let mut start_index = 0usize;
    let mut end_index = blocks.len();

    if let Some(wanted) = start_anchor
        && let Some(found) = blocks
            .iter()
            .position(|block| anchor_id(block) == Some(wanted))
    {
        start_index = found;
    }
    if let Some(wanted) = end_anchor
        && let Some(found) = blocks
            .iter()
            .enumerate()
            .skip(start_index + 1)
            .find(|(_, block)| anchor_id(block) == Some(wanted))
            .map(|(index, _)| index)
    {
        end_index = found;
    }

    blocks[start_index..end_index]
        .iter()
        .filter(|block| !matches!(block, CanonicalBlock::Anchor { .. }))
        .enumerate()
        .map(|(index, block)| {
            let mut block = block.clone();
            let id = block_id_mut(&mut block);
            *id = format!("{id}-{index}");
            block
        })
        .collect()
}

/// Whether an anchor id looks like a chapter marker (`capitulo3`, `ch12`, …).
fn is_chapter_anchor(anchor: &str) -> bool {
    let lowered = anchor.to_lowercase();
    ["capitulo", "chapter", "ch", "cap"]
        .into_iter()
        .any(|stem| {
            lowered.strip_prefix(stem).is_some_and(|digits| {
                !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
            })
        })
}

/// The first anchor that looks like a chapter start, used when a table of
/// contents points at a file without naming a fragment.
#[must_use]
pub fn find_first_chapter_anchor(blocks: &[CanonicalBlock]) -> Option<String> {
    blocks
        .iter()
        .filter_map(anchor_id)
        .find(|anchor| is_chapter_anchor(anchor))
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::{
        extract_blocks_from_html, find_first_chapter_anchor, is_decorative_image_alt,
        is_decorative_text, normalize_text, parse_nav_toc, slice_blocks_by_anchors,
    };
    use reader_core::{BlockKind, CanonicalBlock};

    fn kinds(blocks: &[CanonicalBlock]) -> Vec<BlockKind> {
        blocks.iter().map(CanonicalBlock::kind).collect()
    }

    fn texts(blocks: &[CanonicalBlock]) -> Vec<String> {
        blocks.iter().map(|block| block.text().to_owned()).collect()
    }

    #[test]
    fn structural_elements_become_their_canonical_blocks() {
        let blocks = extract_blocks_from_html(
            "<html><body><h2>One</h2><p>Prose here.</p><blockquote>Quoted.</blockquote>\
             <ul><li>First</li><li>Second</li></ul><hr/><img src=\"map.png\" alt=\"Map of Arrakis\"/>\
             </body></html>",
            "pfx",
        );
        assert_eq!(
            kinds(&blocks),
            vec![
                BlockKind::Heading,
                BlockKind::Paragraph,
                BlockKind::Blockquote,
                BlockKind::ListItem,
                BlockKind::ListItem,
                BlockKind::SceneBreak,
                BlockKind::Image,
            ]
        );
        assert!(matches!(
            &blocks[0],
            CanonicalBlock::Heading { level: Some(2), .. }
        ));
        assert_eq!(blocks[5].text(), "Scene break");
        assert!(matches!(
            &blocks[6],
            CanonicalBlock::Image { image_source: Some(source), .. } if source == "map.png"
        ));
    }

    #[test]
    fn block_ids_are_prefixed_and_sequential_from_one() {
        let blocks = extract_blocks_from_html("<body><p>a</p><p>b</p></body>", "abc123");
        assert_eq!(blocks[0].id(), "abc123-block-1");
        assert_eq!(blocks[1].id(), "abc123-block-2");
    }

    #[test]
    fn whitespace_is_collapsed_and_line_breaks_become_spaces() {
        let blocks =
            extract_blocks_from_html("<body><p>  one   two<br/>three\n\nfour  </p></body>", "p");
        assert_eq!(texts(&blocks), vec!["one two three four"]);
    }

    #[test]
    fn scripts_styles_and_head_content_are_skipped() {
        let blocks = extract_blocks_from_html(
            "<html><head><title>Title</title></head><body><script>var x = 1;</script>\
             <style>p { color: red }</style><noscript><p>fallback</p></noscript><p>Real text.</p></body></html>",
            "p",
        );
        assert_eq!(texts(&blocks), vec!["Real text."]);
    }

    #[test]
    fn decorative_text_and_image_alts_are_dropped() {
        assert!(is_decorative_text("***"));
        assert!(is_decorative_text("···"));
        assert!(is_decorative_text("   "));
        assert!(!is_decorative_text("Real words"));
        // Spaces are neither punctuation nor symbols, so a spaced-out ornament
        // survives — v1 behaved the same way.
        assert!(!is_decorative_text("* * *"));

        for alt in [
            "",
            "image",
            "COVER",
            "img",
            "img12",
            "plate-01.jpeg",
            "a.svg",
        ] {
            assert!(is_decorative_image_alt(alt), "{alt:?} should be decorative");
        }
        for alt in ["Map of Arrakis", "The cover of the book", "img of a dog"] {
            assert!(!is_decorative_image_alt(alt), "{alt:?} should be kept");
        }

        let blocks = extract_blocks_from_html(
            "<body><p>···</p><img src=\"a.png\" alt=\"img3\"/><p>Kept.</p></body>",
            "p",
        );
        assert_eq!(texts(&blocks), vec!["Kept."]);
    }

    #[test]
    fn anchors_are_recorded_as_zero_width_markers() {
        let blocks = extract_blocks_from_html(
            "<body><div id=\"top\"><p id=\"start\">One</p></div><p>Two</p></body>",
            "p",
        );
        let anchors: Vec<&str> = blocks
            .iter()
            .filter_map(|block| match block {
                CanonicalBlock::Anchor { anchor_id, .. } => anchor_id.as_deref(),
                _ => None,
            })
            .collect();
        assert_eq!(anchors, vec!["top", "start"]);
    }

    #[test]
    fn malformed_markup_still_yields_blocks() {
        let blocks = extract_blocks_from_html("<p>unclosed<p>second", "p");
        assert_eq!(texts(&blocks), vec!["unclosed", "second"]);
    }

    #[test]
    fn a_fragment_without_html_or_body_is_still_parsed() {
        let blocks = extract_blocks_from_html("<p>bare</p>", "p");
        assert_eq!(texts(&blocks), vec!["bare"]);
    }

    #[test]
    fn nav_documents_yield_hrefs_labels_and_nesting() {
        let entries = parse_nav_toc(
            "<html><body><nav epub:type=\"toc\"><ol>\
               <li><a href=\"ch1.xhtml\">Chapter One</a>\
                 <ol><li><a href=\"ch1.xhtml#part2\">Part Two</a></li></ol></li>\
               <li><a href=\"ch2.xhtml\"> </a></li>\
             </ol></nav></body></html>",
        );
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].href, "ch1.xhtml");
        assert_eq!(entries[0].depth, 0);
        assert_eq!(entries[1].label, "Part Two");
        assert_eq!(entries[1].depth, 1);
        assert_eq!(entries[2].label, "Untitled chapter");
    }

    #[test]
    fn a_nav_can_also_be_found_by_its_id() {
        let entries = parse_nav_toc("<body><nav id=\"toc\"><a href=\"a.xhtml\">A</a></nav></body>");
        assert_eq!(entries.len(), 1);
        assert!(
            parse_nav_toc("<body><nav id=\"landmarks\"><a href=\"a\">A</a></nav></body>")
                .is_empty()
        );
    }

    #[test]
    fn slicing_by_anchors_drops_the_markers_and_renumbers_ids() {
        let blocks = extract_blocks_from_html(
            "<body><h1 id=\"start\">One</h1><p>First.</p><h2 id=\"middle\">Two</h2><p>Second.</p></body>",
            "p",
        );
        let first = slice_blocks_by_anchors(&blocks, Some("start"), Some("middle"));
        assert_eq!(texts(&first), vec!["One", "First."]);
        assert!(
            first
                .iter()
                .all(|block| !matches!(block, CanonicalBlock::Anchor { .. }))
        );
        assert!(first[0].id().ends_with("-0"));
        assert!(first[1].id().ends_with("-1"));

        let second = slice_blocks_by_anchors(&blocks, Some("middle"), None);
        assert_eq!(texts(&second), vec!["Two", "Second."]);
    }

    #[test]
    fn an_unresolved_anchor_falls_back_to_the_whole_range() {
        let blocks = extract_blocks_from_html("<body><p>a</p><p>b</p></body>", "p");
        assert_eq!(
            texts(&slice_blocks_by_anchors(
                &blocks,
                Some("nope"),
                Some("also-nope")
            )),
            vec!["a", "b"]
        );
    }

    #[test]
    fn chapter_anchors_are_recognized_by_their_shape() {
        let blocks = extract_blocks_from_html(
            "<body><p id=\"header\">x</p><p id=\"capitulo3\">y</p></body>",
            "p",
        );
        assert_eq!(
            find_first_chapter_anchor(&blocks).as_deref(),
            Some("capitulo3")
        );
        let none = extract_blocks_from_html("<body><p id=\"foreword\">x</p></body>", "p");
        assert!(find_first_chapter_anchor(&none).is_none());
    }

    #[test]
    fn normalizing_keeps_interior_single_spaces() {
        assert_eq!(normalize_text("  a \n b\tc  "), "a b c");
        assert_eq!(normalize_text("   "), "");
    }
}

//! Block rendering for plain and code modes.
//!
//! Rendering is a pure function of the block, the width, the palette, and the
//! block's absolute index — never of scroll position — so a chapter renders
//! identically no matter how the reader arrived there.

pub mod code;
pub mod plain;
pub mod text;

use crate::book::CanonicalBlock;
use crate::settings::{CodeDensity, CodeLanguage, LineSpacing, RenderMode};
use crate::style::{Style, StyledLine};
use crate::theme::Palette;

pub use code::render_code;
pub use plain::{highlight_dialogue, render_plain, wrap_with_dialogue};
pub use text::{line_hash, wrap_text};

/// Everything that affects rendered output. Two calls with equal options and
/// equal blocks always produce equal lines, which is what lets the reader cache
/// a rendered chapter keyed on these values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderOptions<'a> {
    pub mode: RenderMode,
    pub width: usize,
    pub palette: &'a Palette,
    pub code_language: CodeLanguage,
    pub code_density: CodeDensity,
    pub plain_highlight: bool,
    pub line_spacing: LineSpacing,
    /// Absolute index of the first block, so disguise patterns stay stable when
    /// rendering a slice of a chapter.
    pub block_index_offset: usize,
    /// Whether the last block contributes its trailing blank lines.
    pub include_trailing_spacing: bool,
    /// Search term to highlight, if a search is active.
    pub search_query: Option<&'a str>,
}

impl<'a> RenderOptions<'a> {
    /// Plain-mode defaults for `palette`.
    #[must_use]
    pub const fn new(palette: &'a Palette, width: usize) -> Self {
        Self {
            mode: RenderMode::Plain,
            width,
            palette,
            code_language: CodeLanguage::TypeScript,
            code_density: CodeDensity::DEFAULT,
            plain_highlight: true,
            line_spacing: LineSpacing::Normal,
            block_index_offset: 0,
            include_trailing_spacing: true,
            search_query: None,
        }
    }

    #[must_use]
    pub const fn with_mode(mut self, mode: RenderMode) -> Self {
        self.mode = mode;
        self
    }

    #[must_use]
    pub const fn with_code(mut self, language: CodeLanguage, density: CodeDensity) -> Self {
        self.mode = RenderMode::Code;
        self.code_language = language;
        self.code_density = density;
        self
    }

    #[must_use]
    pub const fn with_line_spacing(mut self, spacing: LineSpacing) -> Self {
        self.line_spacing = spacing;
        self
    }

    #[must_use]
    pub const fn with_block_offset(mut self, offset: usize) -> Self {
        self.block_index_offset = offset;
        self
    }

    #[must_use]
    pub const fn with_trailing_spacing(mut self, include: bool) -> Self {
        self.include_trailing_spacing = include;
        self
    }

    #[must_use]
    pub const fn with_search(mut self, query: Option<&'a str>) -> Self {
        self.search_query = query;
        self
    }

    #[must_use]
    pub const fn with_plain_highlight(mut self, highlight: bool) -> Self {
        self.plain_highlight = highlight;
        self
    }
}

/// Render one block without any inter-block spacing.
#[must_use]
pub fn render_block(
    block: &CanonicalBlock,
    options: &RenderOptions<'_>,
    block_index: usize,
) -> Vec<StyledLine> {
    match options.mode {
        RenderMode::Plain => render_plain(
            block,
            options.width,
            options.palette,
            options.plain_highlight,
        ),
        RenderMode::Code => render_code(
            block,
            options.width,
            options.palette,
            block_index,
            options.code_language,
            options.code_density,
        ),
    }
}

/// Blank lines to emit after the block at `block_index`.
///
/// Code mode varies the gap so the page does not look mechanically spaced: 70%
/// of blocks get one blank line, 20% get none, and 10% get two. Line spacing
/// then shifts that by one in either direction.
fn spacing_after(block_index: usize, options: &RenderOptions<'_>) -> usize {
    let normal: usize = if options.mode == RenderMode::Code {
        match line_hash(block_index, 999) % 10 {
            0..=6 => 1,
            7..=8 => 0,
            _ => 2,
        }
    } else {
        1
    };
    match options.line_spacing {
        LineSpacing::Compact => normal.saturating_sub(1),
        LineSpacing::Normal => normal,
        LineSpacing::Relaxed => normal + 1,
    }
}

/// Render a run of blocks into terminal lines.
#[must_use]
pub fn render_blocks(blocks: &[CanonicalBlock], options: &RenderOptions<'_>) -> Vec<StyledLine> {
    let mut lines: Vec<StyledLine> = Vec::new();
    for (offset, block) in blocks.iter().enumerate() {
        let block_index = options.block_index_offset + offset;
        let rendered = render_block(block, options, block_index);
        let last_rendered = rendered.len().saturating_sub(1);
        for (line_index, line) in rendered.into_iter().enumerate() {
            lines.push(line);
            // Relaxed spacing also breathes inside a wrapped block.
            if options.line_spacing == LineSpacing::Relaxed && line_index < last_rendered {
                lines.push(StyledLine::empty());
            }
        }
        if options.include_trailing_spacing || offset < blocks.len() - 1 {
            for _ in 0..spacing_after(block_index, options) {
                lines.push(StyledLine::empty());
            }
        }
    }

    if let Some(query) = options.search_query.filter(|query| !query.is_empty()) {
        let marker = Style::new()
            .with_bg(options.palette.warning)
            .with_fg(options.palette.background);
        return lines
            .into_iter()
            .map(|line| line.highlight(query, marker))
            .collect();
    }
    lines
}

/// Count the lines [`render_blocks`] would produce without allocating them.
///
/// Search highlighting and colors do not change geometry. All remaining render
/// inputs are honored, including code-language structures and relaxed spacing.
#[must_use]
pub fn count_rendered_lines(blocks: &[CanonicalBlock], options: &RenderOptions<'_>) -> usize {
    blocks
        .iter()
        .enumerate()
        .map(|(offset, block)| {
            let block_index = options.block_index_offset + offset;
            let rendered = match options.mode {
                RenderMode::Plain => plain::line_count(block, options.width),
                RenderMode::Code => {
                    code::line_count(block, options.width, block_index, options.code_language)
                }
            };
            let internal_spacing = if options.line_spacing == LineSpacing::Relaxed {
                rendered.saturating_sub(1)
            } else {
                0
            };
            let trailing_spacing = if options.include_trailing_spacing || offset + 1 < blocks.len()
            {
                spacing_after(block_index, options)
            } else {
                0
            };
            rendered + internal_spacing + trailing_spacing
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::{RenderOptions, count_rendered_lines, render_blocks};
    use crate::book::CanonicalBlock;
    use crate::settings::{CodeDensity, CodeLanguage, LineSpacing, RenderMode};
    use crate::style::Style;
    use crate::theme::Theme;

    fn paragraph(id: &str, text: &str) -> CanonicalBlock {
        CanonicalBlock::Paragraph {
            id: id.into(),
            text: text.into(),
        }
    }

    fn texts(lines: &[crate::style::StyledLine]) -> Vec<String> {
        lines.iter().map(crate::style::StyledLine::text).collect()
    }

    #[test]
    fn plain_mode_separates_blocks_with_one_blank_line() {
        let theme = Theme::default();
        let blocks = vec![paragraph("a", "first"), paragraph("b", "second")];
        let options = RenderOptions::new(&theme.palette, 40);

        assert_eq!(
            texts(&render_blocks(&blocks, &options)),
            vec!["first", "", "second", ""]
        );
    }

    #[test]
    fn trailing_spacing_can_be_suppressed() {
        let theme = Theme::default();
        let blocks = vec![paragraph("a", "first"), paragraph("b", "second")];
        let options = RenderOptions::new(&theme.palette, 40).with_trailing_spacing(false);

        assert_eq!(
            texts(&render_blocks(&blocks, &options)),
            vec!["first", "", "second"]
        );
    }

    #[test]
    fn compact_spacing_removes_the_gap_and_relaxed_widens_it() {
        let theme = Theme::default();
        let blocks = vec![paragraph("a", "first"), paragraph("b", "second")];

        let compact = render_blocks(
            &blocks,
            &RenderOptions::new(&theme.palette, 40).with_line_spacing(LineSpacing::Compact),
        );
        assert_eq!(texts(&compact), vec!["first", "second"]);

        let relaxed = render_blocks(
            &blocks,
            &RenderOptions::new(&theme.palette, 40).with_line_spacing(LineSpacing::Relaxed),
        );
        assert_eq!(texts(&relaxed), vec!["first", "", "", "second", "", ""]);
    }

    #[test]
    fn relaxed_spacing_also_breathes_between_wrapped_lines() {
        let theme = Theme::default();
        let blocks = vec![paragraph("a", "one two three four")];
        let rendered = render_blocks(
            &blocks,
            &RenderOptions::new(&theme.palette, 8).with_line_spacing(LineSpacing::Relaxed),
        );
        assert_eq!(
            texts(&rendered),
            vec!["one two", "", "three", "", "four", "", ""]
        );
    }

    #[test]
    fn code_mode_spacing_varies_but_stays_deterministic() {
        let theme = Theme::default();
        let blocks: Vec<CanonicalBlock> = (0..40)
            .map(|index| paragraph(&format!("b{index}"), "the quiet harbour at dawn"))
            .collect();
        let options = RenderOptions::new(&theme.palette, 80)
            .with_code(CodeLanguage::TypeScript, CodeDensity::DEFAULT);

        let first = render_blocks(&blocks, &options);
        assert_eq!(texts(&first), texts(&render_blocks(&blocks, &options)));

        // Not every gap is the same size.
        let blank_runs: Vec<usize> = texts(&first)
            .split(|line| !line.is_empty())
            .map(<[String]>::len)
            .filter(|length| *length > 0)
            .collect();
        assert!(blank_runs.contains(&2));
    }

    #[test]
    fn the_block_offset_keeps_disguise_patterns_stable_across_slices() {
        let theme = Theme::default();
        let blocks: Vec<CanonicalBlock> = (0..6)
            .map(|index| paragraph(&format!("b{index}"), "the quiet harbour at dawn"))
            .collect();
        let options = RenderOptions::new(&theme.palette, 80)
            .with_code(CodeLanguage::Rust, CodeDensity::DEFAULT);

        let whole = render_blocks(&blocks, &options);
        let tail = render_blocks(&blocks[3..], &options.clone().with_block_offset(3));
        let whole_texts = texts(&whole);
        let tail_texts = texts(&tail);
        assert_eq!(
            tail_texts,
            whole_texts[whole_texts.len() - tail_texts.len()..].to_vec()
        );
    }

    #[test]
    fn search_highlighting_marks_matches_in_every_mode() {
        let theme = Theme::default();
        let blocks = vec![paragraph("a", "The quiet harbour at dawn")];
        let marker = Style::new()
            .with_bg(theme.palette.warning)
            .with_fg(theme.palette.background);

        for mode in [RenderMode::Plain, RenderMode::Code] {
            let options = RenderOptions::new(&theme.palette, 80)
                .with_mode(mode)
                .with_search(Some("harbour"));
            let rendered = render_blocks(&blocks, &options);
            let marked: Vec<String> = rendered
                .iter()
                .flat_map(|line| line.spans.iter())
                .filter(|span| span.style == marker)
                .map(|span| span.text.clone())
                .collect();
            assert_eq!(marked, vec!["harbour"], "mode {mode:?} lost the match");
        }
    }

    #[test]
    fn an_empty_search_query_changes_nothing() {
        let theme = Theme::default();
        let blocks = vec![paragraph("a", "the quiet harbour")];
        let plain = render_blocks(&blocks, &RenderOptions::new(&theme.palette, 40));
        let searched = render_blocks(
            &blocks,
            &RenderOptions::new(&theme.palette, 40).with_search(Some("")),
        );
        assert_eq!(plain, searched);
    }

    #[test]
    fn line_count_matches_rendering_for_every_render_configuration() {
        let theme = Theme::default();
        let blocks = vec![
            CanonicalBlock::Heading {
                id: "heading".into(),
                text: "A quiet harbour at dawn".into(),
                level: Some(2),
            },
            CanonicalBlock::Paragraph {
                id: "paragraph".into(),
                text: "She said “come in” while the tide worked at the old stones below.".into(),
            },
            CanonicalBlock::Blockquote {
                id: "quote".into(),
                text: "A word much longer than the available line width follows: supercalifragilistic.".into(),
            },
            CanonicalBlock::ListItem {
                id: "item".into(),
                text: "A list entry that wraps onto more than one terminal line.".into(),
            },
            CanonicalBlock::SceneBreak {
                id: "scene".into(),
                text: String::new(),
            },
            CanonicalBlock::Image {
                id: "image".into(),
                text: "the harbour".into(),
                image_source: None,
            },
            CanonicalBlock::Anchor {
                id: "anchor".into(),
                text: "anchored prose".into(),
                anchor_id: None,
            },
        ];

        for mode in [RenderMode::Plain, RenderMode::Code] {
            for language in [
                CodeLanguage::TypeScript,
                CodeLanguage::Python,
                CodeLanguage::Rust,
            ] {
                for spacing in [
                    LineSpacing::Compact,
                    LineSpacing::Normal,
                    LineSpacing::Relaxed,
                ] {
                    for trailing in [false, true] {
                        for block_offset in [
                            0,
                            13,
                            19,
                            41,
                            43,
                            47,
                            13 * 23,
                            17 * 29,
                            19 * 41,
                            19 * 41 - 1,
                            43 * 47,
                            43 * 47 - 1,
                        ] {
                            let options = RenderOptions {
                                mode,
                                width: 28,
                                palette: &theme.palette,
                                code_language: language,
                                code_density: CodeDensity::DEFAULT,
                                plain_highlight: true,
                                line_spacing: spacing,
                                block_index_offset: block_offset,
                                include_trailing_spacing: trailing,
                                search_query: Some("harbour"),
                            };
                            assert_eq!(
                                count_rendered_lines(&blocks, &options),
                                render_blocks(&blocks, &options).len(),
                                "{mode:?} {language:?} {spacing:?} trailing={trailing} offset={block_offset}"
                            );
                        }
                    }
                }
            }
        }
    }
}

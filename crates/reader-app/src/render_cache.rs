//! Keeping the rendered chapter between frames.
//!
//! Drawing a frame needs the whole chapter's lines, not just the visible slice:
//! the scroll offset is clamped against the total and the scrollbar is sized from
//! it. Rendering the chapter again on every keypress therefore made scrolling
//! cost O(chapter) — fine on a short chapter, wasteful on a long one, and paid
//! once per repaint.
//!
//! The cache holds one chapter, keyed by everything that can change its lines. A
//! key mismatch is the only invalidation there is, which means correctness does
//! not depend on every caller remembering to clear it: change the width, the
//! settings, the palette, the search query, or the chapter, and the next read
//! renders again.
//!
//! One chapter is enough. Reading moves forward a chapter at a time and the
//! frames in between all want the same one; holding the whole book would trade a
//! lot of memory for the two repaints around a chapter boundary.

use reader_core::render::{RenderOptions, render_blocks};
use reader_core::{AppSettings, Palette, StyledLine};

/// What a cached render was produced for.
///
/// Everything here is an input to `render_blocks`, so two renders with equal
/// keys have equal lines.
#[derive(Debug, Clone, PartialEq)]
struct RenderKey {
    book_id: String,
    book_import_hash: String,
    chapter_index: usize,
    chapter_id: String,
    settings: AppSettings,
    palette: Palette,
    content_width: u16,
    /// Highlighting changes the spans, so the query is part of the key.
    search_query: Option<String>,
}

/// The rendered lines of one chapter.
#[derive(Debug, Clone, Default)]
pub struct ChapterRenderCache {
    key: Option<RenderKey>,
    lines: Vec<StyledLine>,
    hits: u64,
    misses: u64,
}

impl ChapterRenderCache {
    /// How many reads were served without rendering, and how many rendered.
    ///
    /// Only used by tests and the benchmark; the reader never looks.
    #[must_use]
    pub const fn stats(&self) -> (u64, u64) {
        (self.hits, self.misses)
    }

    /// Forget what is held, whatever the key says.
    ///
    /// Needed only where the same book id can come back with different content —
    /// a re-import of a file that changed on disk.
    pub fn clear(&mut self) {
        self.key = None;
        self.lines = Vec::new();
    }
}

/// One chapter to render, gathered from the pieces `ReaderState` owns.
///
/// A request rather than a long argument list, so the caller can build it from
/// disjoint fields of itself without borrowing all of itself at once.
pub(crate) struct RenderRequest<'a> {
    pub book_id: &'a str,
    pub book_import_hash: &'a str,
    pub chapter_index: usize,
    pub chapter_id: &'a str,
    pub blocks: &'a [reader_core::CanonicalBlock],
    pub settings: AppSettings,
    pub palette: &'a Palette,
    pub content_width: u16,
    pub search_query: Option<&'a str>,
}

impl ChapterRenderCache {
    /// The requested chapter's lines, rendering only when they are not held.
    pub(crate) fn lines(&mut self, request: &RenderRequest<'_>) -> &[StyledLine] {
        // Compared field by field rather than by building a key: a hit is the
        // common case and must not allocate the book id and query just to throw
        // them away.
        let fresh = self.key.as_ref().is_some_and(|key| {
            key.book_id == request.book_id
                && key.book_import_hash == request.book_import_hash
                && key.chapter_index == request.chapter_index
                && key.chapter_id == request.chapter_id
                && key.settings == request.settings
                && key.palette == *request.palette
                && key.content_width == request.content_width
                && key.search_query.as_deref() == request.search_query
        });
        if fresh {
            self.hits += 1;
            return &self.lines;
        }

        let options = RenderOptions {
            mode: request.settings.render_mode,
            width: request.content_width as usize,
            palette: request.palette,
            code_language: request.settings.code_language,
            code_density: request.settings.code_density,
            plain_highlight: request.settings.plain_highlight,
            line_spacing: request.settings.line_spacing,
            block_index_offset: 0,
            include_trailing_spacing: true,
            search_query: request.search_query,
        };
        self.lines = render_blocks(request.blocks, &options);
        self.key = Some(RenderKey {
            book_id: request.book_id.to_owned(),
            book_import_hash: request.book_import_hash.to_owned(),
            chapter_index: request.chapter_index,
            chapter_id: request.chapter_id.to_owned(),
            settings: request.settings,
            palette: *request.palette,
            content_width: request.content_width,
            search_query: request.search_query.map(str::to_owned),
        });
        self.misses += 1;
        &self.lines
    }
}

#[cfg(test)]
mod tests {
    use reader_core::{AppSettings, CanonicalBlock, RenderMode, Theme};

    use super::{ChapterRenderCache, RenderRequest};

    fn blocks() -> Vec<CanonicalBlock> {
        vec![
            CanonicalBlock::Heading {
                id: "h1".into(),
                text: "A quiet harbour".into(),
                level: Some(1),
            },
            CanonicalBlock::Paragraph {
                id: "p1".into(),
                text: "The lantern swung on its hook while the tide worked at the \
                       stones below, patient and unhurried."
                    .into(),
            },
            CanonicalBlock::Paragraph {
                id: "p2".into(),
                text: "Sandstone steps climbed away from the water.".into(),
            },
        ]
    }

    /// A request for chapter 0 of "book", with everything else as given.
    fn request<'a>(
        blocks: &'a [CanonicalBlock],
        settings: AppSettings,
        theme: &'a Theme,
        width: u16,
        query: Option<&'a str>,
    ) -> RenderRequest<'a> {
        RenderRequest {
            book_id: "book",
            book_import_hash: "hash",
            chapter_index: 0,
            chapter_id: "chapter-0",
            blocks,
            settings,
            palette: &theme.palette,
            content_width: width,
            search_query: query,
        }
    }

    /// Read the same chapter through the cache, varying one input at a time.
    fn read(
        cache: &mut ChapterRenderCache,
        blocks: &[CanonicalBlock],
        settings: AppSettings,
        theme: &Theme,
        width: u16,
        query: Option<&str>,
    ) -> usize {
        cache
            .lines(&request(blocks, settings, theme, width, query))
            .len()
    }

    #[test]
    fn the_second_read_of_a_chapter_does_not_render_it_again() {
        let mut cache = ChapterRenderCache::default();
        let blocks = blocks();
        let settings = AppSettings::default();
        let theme = Theme::default();

        let first = read(&mut cache, &blocks, settings, &theme, 40, None);
        assert_eq!(cache.stats(), (0, 1), "the first read has to render");

        for _ in 0..20 {
            let again = read(&mut cache, &blocks, settings, &theme, 40, None);
            assert_eq!(again, first, "the lines must not change");
        }
        assert_eq!(
            cache.stats(),
            (20, 1),
            "twenty repaints, one render — this is the whole point"
        );
    }

    #[test]
    fn every_input_that_changes_the_lines_also_misses_the_cache() {
        let blocks = blocks();
        let base = AppSettings::default();
        let theme = Theme::default();

        // Each case reads once to warm the cache, then reads with one input
        // changed, and must render a second time.
        let mut narrower = ChapterRenderCache::default();
        read(&mut narrower, &blocks, base, &theme, 40, None);
        read(&mut narrower, &blocks, base, &theme, 24, None);
        assert_eq!(narrower.stats(), (0, 2), "a different width");

        let mut disguised = ChapterRenderCache::default();
        read(&mut disguised, &blocks, base, &theme, 40, None);
        // The default disguise is code, so plain is the change here.
        let plain = AppSettings {
            render_mode: RenderMode::Plain,
            ..base
        };
        read(&mut disguised, &blocks, plain, &theme, 40, None);
        assert_eq!(disguised.stats(), (0, 2), "a different render mode");

        let mut searched = ChapterRenderCache::default();
        read(&mut searched, &blocks, base, &theme, 40, None);
        read(&mut searched, &blocks, base, &theme, 40, Some("lantern"));
        assert_eq!(searched.stats(), (0, 2), "highlighting changes the spans");

        let mut recoloured = ChapterRenderCache::default();
        read(&mut recoloured, &blocks, base, &theme, 40, None);
        let other = Theme::resolve(
            reader_core::ColorSchemeId::Amber,
            reader_core::AppearanceThemeId::Light,
        );
        read(&mut recoloured, &blocks, base, &other, 40, None);
        assert_eq!(recoloured.stats(), (0, 2), "a different palette");

        let mut turned = ChapterRenderCache::default();
        read(&mut turned, &blocks, base, &theme, 40, None);
        turned.lines(&RenderRequest {
            chapter_index: 1,
            ..request(&blocks, base, &theme, 40, None)
        });
        assert_eq!(turned.stats(), (0, 2), "a different chapter");

        let mut swapped = ChapterRenderCache::default();
        read(&mut swapped, &blocks, base, &theme, 40, None);
        swapped.lines(&RenderRequest {
            book_id: "other-book",
            ..request(&blocks, base, &theme, 40, None)
        });
        assert_eq!(swapped.stats(), (0, 2), "a different book");

        let mut refreshed = ChapterRenderCache::default();
        read(&mut refreshed, &blocks, base, &theme, 40, None);
        refreshed.lines(&RenderRequest {
            book_import_hash: "new-hash",
            ..request(&blocks, base, &theme, 40, None)
        });
        assert_eq!(refreshed.stats(), (0, 2), "re-imported content");

        let mut renamed_chapter = ChapterRenderCache::default();
        read(&mut renamed_chapter, &blocks, base, &theme, 40, None);
        renamed_chapter.lines(&RenderRequest {
            chapter_id: "renamed-chapter",
            ..request(&blocks, base, &theme, 40, None)
        });
        assert_eq!(
            renamed_chapter.stats(),
            (0, 2),
            "a different chapter identity"
        );
    }

    #[test]
    fn going_back_to_the_previous_inputs_renders_again() {
        // The cache holds one chapter, so alternating between two of them is the
        // worst case: every read misses. This is asserted so the behavior is a
        // decision rather than a surprise.
        let mut cache = ChapterRenderCache::default();
        let blocks = blocks();
        let settings = AppSettings::default();
        let theme = Theme::default();

        for width in [40, 24, 40, 24] {
            read(&mut cache, &blocks, settings, &theme, width, None);
        }
        assert_eq!(cache.stats(), (0, 4));
    }

    #[test]
    fn clearing_forces_the_next_read_to_render() {
        let mut cache = ChapterRenderCache::default();
        let blocks = blocks();
        let settings = AppSettings::default();
        let theme = Theme::default();

        read(&mut cache, &blocks, settings, &theme, 40, None);
        read(&mut cache, &blocks, settings, &theme, 40, None);
        cache.clear();
        read(&mut cache, &blocks, settings, &theme, 40, None);

        assert_eq!(cache.stats(), (1, 2));
    }
}

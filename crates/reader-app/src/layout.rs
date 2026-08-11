//! Turning a terminal size into reading geometry.
//!
//! The reading column is narrower than the terminal: the frame, an optional
//! scrollbar, the configured margin, and the font scale each take from it. Every
//! scroll bound and progress figure is derived from the same layout, so a resize
//! moves them together.

use reader_core::settings::AppSettings;

/// Widest an overlay column may get.
const OVERLAY_MAX_WIDTH: u16 = 46;
/// Narrowest the reading column may get before overlays stop taking space.
const MIN_MAIN_WIDTH: u16 = 24;
/// The reading column never narrows below this before the margin is reduced.
const MIN_TEXT_WIDTH: u16 = 20;

/// What the overlay currently occupying the screen does to the layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayLayout {
    /// No overlay: the reading column keeps the whole frame.
    None,
    /// A side column, which narrows the reading area.
    Side,
    /// A centred modal, which suspends the reading layout underneath it.
    Modal,
    /// The full-page manual, which uses the frame but not the reading margins.
    FullPage,
}

/// A terminal size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewport {
    pub width: u16,
    pub height: u16,
}

impl Viewport {
    #[must_use]
    pub const fn new(width: u16, height: u16) -> Self {
        Self { width, height }
    }
}

impl Default for Viewport {
    /// The size assumed before the terminal reports one.
    fn default() -> Self {
        Self::new(120, 40)
    }
}

/// Resolved geometry for one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewportLayout {
    /// Rows available for book text.
    pub body_height: u16,
    /// Columns taken by a side overlay, including its gap.
    pub overlay_width: u16,
    /// Columns left for the main area.
    pub main_width: u16,
    /// Columns of text the renderer should wrap to.
    pub content_width: u16,
    /// Columns of blank space left of the text, centring it.
    pub content_padding: u16,
    /// Whether a scrollbar column is reserved.
    pub scrollbar_width: u16,
}

/// Compute the layout for a frame.
///
/// `footer_height` is supplied by the caller because it depends on wrapped
/// status text, which only the TUI knows how to measure.
#[must_use]
pub fn compute_layout(
    viewport: Viewport,
    settings: &AppSettings,
    overlay: OverlayLayout,
    has_book: bool,
    footer_height: u16,
) -> ViewportLayout {
    let body_height = viewport
        .height
        .saturating_sub(footer_height)
        .saturating_sub(2)
        .max(1);

    // A side overlay takes just under a third of the frame, capped.
    let overlay_width = match overlay {
        OverlayLayout::Side => OVERLAY_MAX_WIDTH.min(viewport.width / 100 * 32),
        _ => 0,
    };
    let gap = if overlay_width > 0 { 3 } else { 0 };
    let main_width = viewport
        .width
        .saturating_sub(overlay_width)
        .saturating_sub(gap)
        .max(MIN_MAIN_WIDTH);

    let modal = overlay == OverlayLayout::Modal;
    let scrollbar_width = u16::from(has_book && !modal);
    let base_content_width = main_width
        .saturating_sub(2)
        .saturating_sub(scrollbar_width)
        .max(1);

    // The manual and modals do not use the reading margins.
    let reading_layout = has_book && overlay != OverlayLayout::FullPage && !modal;
    let requested_margin = if reading_layout {
        settings.margin_size.min(30)
    } else {
        0
    };
    let max_margin = base_content_width.saturating_sub(MIN_TEXT_WIDTH.min(base_content_width)) / 2;
    let applied_margin = requested_margin.min(max_margin);
    let width_inside_margins = base_content_width.saturating_sub(applied_margin * 2).max(1);

    let font_scale = if reading_layout {
        settings.font_scale.clamp(1.0, 2.0)
    } else {
        1.0
    };
    let content_width = ((f64::from(width_inside_margins) / font_scale).floor() as u16).max(1);
    let content_padding = if reading_layout {
        base_content_width.saturating_sub(content_width) / 2
    } else {
        0
    };

    ViewportLayout {
        body_height,
        overlay_width,
        main_width,
        content_width,
        content_padding,
        scrollbar_width,
    }
}

#[cfg(test)]
mod tests {
    use reader_core::settings::AppSettings;

    use super::{OverlayLayout, Viewport, compute_layout};

    fn settings(margin: u16, font_scale: f64) -> AppSettings {
        AppSettings {
            margin_size: margin,
            font_scale,
            ..AppSettings::default()
        }
    }

    #[test]
    fn the_body_leaves_room_for_the_footer_and_frame() {
        let layout = compute_layout(
            Viewport::new(100, 40),
            &settings(0, 1.0),
            OverlayLayout::None,
            true,
            3,
        );
        assert_eq!(layout.body_height, 35);
    }

    #[test]
    fn a_tiny_terminal_still_yields_a_usable_layout() {
        let layout = compute_layout(
            Viewport::new(10, 3),
            &settings(24, 1.5),
            OverlayLayout::Side,
            true,
            3,
        );
        assert_eq!(layout.body_height, 1);
        assert!(layout.content_width >= 1);
        assert!(layout.main_width >= 24);
    }

    #[test]
    fn margins_narrow_the_text_and_centre_it() {
        let without = compute_layout(
            Viewport::new(100, 40),
            &settings(0, 1.0),
            OverlayLayout::None,
            true,
            3,
        );
        let with = compute_layout(
            Viewport::new(100, 40),
            &settings(12, 1.0),
            OverlayLayout::None,
            true,
            3,
        );
        assert_eq!(with.content_width, without.content_width - 24);
        assert_eq!(with.content_padding, 12);
    }

    #[test]
    fn an_oversized_margin_is_capped_so_text_stays_readable() {
        let layout = compute_layout(
            Viewport::new(50, 40),
            &settings(30, 1.0),
            OverlayLayout::None,
            true,
            3,
        );
        assert!(layout.content_width >= 20, "{layout:?}");
    }

    #[test]
    fn font_scale_divides_the_reading_width() {
        let normal = compute_layout(
            Viewport::new(100, 40),
            &settings(0, 1.0),
            OverlayLayout::None,
            true,
            3,
        );
        let scaled = compute_layout(
            Viewport::new(100, 40),
            &settings(0, 1.5),
            OverlayLayout::None,
            true,
            3,
        );
        assert_eq!(scaled.content_width, normal.content_width / 3 * 2);
        assert!(scaled.content_padding > 0);
    }

    #[test]
    fn a_side_overlay_narrows_the_reading_column() {
        let none = compute_layout(
            Viewport::new(120, 40),
            &settings(0, 1.0),
            OverlayLayout::None,
            true,
            3,
        );
        let side = compute_layout(
            Viewport::new(120, 40),
            &settings(0, 1.0),
            OverlayLayout::Side,
            true,
            3,
        );
        assert!(side.overlay_width > 0);
        assert!(side.main_width < none.main_width);
    }

    #[test]
    fn modals_and_the_manual_suspend_the_reading_layout() {
        for overlay in [OverlayLayout::Modal, OverlayLayout::FullPage] {
            let layout =
                compute_layout(Viewport::new(100, 40), &settings(12, 1.5), overlay, true, 3);
            assert_eq!(layout.content_padding, 0, "{overlay:?}");
        }
        let modal = compute_layout(
            Viewport::new(100, 40),
            &settings(0, 1.0),
            OverlayLayout::Modal,
            true,
            3,
        );
        assert_eq!(modal.scrollbar_width, 0, "a modal hides the scrollbar");
    }

    #[test]
    fn without_a_book_there_is_no_scrollbar_or_reading_margin() {
        let layout = compute_layout(
            Viewport::new(100, 40),
            &settings(24, 1.5),
            OverlayLayout::None,
            false,
            3,
        );
        assert_eq!(layout.scrollbar_width, 0);
        assert_eq!(layout.content_padding, 0);
    }
}

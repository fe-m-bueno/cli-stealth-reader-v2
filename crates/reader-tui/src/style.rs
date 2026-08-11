//! Mapping the domain's styled text onto Ratatui.
//!
//! The renderers produce [`reader_core::StyledLine`]s with palette colors; this
//! is the only place that knows how those become terminal attributes.

use ratatui::style::{Color as TuiColor, Modifier, Style as TuiStyle};
use ratatui::text::{Line, Span};
use reader_core::style::{AnsiColor, Color, Style, StyledLine};

/// Convert a palette color to a Ratatui color.
///
/// Named colors stay named so the terminal's own theme applies; true-color
/// values are passed through as RGB.
#[must_use]
pub fn to_tui_color(color: Color) -> TuiColor {
    match color {
        Color::Rgb { r, g, b } => TuiColor::Rgb(r, g, b),
        Color::Ansi(ansi) => match ansi {
            AnsiColor::Black => TuiColor::Black,
            AnsiColor::Red => TuiColor::Red,
            AnsiColor::Green => TuiColor::Green,
            AnsiColor::Yellow => TuiColor::Yellow,
            AnsiColor::Blue => TuiColor::Blue,
            AnsiColor::Magenta => TuiColor::Magenta,
            AnsiColor::Cyan => TuiColor::Cyan,
            AnsiColor::White => TuiColor::Gray,
            AnsiColor::BrightBlack => TuiColor::DarkGray,
            AnsiColor::BrightRed => TuiColor::LightRed,
            AnsiColor::BrightGreen => TuiColor::LightGreen,
            AnsiColor::BrightYellow => TuiColor::LightYellow,
            AnsiColor::BrightBlue => TuiColor::LightBlue,
            AnsiColor::BrightMagenta => TuiColor::LightMagenta,
            AnsiColor::BrightCyan => TuiColor::LightCyan,
            AnsiColor::BrightWhite => TuiColor::White,
        },
    }
}

/// Convert a domain style to a Ratatui style.
#[must_use]
pub fn to_tui_style(style: Style) -> TuiStyle {
    let mut result = TuiStyle::default();
    if let Some(color) = style.foreground {
        result = result.fg(to_tui_color(color));
    }
    if let Some(color) = style.background {
        result = result.bg(to_tui_color(color));
    }
    if style.bold {
        result = result.add_modifier(Modifier::BOLD);
    }
    if style.inverse {
        result = result.add_modifier(Modifier::REVERSED);
    }
    result
}

/// Convert a rendered line to a Ratatui line.
#[must_use]
pub fn to_tui_line(line: &StyledLine) -> Line<'static> {
    Line::from(
        line.spans
            .iter()
            .map(|span| Span::styled(span.text.clone(), to_tui_style(span.style)))
            .collect::<Vec<_>>(),
    )
}

/// Convert several rendered lines.
#[must_use]
pub fn to_tui_lines(lines: &[StyledLine]) -> Vec<Line<'static>> {
    lines.iter().map(to_tui_line).collect()
}

#[cfg(test)]
mod tests {
    use ratatui::style::{Color as TuiColor, Modifier};
    use reader_core::style::{AnsiColor, Color, Span, Style, StyledLine};

    use super::{to_tui_color, to_tui_line, to_tui_style};

    #[test]
    fn true_color_passes_through_and_named_colors_stay_named() {
        assert_eq!(
            to_tui_color(Color::rgb(1, 2, 3)),
            TuiColor::Rgb(1, 2, 3),
            "RGB is preserved exactly"
        );
        assert_eq!(
            to_tui_color(Color::Ansi(AnsiColor::Cyan)),
            TuiColor::Cyan,
            "a named color stays named so the terminal theme applies"
        );
        assert_eq!(
            to_tui_color(Color::Ansi(AnsiColor::BrightBlack)),
            TuiColor::DarkGray
        );
    }

    #[test]
    fn styles_carry_colors_and_modifiers() {
        let style = to_tui_style(
            Style::fg(Color::rgb(10, 20, 30))
                .with_bg(Color::rgb(40, 50, 60))
                .bold()
                .inverse(),
        );
        assert_eq!(style.fg, Some(TuiColor::Rgb(10, 20, 30)));
        assert_eq!(style.bg, Some(TuiColor::Rgb(40, 50, 60)));
        assert!(style.add_modifier.contains(Modifier::BOLD));
        assert!(style.add_modifier.contains(Modifier::REVERSED));

        let plain = to_tui_style(Style::new());
        assert_eq!(plain.fg, None);
        assert_eq!(plain.bg, None);
        assert!(plain.add_modifier.is_empty());
    }

    #[test]
    fn a_line_keeps_its_spans_and_their_text() {
        let mut line = StyledLine::empty();
        line.push(Span::new("const ", Style::fg(Color::rgb(1, 1, 1))));
        line.push(Span::new("value", Style::fg(Color::rgb(2, 2, 2))));

        let converted = to_tui_line(&line);

        assert_eq!(converted.spans.len(), 2);
        assert_eq!(converted.spans[0].content, "const ");
        assert_eq!(converted.spans[1].content, "value");
    }
}

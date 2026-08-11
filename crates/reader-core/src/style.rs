//! Terminal colors and styled text.
//!
//! v1 emitted ANSI escape strings from the renderers and later had to parse them
//! back out (`highlightPreservingCSI`). v2 keeps rendering structural: renderers
//! produce [`StyledLine`]s and the TUI crate maps them onto terminal attributes.
//! The observable contract is the stripped text plus the styling of each span.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

/// The 16 terminal palette entries a theme may reference as `ansi:<name>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AnsiColor {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
}

impl AnsiColor {
    const ALL: [(&'static str, Self); 18] = [
        ("black", Self::Black),
        ("red", Self::Red),
        ("green", Self::Green),
        ("yellow", Self::Yellow),
        ("blue", Self::Blue),
        ("magenta", Self::Magenta),
        ("cyan", Self::Cyan),
        ("white", Self::White),
        ("brightblack", Self::BrightBlack),
        ("brightred", Self::BrightRed),
        ("brightgreen", Self::BrightGreen),
        ("brightyellow", Self::BrightYellow),
        ("brightblue", Self::BrightBlue),
        ("brightmagenta", Self::BrightMagenta),
        ("brightcyan", Self::BrightCyan),
        ("brightwhite", Self::BrightWhite),
        ("gray", Self::BrightBlack),
        ("grey", Self::BrightBlack),
    ];

    fn from_name(name: &str) -> Option<Self> {
        let normalized: String = name
            .chars()
            .filter(|char| !matches!(char, ' ' | '_' | '-'))
            .flat_map(char::to_lowercase)
            .collect();
        Self::ALL
            .iter()
            .find(|(candidate, _)| *candidate == normalized)
            .map(|(_, color)| *color)
    }

    /// The hex approximation v1 used when mixing ANSI-named colors.
    #[must_use]
    pub const fn fallback_rgb(self) -> (u8, u8, u8) {
        match self {
            Self::Black => (0x00, 0x00, 0x00),
            Self::Red => (0x80, 0x00, 0x00),
            Self::Green => (0x00, 0x80, 0x00),
            Self::Yellow => (0x80, 0x80, 0x00),
            Self::Blue => (0x00, 0x00, 0x80),
            Self::Magenta => (0x80, 0x00, 0x80),
            Self::Cyan => (0x00, 0x8b, 0x8b),
            Self::White => (0xc0, 0xc0, 0xc0),
            Self::BrightBlack => (0x80, 0x80, 0x80),
            Self::BrightRed => (0xff, 0x00, 0x00),
            Self::BrightGreen => (0x00, 0xff, 0x00),
            Self::BrightYellow => (0xff, 0xff, 0x00),
            Self::BrightBlue => (0x00, 0x00, 0xff),
            Self::BrightMagenta => (0xff, 0x00, 0xff),
            Self::BrightCyan => (0x00, 0xff, 0xff),
            Self::BrightWhite => (0xff, 0xff, 0xff),
        }
    }

    /// SGR foreground code; the background code is this plus ten.
    #[must_use]
    pub const fn foreground_code(self) -> u8 {
        match self {
            Self::Black => 30,
            Self::Red => 31,
            Self::Green => 32,
            Self::Yellow => 33,
            Self::Blue => 34,
            Self::Magenta => 35,
            Self::Cyan => 36,
            Self::White => 37,
            Self::BrightBlack => 90,
            Self::BrightRed => 91,
            Self::BrightGreen => 92,
            Self::BrightYellow => 93,
            Self::BrightBlue => 94,
            Self::BrightMagenta => 95,
            Self::BrightCyan => 96,
            Self::BrightWhite => 97,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Black => "black",
            Self::Red => "red",
            Self::Green => "green",
            Self::Yellow => "yellow",
            Self::Blue => "blue",
            Self::Magenta => "magenta",
            Self::Cyan => "cyan",
            Self::White => "white",
            Self::BrightBlack => "brightBlack",
            Self::BrightRed => "brightRed",
            Self::BrightGreen => "brightGreen",
            Self::BrightYellow => "brightYellow",
            Self::BrightBlue => "brightBlue",
            Self::BrightMagenta => "brightMagenta",
            Self::BrightCyan => "brightCyan",
            Self::BrightWhite => "brightWhite",
        }
    }
}

/// A theme color: either a palette entry or a true-color value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Color {
    Ansi(AnsiColor),
    Rgb { r: u8, g: u8, b: u8 },
}

/// Rejected color literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorParseError {
    pub input: String,
}

impl std::fmt::Display for ColorParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "unsupported color literal: {}", self.input)
    }
}

impl std::error::Error for ColorParseError {}

impl Color {
    /// Construct from the v1 literal forms: `ansi:<name>`, `#rgb`, or `#rrggbb`.
    pub fn parse(literal: &str) -> Result<Self, ColorParseError> {
        let error = || ColorParseError {
            input: literal.to_owned(),
        };
        if let Some(name) = literal.strip_prefix("ansi:") {
            return AnsiColor::from_name(name).map(Self::Ansi).ok_or_else(error);
        }
        let digits = literal.strip_prefix('#').unwrap_or(literal);
        let expanded: String = match digits.len() {
            3 => digits.chars().flat_map(|char| [char, char]).collect(),
            6 => digits.to_owned(),
            _ => return Err(error()),
        };
        let value = u32::from_str_radix(&expanded, 16).map_err(|_| error())?;
        Ok(Self::from_u32(value))
    }

    #[must_use]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::Rgb { r, g, b }
    }

    const fn from_u32(value: u32) -> Self {
        Self::Rgb {
            r: ((value >> 16) & 0xff) as u8,
            g: ((value >> 8) & 0xff) as u8,
            b: (value & 0xff) as u8,
        }
    }

    /// RGB channels, using v1's palette approximation for ANSI colors.
    #[must_use]
    pub const fn channels(self) -> (u8, u8, u8) {
        match self {
            Self::Ansi(ansi) => ansi.fallback_rgb(),
            Self::Rgb { r, g, b } => (r, g, b),
        }
    }

    /// The literal form, so settings round-trip byte for byte with v1.
    #[must_use]
    pub fn to_literal(self) -> String {
        match self {
            Self::Ansi(ansi) => format!("ansi:{}", ansi.name()),
            Self::Rgb { r, g, b } => format!("#{r:02x}{g:02x}{b:02x}"),
        }
    }

    /// Linear interpolation toward `other`, matching v1's `mix`.
    #[must_use]
    pub fn mix(self, other: Self, amount: f64) -> Self {
        let (ar, ag, ab) = self.channels();
        let (br, bg, bb) = other.channels();
        let blend = |start: u8, end: u8| {
            let value = f64::from(start) + (f64::from(end) - f64::from(start)) * amount;
            // JS `Math.round` rounds half away from zero for positives, which is
            // what `f64::round` does for the non-negative values used here.
            value.round().clamp(0.0, 255.0) as u8
        };
        Self::Rgb {
            r: blend(ar, br),
            g: blend(ag, bg),
            b: blend(ab, bb),
        }
    }

    /// WCAG relative luminance.
    #[must_use]
    pub fn luminance(self) -> f64 {
        let (r, g, b) = self.channels();
        let channel = |value: u8| {
            let normalized = f64::from(value) / 255.0;
            if normalized <= 0.03928 {
                normalized / 12.92
            } else {
                ((normalized + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
    }

    /// WCAG contrast ratio between two colors.
    #[must_use]
    pub fn contrast(self, other: Self) -> f64 {
        let first = self.luminance();
        let second = other.luminance();
        (first.max(second) + 0.05) / (first.min(second) + 0.05)
    }
}

/// Text attributes a renderer may request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Style {
    pub foreground: Option<Color>,
    pub background: Option<Color>,
    pub bold: bool,
    pub inverse: bool,
}

impl Style {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            foreground: None,
            background: None,
            bold: false,
            inverse: false,
        }
    }

    #[must_use]
    pub const fn fg(color: Color) -> Self {
        Self {
            foreground: Some(color),
            ..Self::new()
        }
    }

    #[must_use]
    pub const fn with_fg(mut self, color: Color) -> Self {
        self.foreground = Some(color);
        self
    }

    #[must_use]
    pub const fn with_bg(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    #[must_use]
    pub const fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    #[must_use]
    pub const fn inverse(mut self) -> Self {
        self.inverse = true;
        self
    }

    /// The SGR sequence that opens this style, used by the ANSI writer.
    #[must_use]
    pub fn sgr(&self) -> String {
        let mut codes: Vec<String> = Vec::new();
        if self.bold {
            codes.push("1".into());
        }
        if self.inverse {
            codes.push("7".into());
        }
        if let Some(color) = self.foreground {
            match color {
                Color::Ansi(ansi) => codes.push(ansi.foreground_code().to_string()),
                Color::Rgb { r, g, b } => codes.push(format!("38;2;{r};{g};{b}")),
            }
        }
        if let Some(color) = self.background {
            match color {
                Color::Ansi(ansi) => codes.push((ansi.foreground_code() + 10).to_string()),
                Color::Rgb { r, g, b } => codes.push(format!("48;2;{r};{g};{b}")),
            }
        }
        if codes.is_empty() {
            String::new()
        } else {
            format!("\x1b[{}m", codes.join(";"))
        }
    }
}

/// A run of text sharing one style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub style: Style,
}

impl Span {
    pub fn new(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }

    pub fn raw(text: impl Into<String>) -> Self {
        Self::new(text, Style::new())
    }

    /// Character count as rendered, matching the width accounting of the layout.
    #[must_use]
    pub fn width(&self) -> usize {
        self.text.chars().count()
    }
}

/// One rendered terminal line.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StyledLine {
    pub spans: Vec<Span>,
}

impl StyledLine {
    #[must_use]
    pub const fn empty() -> Self {
        Self { spans: Vec::new() }
    }

    #[must_use]
    pub fn from_spans(spans: Vec<Span>) -> Self {
        Self { spans }
    }

    pub fn single(text: impl Into<String>, style: Style) -> Self {
        Self {
            spans: vec![Span::new(text, style)],
        }
    }

    pub fn push(&mut self, span: Span) {
        if span.text.is_empty() {
            return;
        }
        if let Some(last) = self.spans.last_mut()
            && last.style == span.style
        {
            last.text.push_str(&span.text);
            return;
        }
        self.spans.push(span);
    }

    /// The line without styling — the primary assertion surface in tests.
    #[must_use]
    pub fn text(&self) -> String {
        self.spans.iter().map(|span| span.text.as_str()).collect()
    }

    #[must_use]
    pub fn width(&self) -> usize {
        self.spans.iter().map(Span::width).sum()
    }

    #[must_use]
    pub fn is_blank(&self) -> bool {
        self.spans.iter().all(|span| span.text.is_empty())
    }

    /// Escape-sequence form, for golden files and non-TUI output paths.
    #[must_use]
    pub fn to_ansi(&self) -> String {
        let mut out = String::new();
        for span in &self.spans {
            let open = span.style.sgr();
            if open.is_empty() {
                out.push_str(&span.text);
            } else {
                let _ = write!(out, "{open}{}\x1b[0m", span.text);
            }
        }
        out
    }

    /// Split styling so that every case-insensitive match of `needle` gets
    /// `style`. Replaces v1's CSI-aware string surgery.
    #[must_use]
    pub fn highlight(&self, needle: &str, style: Style) -> Self {
        if needle.is_empty() {
            return self.clone();
        }
        let lower_needle: Vec<char> = needle.to_lowercase().chars().collect();
        let mut result = Self::empty();
        for span in &self.spans {
            let chars: Vec<char> = span.text.chars().collect();
            let lowered: Vec<char> = span
                .text
                .chars()
                .flat_map(|char| char.to_lowercase().collect::<Vec<_>>())
                .collect();
            // Case folding can change length (e.g. `İ`); fall back to the raw
            // span rather than mismatching indices.
            if lowered.len() != chars.len() {
                result.push(span.clone());
                continue;
            }
            let mut cursor = 0;
            let mut plain_start = 0;
            while cursor + lower_needle.len() <= chars.len() {
                if lowered[cursor..cursor + lower_needle.len()] == lower_needle[..] {
                    if plain_start < cursor {
                        result.push(Span::new(
                            chars[plain_start..cursor].iter().collect::<String>(),
                            span.style,
                        ));
                    }
                    result.push(Span::new(
                        chars[cursor..cursor + lower_needle.len()]
                            .iter()
                            .collect::<String>(),
                        style,
                    ));
                    cursor += lower_needle.len();
                    plain_start = cursor;
                } else {
                    cursor += 1;
                }
            }
            if plain_start < chars.len() {
                result.push(Span::new(
                    chars[plain_start..].iter().collect::<String>(),
                    span.style,
                ));
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::{AnsiColor, Color, Span, Style, StyledLine};

    #[test]
    fn parses_the_v1_color_literals() {
        assert_eq!(
            Color::parse("ansi:brightBlack"),
            Ok(Color::Ansi(AnsiColor::BrightBlack))
        );
        assert_eq!(Color::parse("#0d0d0d"), Ok(Color::rgb(13, 13, 13)));
        assert_eq!(Color::parse("#fff"), Ok(Color::rgb(255, 255, 255)));
        assert!(Color::parse("ansi:chartreuse").is_err());
        assert!(Color::parse("#12345").is_err());
    }

    #[test]
    fn round_trips_literals() {
        for literal in ["ansi:cyan", "#00cc66", "#ffffff"] {
            let color = Color::parse(literal).expect("literal should parse");
            assert_eq!(color.to_literal(), literal);
        }
    }

    #[test]
    fn mixes_and_measures_contrast_like_v1() {
        let mixed = Color::rgb(0, 0, 0).mix(Color::rgb(255, 255, 255), 0.5);
        assert_eq!(mixed, Color::rgb(128, 128, 128));
        let ratio = Color::rgb(0, 0, 0).contrast(Color::rgb(255, 255, 255));
        assert!((ratio - 21.0).abs() < 1e-9);
    }

    #[test]
    fn merges_adjacent_spans_that_share_a_style() {
        let mut line = StyledLine::empty();
        line.push(Span::raw("one"));
        line.push(Span::raw(" two"));
        assert_eq!(line.spans.len(), 1);
        assert_eq!(line.text(), "one two");
    }

    #[test]
    fn highlight_splits_matches_without_touching_other_text() {
        let accent = Style::fg(Color::rgb(1, 2, 3));
        let marker = Style::fg(Color::rgb(9, 9, 9)).bold();
        let line = StyledLine::single("The Quiet quiet harbour", accent);

        let highlighted = line.highlight("quiet", marker);

        assert_eq!(highlighted.text(), "The Quiet quiet harbour");
        let marked: Vec<&str> = highlighted
            .spans
            .iter()
            .filter(|span| span.style == marker)
            .map(|span| span.text.as_str())
            .collect();
        assert_eq!(marked, vec!["Quiet", "quiet"]);
    }

    #[test]
    fn ansi_output_wraps_each_styled_span() {
        let line = StyledLine::single("hi", Style::fg(Color::Ansi(AnsiColor::Cyan)).bold());
        assert_eq!(line.to_ansi(), "\x1b[1;36mhi\x1b[0m");
    }
}

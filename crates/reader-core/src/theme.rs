//! Color schemes and appearance variants.
//!
//! A theme is a color scheme (`codex`, `claude`, …) resolved through an
//! appearance variant (dark, light chalk, colorblind, ANSI). The derivation is
//! pure so every variant is a value test.

use crate::style::{AnsiColor, Color};

const fn hex(value: u32) -> Color {
    Color::Rgb {
        r: ((value >> 16) & 0xff) as u8,
        g: ((value >> 8) & 0xff) as u8,
        b: (value & 0xff) as u8,
    }
}

const fn ansi(color: AnsiColor) -> Color {
    Color::Ansi(color)
}

const CHALK_BACKGROUND: Color = hex(0xf7f2e4);

/// Identifier of a built-in color scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorSchemeId {
    Codex,
    Claude,
    Graphite,
    Amber,
    Forest,
}

impl ColorSchemeId {
    pub const ALL: [Self; 5] = [
        Self::Codex,
        Self::Claude,
        Self::Graphite,
        Self::Amber,
        Self::Forest,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Graphite => "graphite",
            Self::Amber => "amber",
            Self::Forest => "forest",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::Claude => "Claude Code",
            Self::Graphite => "Graphite",
            Self::Amber => "Amber",
            Self::Forest => "Forest",
        }
    }

    #[must_use]
    pub fn from_id(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == value)
    }

    /// The dark-variant palette this scheme is authored in.
    #[must_use]
    pub const fn palette(self) -> Palette {
        match self {
            Self::Codex => Palette {
                accent: ansi(AnsiColor::Cyan),
                accent_muted: ansi(AnsiColor::BrightBlack),
                foreground: hex(0xffffff),
                dim: hex(0xaaaaaa),
                background: hex(0x0d0d0d),
                border: hex(0x5d5d5d),
                warning: hex(0xffcc00),
                keyword: ansi(AnsiColor::Cyan),
                code_string: hex(0x00cc66),
                subtle: hex(0x616567),
            },
            Self::Claude => Palette {
                accent: hex(0xd77757),
                accent_muted: hex(0xeb9f7f),
                foreground: hex(0xffffff),
                dim: hex(0x999999),
                background: hex(0x0d0d0d),
                border: hex(0x888888),
                warning: hex(0xffc107),
                keyword: hex(0xb1b9f9),
                code_string: hex(0x4eba65),
                subtle: hex(0x505050),
            },
            Self::Graphite => Palette {
                accent: hex(0xc6d0da),
                accent_muted: hex(0x606a75),
                foreground: hex(0xeceff3),
                dim: hex(0x8f98a1),
                background: hex(0x121417),
                border: hex(0x2f343a),
                warning: hex(0xf1b36a),
                keyword: hex(0x9a9a9a),
                code_string: hex(0x7a9a7a),
                subtle: hex(0x4a4f55),
            },
            Self::Amber => Palette {
                accent: hex(0xffb347),
                accent_muted: hex(0x8a5a18),
                foreground: hex(0xfbe8c6),
                dim: hex(0xb3935a),
                background: hex(0x110d07),
                border: hex(0x3e2910),
                warning: hex(0xffd166),
                keyword: hex(0xd4a853),
                code_string: hex(0xc8864a),
                subtle: hex(0x5a3e1e),
            },
            Self::Forest => Palette {
                accent: hex(0x7ce2a1),
                accent_muted: hex(0x2d7047),
                foreground: hex(0xdff6e6),
                dim: hex(0x86a88e),
                background: hex(0x0b120e),
                border: hex(0x1a3d24),
                warning: hex(0xf2c97d),
                keyword: hex(0x5f9e6e),
                code_string: hex(0x7ec88a),
                subtle: hex(0x2e4d36),
            },
        }
    }
}

/// Identifier of an appearance variant applied on top of a color scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppearanceThemeId {
    Dark,
    Light,
    DarkColorblind,
    LightColorblind,
    DarkAnsi,
    LightAnsi,
}

impl AppearanceThemeId {
    pub const ALL: [Self; 6] = [
        Self::Dark,
        Self::Light,
        Self::DarkColorblind,
        Self::LightColorblind,
        Self::DarkAnsi,
        Self::LightAnsi,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
            Self::DarkColorblind => "dark-colorblind",
            Self::LightColorblind => "light-colorblind",
            Self::DarkAnsi => "dark-ansi",
            Self::LightAnsi => "light-ansi",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Dark => "Dark",
            Self::Light => "Light Chalk",
            Self::DarkColorblind => "Dark Colorblind",
            Self::LightColorblind => "Light Colorblind",
            Self::DarkAnsi => "Dark ANSI",
            Self::LightAnsi => "Light ANSI",
        }
    }

    #[must_use]
    pub fn from_id(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == value)
    }

    #[must_use]
    const fn is_light(self) -> bool {
        matches!(self, Self::Light | Self::LightColorblind | Self::LightAnsi)
    }
}

/// The ten colors every rendered surface draws from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Palette {
    pub accent: Color,
    pub accent_muted: Color,
    pub foreground: Color,
    pub dim: Color,
    pub background: Color,
    pub border: Color,
    pub warning: Color,
    pub keyword: Color,
    pub code_string: Color,
    pub subtle: Color,
}

/// A resolved theme: a color scheme seen through an appearance variant.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Theme {
    pub scheme: ColorSchemeId,
    pub appearance: AppearanceThemeId,
    pub palette: Palette,
}

impl Theme {
    /// v1's `applyAppearanceTheme`.
    #[must_use]
    pub fn resolve(scheme: ColorSchemeId, appearance: AppearanceThemeId) -> Self {
        let palette = match appearance {
            AppearanceThemeId::Dark => scheme.palette(),
            AppearanceThemeId::Light => light_palette(scheme),
            AppearanceThemeId::DarkColorblind | AppearanceThemeId::LightColorblind => {
                colorblind_palette(scheme, appearance.is_light())
            }
            AppearanceThemeId::DarkAnsi | AppearanceThemeId::LightAnsi => {
                ansi_palette(scheme, appearance.is_light())
            }
        };
        Self {
            scheme,
            appearance,
            palette,
        }
    }

    /// v1's composed theme id, `<scheme>` for dark and `<scheme>:<variant>` otherwise.
    #[must_use]
    pub fn id(&self) -> String {
        if self.appearance == AppearanceThemeId::Dark {
            self.scheme.as_str().to_owned()
        } else {
            format!("{}:{}", self.scheme.as_str(), self.appearance.as_str())
        }
    }

    /// v1's `labelFor`.
    #[must_use]
    pub fn label(&self) -> String {
        if self.appearance == AppearanceThemeId::Dark {
            self.scheme.label().to_owned()
        } else {
            format!("{} · {}", self.scheme.label(), self.appearance.label())
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::resolve(ColorSchemeId::Codex, AppearanceThemeId::Dark)
    }
}

/// Darken until the color is readable on the chalk background.
fn readable_on_light(color: Color) -> Color {
    let mut current = color;
    let mut step = 0;
    while step < 12 && current.contrast(CHALK_BACKGROUND) < 4.5 {
        current = current.mix(hex(0x000000), 0.12);
        step += 1;
    }
    current
}

fn light_palette(scheme: ColorSchemeId) -> Palette {
    let base = scheme.palette();
    let accent = readable_on_light(base.accent);
    Palette {
        accent,
        accent_muted: accent.mix(CHALK_BACKGROUND, 0.42),
        foreground: hex(0x25231d),
        dim: hex(0x706a5d),
        background: CHALK_BACKGROUND,
        border: hex(0xd8cfb8),
        warning: readable_on_light(base.warning),
        keyword: readable_on_light(base.keyword),
        code_string: readable_on_light(base.code_string),
        subtle: hex(0x837a68),
    }
}

/// The five scheme-dependent hues of a colorblind-safe variant.
struct VariantHues {
    accent: Color,
    accent_muted: Color,
    warning: Color,
    keyword: Color,
    code_string: Color,
}

fn colorblind_hues(scheme: ColorSchemeId, light: bool) -> VariantHues {
    match (scheme, light) {
        (ColorSchemeId::Claude, true) => VariantHues {
            accent: hex(0xb65c00),
            accent_muted: hex(0xc99b69),
            warning: hex(0x8a5a00),
            keyword: hex(0x0072b2),
            code_string: hex(0x007f5f),
        },
        (ColorSchemeId::Claude, false) => VariantHues {
            accent: hex(0xe69f00),
            accent_muted: hex(0x7a560f),
            warning: hex(0xf0e442),
            keyword: hex(0x56b4e9),
            code_string: hex(0x009e73),
        },
        (ColorSchemeId::Graphite, true) => VariantHues {
            accent: hex(0x4d5358),
            accent_muted: hex(0x9aa0a5),
            warning: hex(0xd55e00),
            keyword: hex(0x0072b2),
            code_string: hex(0x007f5f),
        },
        (ColorSchemeId::Graphite, false) => VariantHues {
            accent: hex(0xd8dee4),
            accent_muted: hex(0x6f7780),
            warning: hex(0xe69f00),
            keyword: hex(0x56b4e9),
            code_string: hex(0xf0e442),
        },
        (ColorSchemeId::Amber, true) => VariantHues {
            accent: hex(0xb65c00),
            accent_muted: hex(0xc99b69),
            warning: hex(0x8a5a00),
            keyword: hex(0x0072b2),
            code_string: hex(0x984ea3),
        },
        (ColorSchemeId::Amber, false) => VariantHues {
            accent: hex(0xe69f00),
            accent_muted: hex(0x7a560f),
            warning: hex(0xf0e442),
            keyword: hex(0x56b4e9),
            code_string: hex(0xcc79a7),
        },
        (ColorSchemeId::Forest, true) => VariantHues {
            accent: hex(0x007f5f),
            accent_muted: hex(0x78aa9b),
            warning: hex(0xd55e00),
            keyword: hex(0x0072b2),
            code_string: hex(0x984ea3),
        },
        (ColorSchemeId::Forest, false) => VariantHues {
            accent: hex(0x34c9a2),
            accent_muted: hex(0x236b59),
            warning: hex(0xf0e442),
            keyword: hex(0x56b4e9),
            code_string: hex(0xe69f00),
        },
        (ColorSchemeId::Codex, true) => VariantHues {
            accent: hex(0x0072b2),
            accent_muted: hex(0x7aa6c4),
            warning: hex(0xd55e00),
            keyword: hex(0xcc79a7),
            code_string: hex(0x007f5f),
        },
        (ColorSchemeId::Codex, false) => VariantHues {
            accent: hex(0x56b4e9),
            accent_muted: hex(0x2f657d),
            warning: hex(0xe69f00),
            keyword: hex(0xcc79a7),
            code_string: hex(0xf0e442),
        },
    }
}

fn colorblind_palette(scheme: ColorSchemeId, light: bool) -> Palette {
    let hues = colorblind_hues(scheme, light);
    Palette {
        accent: hues.accent,
        accent_muted: hues.accent_muted,
        foreground: if light { hex(0x202124) } else { hex(0xedf2f4) },
        dim: if light { hex(0x62676b) } else { hex(0x9aa4aa) },
        background: if light { hex(0xf8f7f1) } else { hex(0x0b0f12) },
        border: if light { hex(0xd1cec4) } else { hex(0x313a42) },
        warning: hues.warning,
        keyword: hues.keyword,
        code_string: hues.code_string,
        subtle: if light { hex(0x777b80) } else { hex(0x68747c) },
    }
}

fn ansi_hues(scheme: ColorSchemeId, light: bool) -> VariantHues {
    use AnsiColor::{
        Black, Blue, BrightBlue, BrightCyan, BrightGreen, BrightMagenta, BrightRed, BrightWhite,
        BrightYellow, Cyan, Green, Magenta, Red, White, Yellow,
    };
    match (scheme, light) {
        (ColorSchemeId::Claude, true) => VariantHues {
            accent: ansi(BrightRed),
            accent_muted: ansi(BrightYellow),
            warning: ansi(Yellow),
            keyword: ansi(Blue),
            code_string: ansi(Green),
        },
        (ColorSchemeId::Claude, false) => VariantHues {
            accent: ansi(BrightRed),
            accent_muted: ansi(BrightYellow),
            warning: ansi(BrightYellow),
            keyword: ansi(BrightBlue),
            code_string: ansi(BrightGreen),
        },
        (ColorSchemeId::Graphite, true) => VariantHues {
            accent: ansi(Black),
            accent_muted: ansi(AnsiColor::BrightBlack),
            warning: ansi(Red),
            keyword: ansi(Blue),
            code_string: ansi(Green),
        },
        (ColorSchemeId::Graphite, false) => VariantHues {
            accent: ansi(BrightWhite),
            accent_muted: ansi(AnsiColor::BrightBlack),
            warning: ansi(Yellow),
            keyword: ansi(White),
            code_string: ansi(Cyan),
        },
        (ColorSchemeId::Amber, true) => VariantHues {
            accent: ansi(Red),
            accent_muted: ansi(Yellow),
            warning: ansi(Magenta),
            keyword: ansi(Blue),
            code_string: ansi(Magenta),
        },
        (ColorSchemeId::Amber, false) => VariantHues {
            accent: ansi(Yellow),
            accent_muted: ansi(Red),
            warning: ansi(BrightYellow),
            keyword: ansi(BrightBlue),
            code_string: ansi(BrightMagenta),
        },
        (ColorSchemeId::Forest, true) => VariantHues {
            accent: ansi(Green),
            accent_muted: ansi(Cyan),
            warning: ansi(Red),
            keyword: ansi(Blue),
            code_string: ansi(Magenta),
        },
        (ColorSchemeId::Forest, false) => VariantHues {
            accent: ansi(BrightGreen),
            accent_muted: ansi(Green),
            warning: ansi(Yellow),
            keyword: ansi(BrightCyan),
            code_string: ansi(Green),
        },
        (ColorSchemeId::Codex, true) => VariantHues {
            accent: ansi(Cyan),
            accent_muted: ansi(AnsiColor::BrightBlack),
            warning: ansi(Red),
            keyword: ansi(Cyan),
            code_string: ansi(Green),
        },
        (ColorSchemeId::Codex, false) => VariantHues {
            accent: ansi(Cyan),
            accent_muted: ansi(AnsiColor::BrightBlack),
            warning: ansi(Yellow),
            keyword: ansi(Cyan),
            code_string: ansi(BrightGreen),
        },
    }
}

fn ansi_palette(scheme: ColorSchemeId, light: bool) -> Palette {
    let hues = ansi_hues(scheme, light);
    let border = if light {
        ansi(AnsiColor::BrightBlack)
    } else {
        match scheme {
            ColorSchemeId::Claude => ansi(AnsiColor::White),
            ColorSchemeId::Codex => ansi(AnsiColor::BrightBlack),
            _ => ansi(AnsiColor::Blue),
        }
    };
    Palette {
        accent: hues.accent,
        accent_muted: hues.accent_muted,
        foreground: if light {
            ansi(AnsiColor::Black)
        } else {
            ansi(AnsiColor::BrightWhite)
        },
        dim: ansi(AnsiColor::BrightBlack),
        background: if light {
            ansi(AnsiColor::BrightWhite)
        } else {
            ansi(AnsiColor::Black)
        },
        border,
        warning: hues.warning,
        keyword: hues.keyword,
        code_string: hues.code_string,
        subtle: ansi(AnsiColor::BrightBlack),
    }
}

#[cfg(test)]
mod tests {
    use super::{AppearanceThemeId, CHALK_BACKGROUND, ColorSchemeId, Theme, hex};

    #[test]
    fn dark_variant_keeps_the_authored_palette_and_plain_label() {
        let theme = Theme::resolve(ColorSchemeId::Claude, AppearanceThemeId::Dark);
        assert_eq!(theme.id(), "claude");
        assert_eq!(theme.label(), "Claude Code");
        assert_eq!(theme.palette, ColorSchemeId::Claude.palette());
    }

    #[test]
    fn variant_ids_and_labels_compose_like_v1() {
        let theme = Theme::resolve(ColorSchemeId::Forest, AppearanceThemeId::LightAnsi);
        assert_eq!(theme.id(), "forest:light-ansi");
        assert_eq!(theme.label(), "Forest · Light ANSI");
    }

    #[test]
    fn light_variant_darkens_accents_until_readable_on_chalk() {
        for scheme in ColorSchemeId::ALL {
            let theme = Theme::resolve(scheme, AppearanceThemeId::Light);
            assert_eq!(theme.palette.background, CHALK_BACKGROUND);
            assert_eq!(theme.palette.foreground, hex(0x25231d));
            for color in [
                theme.palette.accent,
                theme.palette.warning,
                theme.palette.keyword,
                theme.palette.code_string,
            ] {
                assert!(
                    color.contrast(CHALK_BACKGROUND) >= 4.5,
                    "{scheme:?} produced an unreadable {color:?}"
                );
            }
        }
    }

    #[test]
    fn every_scheme_resolves_in_every_appearance() {
        for scheme in ColorSchemeId::ALL {
            for appearance in AppearanceThemeId::ALL {
                let theme = Theme::resolve(scheme, appearance);
                assert_eq!(theme.scheme, scheme);
                assert_eq!(theme.appearance, appearance);
            }
        }
    }

    #[test]
    fn identifiers_round_trip_through_strings() {
        for scheme in ColorSchemeId::ALL {
            assert_eq!(ColorSchemeId::from_id(scheme.as_str()), Some(scheme));
        }
        for appearance in AppearanceThemeId::ALL {
            assert_eq!(
                AppearanceThemeId::from_id(appearance.as_str()),
                Some(appearance)
            );
        }
        assert_eq!(ColorSchemeId::from_id("nope"), None);
    }

    #[test]
    fn ansi_variants_keep_the_scheme_specific_border() {
        let claude = Theme::resolve(ColorSchemeId::Claude, AppearanceThemeId::DarkAnsi);
        let forest = Theme::resolve(ColorSchemeId::Forest, AppearanceThemeId::DarkAnsi);
        assert_eq!(claude.palette.border.to_literal(), "ansi:white");
        assert_eq!(forest.palette.border.to_literal(), "ansi:blue");
    }
}

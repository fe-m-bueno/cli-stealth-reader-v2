//! Reader preferences and their v1 persistence format.
//!
//! v1 stored every preference as a string in the SQLite `settings` table and
//! silently kept the default when a stored value was invalid. That tolerance is
//! part of the contract: a database written by an older or newer build must not
//! break startup.

use crate::theme::{AppearanceThemeId, ColorSchemeId};

/// Plain text or code-disguised reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenderMode {
    Code,
    Plain,
}

impl RenderMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Code => "code",
            Self::Plain => "plain",
        }
    }

    #[must_use]
    pub fn from_id(value: &str) -> Option<Self> {
        match value {
            "code" => Some(Self::Code),
            "plain" => Some(Self::Plain),
            _ => None,
        }
    }
}

/// Language the code disguise imitates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CodeLanguage {
    TypeScript,
    Python,
    Rust,
}

impl CodeLanguage {
    pub const ALL: [Self; 3] = [Self::TypeScript, Self::Python, Self::Rust];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TypeScript => "typescript",
            Self::Python => "python",
            Self::Rust => "rust",
        }
    }

    #[must_use]
    pub fn from_id(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == value)
    }
}

/// How much synthetic code structure the disguise adds, 1 (sparse) to 5 (dense).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CodeDensity(u8);

impl CodeDensity {
    pub const MIN: Self = Self(1);
    pub const MAX: Self = Self(5);
    pub const DEFAULT: Self = Self(3);

    /// Reject anything outside 1..=5, as v1's allow-list did.
    #[must_use]
    pub const fn new(value: u8) -> Option<Self> {
        if matches!(value, 1..=5) {
            Some(Self(value))
        } else {
            None
        }
    }

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl Default for CodeDensity {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Blank-line policy between rendered blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LineSpacing {
    Compact,
    Normal,
    Relaxed,
}

impl LineSpacing {
    pub const ALL: [Self; 3] = [Self::Compact, Self::Normal, Self::Relaxed];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Normal => "normal",
            Self::Relaxed => "relaxed",
        }
    }

    #[must_use]
    pub fn from_id(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == value)
    }
}

/// What the footer reports about progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProgressVisibility {
    TimeChapter,
    TimeBook,
    Book,
    Both,
    Chapter,
    Hidden,
}

impl ProgressVisibility {
    /// Cycle order used by the `p` shortcut.
    pub const ALL: [Self; 6] = [
        Self::TimeChapter,
        Self::TimeBook,
        Self::Book,
        Self::Both,
        Self::Chapter,
        Self::Hidden,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TimeChapter => "time-chapter",
            Self::TimeBook => "time-book",
            Self::Book => "book",
            Self::Both => "both",
            Self::Chapter => "chapter",
            Self::Hidden => "hidden",
        }
    }

    #[must_use]
    pub fn from_id(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == value)
    }

    /// Next entry, wrapping at the end.
    #[must_use]
    pub fn next(self) -> Self {
        let position = Self::ALL
            .iter()
            .position(|candidate| *candidate == self)
            .unwrap_or(0);
        Self::ALL[(position + 1) % Self::ALL.len()]
    }
}

/// Tabs of the reader settings overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingsTab {
    Themes,
    Reading,
    Layout,
    More,
}

impl SettingsTab {
    pub const ALL: [Self; 4] = [Self::Themes, Self::Reading, Self::Layout, Self::More];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Themes => "themes",
            Self::Reading => "reading",
            Self::Layout => "layout",
            Self::More => "more",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Themes => "Themes",
            Self::Reading => "Reading",
            Self::Layout => "Layout",
            Self::More => "More",
        }
    }

    #[must_use]
    pub fn from_id(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == value)
    }
}

/// Allowed font scales, in cycle order.
pub const FONT_SCALES: [f64; 4] = [1.0, 1.15, 1.3, 1.5];
/// Allowed margin widths in columns, in cycle order.
pub const MARGIN_SIZES: [u16; 6] = [0, 4, 8, 12, 16, 24];

/// Whether `value` is one of the canonical font scales.
#[must_use]
pub fn is_font_scale(value: f64) -> bool {
    FONT_SCALES
        .iter()
        .any(|candidate| (candidate - value).abs() < f64::EPSILON)
}

/// Whether `value` is one of the canonical margin widths.
#[must_use]
pub fn is_margin_size(value: u16) -> bool {
    MARGIN_SIZES.contains(&value)
}

/// The persisted reader preferences.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AppSettings {
    pub theme_id: ColorSchemeId,
    pub appearance_theme_id: AppearanceThemeId,
    pub progress_visibility: ProgressVisibility,
    pub render_mode: RenderMode,
    pub code_language: CodeLanguage,
    pub code_density: CodeDensity,
    pub plain_highlight: bool,
    pub font_scale: f64,
    pub margin_size: u16,
    pub line_spacing: LineSpacing,
    pub mouse_capture: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme_id: ColorSchemeId::Codex,
            appearance_theme_id: AppearanceThemeId::Dark,
            progress_visibility: ProgressVisibility::TimeChapter,
            render_mode: RenderMode::Code,
            code_language: CodeLanguage::TypeScript,
            code_density: CodeDensity::DEFAULT,
            plain_highlight: true,
            font_scale: 1.0,
            margin_size: 0,
            line_spacing: LineSpacing::Normal,
            mouse_capture: false,
        }
    }
}

/// The v1 `settings` table keys, in the order v1 seeded them.
pub const SETTINGS_KEYS: [&str; 11] = [
    "themeId",
    "appearanceThemeId",
    "progressVisibility",
    "renderMode",
    "codeLanguage",
    "codeDensity",
    "plainHighlight",
    "fontScale",
    "marginSize",
    "lineSpacing",
    "mouseCapture",
];

/// Format a float the way `String(value)` did in v1, so `1` never becomes `1.0`.
fn format_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        let text = format!("{value}");
        text
    }
}

impl AppSettings {
    /// The stored string for one key, matching v1's `String(value)` coercion.
    #[must_use]
    pub fn value_for_key(&self, key: &str) -> Option<String> {
        Some(match key {
            "themeId" => self.theme_id.as_str().to_owned(),
            "appearanceThemeId" => self.appearance_theme_id.as_str().to_owned(),
            "progressVisibility" => self.progress_visibility.as_str().to_owned(),
            "renderMode" => self.render_mode.as_str().to_owned(),
            "codeLanguage" => self.code_language.as_str().to_owned(),
            "codeDensity" => self.code_density.get().to_string(),
            "plainHighlight" => self.plain_highlight.to_string(),
            "fontScale" => format_number(self.font_scale),
            "marginSize" => self.margin_size.to_string(),
            "lineSpacing" => self.line_spacing.as_str().to_owned(),
            "mouseCapture" => self.mouse_capture.to_string(),
            _ => return None,
        })
    }

    /// Every key/value pair to persist, in v1's seeding order.
    #[must_use]
    pub fn entries(&self) -> Vec<(&'static str, String)> {
        SETTINGS_KEYS
            .into_iter()
            .filter_map(|key| self.value_for_key(key).map(|value| (key, value)))
            .collect()
    }

    /// Apply one stored key/value pair. Invalid values are ignored, exactly as
    /// v1's `getSettings` did, and the return value reports whether it applied.
    pub fn apply_stored(&mut self, key: &str, value: &str) -> bool {
        match key {
            "themeId" => ColorSchemeId::from_id(value)
                .map(|parsed| self.theme_id = parsed)
                .is_some(),
            "appearanceThemeId" => AppearanceThemeId::from_id(value)
                .map(|parsed| self.appearance_theme_id = parsed)
                .is_some(),
            "progressVisibility" => ProgressVisibility::from_id(value)
                .map(|parsed| self.progress_visibility = parsed)
                .is_some(),
            "renderMode" => RenderMode::from_id(value)
                .map(|parsed| self.render_mode = parsed)
                .is_some(),
            "codeLanguage" => CodeLanguage::from_id(value)
                .map(|parsed| self.code_language = parsed)
                .is_some(),
            "codeDensity" => value
                .parse::<u8>()
                .ok()
                .and_then(CodeDensity::new)
                .map(|parsed| self.code_density = parsed)
                .is_some(),
            // v1 compared against the literal "true", so anything else is false.
            "plainHighlight" => {
                self.plain_highlight = value == "true";
                true
            }
            "mouseCapture" => {
                self.mouse_capture = value == "true";
                true
            }
            "fontScale" => value
                .parse::<f64>()
                .ok()
                .filter(|parsed| is_font_scale(*parsed))
                .map(|parsed| self.font_scale = parsed)
                .is_some(),
            "marginSize" => value
                .parse::<u16>()
                .ok()
                .filter(|parsed| is_margin_size(*parsed))
                .map(|parsed| self.margin_size = parsed)
                .is_some(),
            "lineSpacing" => LineSpacing::from_id(value)
                .map(|parsed| self.line_spacing = parsed)
                .is_some(),
            _ => false,
        }
    }

    /// Build settings from stored rows, defaulting anything missing or invalid.
    #[must_use]
    pub fn from_stored<'a, I>(rows: I) -> Self
    where
        I: IntoIterator<Item = (&'a str, &'a str)>,
    {
        let mut settings = Self::default();
        for (key, value) in rows {
            settings.apply_stored(key, value);
        }
        settings
    }

    /// The theme these settings resolve to.
    #[must_use]
    pub fn theme(&self) -> crate::theme::Theme {
        crate::theme::Theme::resolve(self.theme_id, self.appearance_theme_id)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AppSettings, CodeDensity, CodeLanguage, LineSpacing, ProgressVisibility, RenderMode,
        SETTINGS_KEYS, is_font_scale, is_margin_size,
    };
    use crate::theme::{AppearanceThemeId, ColorSchemeId};

    #[test]
    fn defaults_match_the_v1_seeded_row_values() {
        let settings = AppSettings::default();
        let entries = settings.entries();
        assert_eq!(
            entries,
            vec![
                ("themeId", "codex".to_owned()),
                ("appearanceThemeId", "dark".to_owned()),
                ("progressVisibility", "time-chapter".to_owned()),
                ("renderMode", "code".to_owned()),
                ("codeLanguage", "typescript".to_owned()),
                ("codeDensity", "3".to_owned()),
                ("plainHighlight", "true".to_owned()),
                ("fontScale", "1".to_owned()),
                ("marginSize", "0".to_owned()),
                ("lineSpacing", "normal".to_owned()),
                ("mouseCapture", "false".to_owned()),
            ]
        );
    }

    #[test]
    fn every_key_has_a_value_and_every_value_has_a_key() {
        let settings = AppSettings::default();
        for key in SETTINGS_KEYS {
            assert!(settings.value_for_key(key).is_some(), "missing {key}");
        }
        assert!(settings.value_for_key("nope").is_none());
    }

    #[test]
    fn stored_rows_round_trip() {
        let settings = AppSettings {
            theme_id: ColorSchemeId::Forest,
            appearance_theme_id: AppearanceThemeId::LightColorblind,
            progress_visibility: ProgressVisibility::Hidden,
            render_mode: RenderMode::Plain,
            code_language: CodeLanguage::Rust,
            code_density: CodeDensity::new(5).expect("5 is a valid density"),
            plain_highlight: false,
            font_scale: 1.3,
            margin_size: 12,
            line_spacing: LineSpacing::Relaxed,
            mouse_capture: true,
        };

        let stored = settings.entries();
        let borrowed: Vec<(&str, &str)> = stored
            .iter()
            .map(|(key, value)| (*key, value.as_str()))
            .collect();

        assert_eq!(AppSettings::from_stored(borrowed), settings);
    }

    #[test]
    fn invalid_stored_values_keep_the_default() {
        let settings = AppSettings::from_stored([
            ("codeDensity", "9"),
            ("fontScale", "2"),
            ("marginSize", "7"),
            ("lineSpacing", "airy"),
            ("renderMode", "sneaky"),
            ("themeId", "solarized"),
            ("unknownKey", "whatever"),
        ]);
        assert_eq!(settings, AppSettings::default());
    }

    #[test]
    fn boolean_settings_only_accept_the_literal_true() {
        let settings =
            AppSettings::from_stored([("plainHighlight", "1"), ("mouseCapture", "true")]);
        assert!(!settings.plain_highlight);
        assert!(settings.mouse_capture);
    }

    #[test]
    fn allow_lists_reject_off_scale_values() {
        assert!(is_font_scale(1.15));
        assert!(!is_font_scale(1.2));
        assert!(is_margin_size(24));
        assert!(!is_margin_size(3));
        assert!(CodeDensity::new(0).is_none());
        assert!(CodeDensity::new(6).is_none());
        assert_eq!(CodeDensity::new(1), Some(CodeDensity::MIN));
    }

    #[test]
    fn progress_visibility_cycles_in_v1_order() {
        let mut visibility = ProgressVisibility::TimeChapter;
        let mut seen = Vec::new();
        for _ in 0..ProgressVisibility::ALL.len() {
            seen.push(visibility.as_str());
            visibility = visibility.next();
        }
        assert_eq!(
            seen,
            vec![
                "time-chapter",
                "time-book",
                "book",
                "both",
                "chapter",
                "hidden"
            ]
        );
        assert_eq!(visibility, ProgressVisibility::TimeChapter);
    }
}

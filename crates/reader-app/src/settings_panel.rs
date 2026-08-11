//! The tabbed settings panel.
//!
//! Changing a setting here applies immediately, so the page behind the panel
//! shows the result — that preview is the point of the panel, since no
//! description of "relaxed line spacing" is as useful as seeing it. The change
//! is not written to the database until it is saved, and cancelling restores
//! what was there before.

use reader_core::settings::{
    AppSettings, CodeDensity, CodeLanguage, FONT_SCALES, LineSpacing, MARGIN_SIZES, RenderMode,
    SettingsTab,
};
use reader_core::theme::{AppearanceThemeId, ColorSchemeId};

use crate::state::ReaderState;

/// One line of the panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingRow {
    pub display: String,
    pub search: String,
    pub field: SettingField,
}

/// A setting the panel can change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingField {
    ColorScheme,
    Appearance,
    RenderMode,
    CodeLanguage,
    CodeDensity,
    PlainHighlight,
    FontScale,
    MarginSize,
    LineSpacing,
    ProgressVisibility,
    MouseCapture,
}

impl SettingField {
    /// Which tab this setting lives on.
    #[must_use]
    pub const fn tab(self) -> SettingsTab {
        match self {
            Self::ColorScheme | Self::Appearance => SettingsTab::Themes,
            Self::RenderMode | Self::CodeLanguage | Self::CodeDensity | Self::PlainHighlight => {
                SettingsTab::Reading
            }
            Self::FontScale | Self::MarginSize | Self::LineSpacing => SettingsTab::Layout,
            Self::ProgressVisibility | Self::MouseCapture => SettingsTab::More,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::ColorScheme => "Colorscheme",
            Self::Appearance => "Appearance",
            Self::RenderMode => "Reading mode",
            Self::CodeLanguage => "Code language",
            Self::CodeDensity => "Code density",
            Self::PlainHighlight => "Dialogue highlight",
            Self::FontScale => "Font scale",
            Self::MarginSize => "Margin",
            Self::LineSpacing => "Line spacing",
            Self::ProgressVisibility => "Progress display",
            Self::MouseCapture => "Mouse capture",
        }
    }

    /// Every field, in panel order.
    pub const ALL: [Self; 11] = [
        Self::ColorScheme,
        Self::Appearance,
        Self::RenderMode,
        Self::CodeLanguage,
        Self::CodeDensity,
        Self::PlainHighlight,
        Self::FontScale,
        Self::MarginSize,
        Self::LineSpacing,
        Self::ProgressVisibility,
        Self::MouseCapture,
    ];

    /// How the setting's current value reads.
    #[must_use]
    pub fn value(self, settings: &AppSettings) -> String {
        match self {
            Self::ColorScheme => settings.theme_id.label().to_owned(),
            Self::Appearance => settings.appearance_theme_id.label().to_owned(),
            Self::RenderMode => match settings.render_mode {
                RenderMode::Plain => "plain".to_owned(),
                RenderMode::Code => format!("code ({})", settings.code_language.as_str()),
            },
            Self::CodeLanguage => settings.code_language.as_str().to_owned(),
            Self::CodeDensity => settings.code_density.get().to_string(),
            Self::PlainHighlight => on_off(settings.plain_highlight),
            Self::FontScale => format!("{:.2}×", settings.font_scale),
            Self::MarginSize => format!("{} columns", settings.margin_size),
            Self::LineSpacing => settings.line_spacing.as_str().to_owned(),
            Self::ProgressVisibility => settings.progress_visibility.as_str().to_owned(),
            Self::MouseCapture => on_off(settings.mouse_capture),
        }
    }

    /// Advance the setting to its next value, wrapping.
    pub fn cycle(self, settings: &mut AppSettings) {
        match self {
            Self::ColorScheme => {
                settings.theme_id = next_in(&ColorSchemeId::ALL, settings.theme_id)
            }
            Self::Appearance => {
                settings.appearance_theme_id =
                    next_in(&AppearanceThemeId::ALL, settings.appearance_theme_id);
            }
            Self::RenderMode => {
                settings.render_mode = match settings.render_mode {
                    RenderMode::Plain => RenderMode::Code,
                    RenderMode::Code => RenderMode::Plain,
                };
            }
            Self::CodeLanguage => {
                settings.code_language = next_in(&CodeLanguage::ALL, settings.code_language);
            }
            Self::CodeDensity => {
                let next = settings.code_density.get() % CodeDensity::MAX.get() + 1;
                settings.code_density = CodeDensity::new(next).unwrap_or(CodeDensity::DEFAULT);
            }
            Self::PlainHighlight => settings.plain_highlight = !settings.plain_highlight,
            Self::FontScale => settings.font_scale = next_value(&FONT_SCALES, settings.font_scale),
            Self::MarginSize => settings.margin_size = next_in(&MARGIN_SIZES, settings.margin_size),
            Self::LineSpacing => {
                settings.line_spacing = next_in(&LineSpacing::ALL, settings.line_spacing);
            }
            Self::ProgressVisibility => {
                settings.progress_visibility = settings.progress_visibility.next();
            }
            Self::MouseCapture => settings.mouse_capture = !settings.mouse_capture,
        }
    }
}

fn on_off(value: bool) -> String {
    if value { "on" } else { "off" }.to_owned()
}

/// The entry after `current`, wrapping at the end.
fn next_in<T: PartialEq + Copy>(values: &[T], current: T) -> T {
    let position = values
        .iter()
        .position(|value| *value == current)
        .unwrap_or(0);
    values[(position + 1) % values.len()]
}

/// The float after `current`, compared with a tolerance.
fn next_value(values: &[f64], current: f64) -> f64 {
    let position = values
        .iter()
        .position(|value| (value - current).abs() < f64::EPSILON)
        .unwrap_or(0);
    values[(position + 1) % values.len()]
}

/// The rows of the active tab.
#[must_use]
pub fn rows(state: &ReaderState) -> Vec<SettingRow> {
    let tab = state.settings_tab;
    let mut rows = vec![SettingRow {
        display: format!(
            "  {}",
            SettingsTab::ALL
                .iter()
                .map(|candidate| {
                    if *candidate == tab {
                        format!("[{}]", candidate.label())
                    } else {
                        format!(" {} ", candidate.label())
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        ),
        // The tab strip is a heading, not something a query should surface.
        search: String::new(),
        field: SettingField::ColorScheme,
    }];

    rows.extend(
        SettingField::ALL
            .into_iter()
            .filter(|field| field.tab() == tab)
            .map(|field| SettingRow {
                display: format!("  {:<20} {}", field.label(), field.value(&state.settings)),
                search: format!("{} {}", field.label(), field.value(&state.settings)),
                field,
            }),
    );
    rows
}

/// Which row index the tab strip occupies; it cannot be changed.
pub const TAB_STRIP_ROW: usize = 0;

/// Open the panel, remembering what to restore if it is cancelled.
pub fn open(state: &mut ReaderState) {
    state.settings_backup = Some(state.settings);
    state.settings_tab = SettingsTab::Themes;
}

/// Move to the next or previous tab.
pub fn cycle_tab(state: &mut ReaderState, forward: bool) {
    let tabs = SettingsTab::ALL;
    let position = tabs
        .iter()
        .position(|tab| *tab == state.settings_tab)
        .unwrap_or(0);
    let next = if forward {
        (position + 1) % tabs.len()
    } else {
        (position + tabs.len() - 1) % tabs.len()
    };
    state.settings_tab = tabs[next];
}

/// Change the setting on `row`, previewing it immediately.
///
/// Returns whether anything changed — the tab strip is not a setting.
pub fn change(state: &mut ReaderState, row_index: usize) -> Option<SettingField> {
    let rows = rows(state);
    if row_index == TAB_STRIP_ROW {
        return None;
    }
    let field = rows.get(row_index)?.field;
    let mut settings = state.settings;
    field.cycle(&mut settings);
    state.settings = settings;
    state.refresh_theme();
    Some(field)
}

/// Keep the previewed settings. The caller persists them.
pub fn save(state: &mut ReaderState) -> AppSettings {
    state.settings_backup = None;
    state.settings
}

/// Discard the preview and restore what was there when the panel opened.
pub fn cancel(state: &mut ReaderState) {
    if let Some(previous) = state.settings_backup.take() {
        state.settings = previous;
        state.refresh_theme();
    }
}

#[cfg(test)]
mod tests {
    use reader_core::settings::{
        AppSettings, CodeLanguage, FONT_SCALES, LineSpacing, MARGIN_SIZES, ProgressVisibility,
        RenderMode, SettingsTab,
    };

    use super::{SettingField, TAB_STRIP_ROW, cancel, change, cycle_tab, open, rows, save};
    use crate::state::ReaderState;

    fn reader() -> ReaderState {
        let mut state = ReaderState::new(AppSettings::default());
        open(&mut state);
        state
    }

    fn labels(state: &ReaderState) -> Vec<String> {
        rows(state)
            .into_iter()
            .skip(1)
            .map(|row| row.display.trim().to_owned())
            .collect()
    }

    #[test]
    fn each_tab_shows_its_own_settings() {
        let mut state = reader();
        assert_eq!(state.settings_tab, SettingsTab::Themes);
        assert_eq!(labels(&state).len(), 2, "themes has two settings");

        cycle_tab(&mut state, true);
        assert_eq!(state.settings_tab, SettingsTab::Reading);
        assert_eq!(labels(&state).len(), 4);

        cycle_tab(&mut state, true);
        assert_eq!(state.settings_tab, SettingsTab::Layout);
        assert_eq!(labels(&state).len(), 3);

        cycle_tab(&mut state, true);
        assert_eq!(state.settings_tab, SettingsTab::More);
        assert_eq!(labels(&state).len(), 2);

        cycle_tab(&mut state, true);
        assert_eq!(state.settings_tab, SettingsTab::Themes, "tabs wrap");
        cycle_tab(&mut state, false);
        assert_eq!(state.settings_tab, SettingsTab::More, "and wrap backwards");
    }

    #[test]
    fn the_tab_strip_marks_the_active_tab_and_is_not_a_setting() {
        let mut state = reader();
        let strip = &rows(&state)[TAB_STRIP_ROW];
        assert!(strip.display.contains("[Themes]"), "{}", strip.display);
        assert!(strip.display.contains(" Reading "), "{}", strip.display);
        assert!(strip.search.is_empty(), "the strip is not searchable");
        assert_eq!(change(&mut state, TAB_STRIP_ROW), None);
    }

    #[test]
    fn changing_a_setting_previews_it_immediately() {
        let mut state = reader();
        let before = state.theme.clone();

        let changed = change(&mut state, 1).expect("a setting changed");

        assert_eq!(changed, SettingField::ColorScheme);
        assert_ne!(state.theme, before, "the preview applies at once");
        assert_eq!(state.settings.theme_id.as_str(), "claude");
    }

    #[test]
    fn cancelling_restores_everything_the_panel_changed() {
        let mut state = reader();
        let original = state.settings;
        let original_theme = state.theme.clone();

        change(&mut state, 1);
        change(&mut state, 2);
        cycle_tab(&mut state, true);
        change(&mut state, 3);
        assert_ne!(state.settings, original);

        cancel(&mut state);

        assert_eq!(state.settings, original);
        assert_eq!(state.theme, original_theme);
        assert!(state.settings_backup.is_none());
    }

    #[test]
    fn saving_keeps_the_preview_and_returns_what_to_persist() {
        let mut state = reader();
        change(&mut state, 1);
        let previewed = state.settings;

        let saved = save(&mut state);

        assert_eq!(saved, previewed);
        assert!(state.settings_backup.is_none());
        cancel(&mut state);
        assert_eq!(
            state.settings, previewed,
            "cancelling after a save does nothing"
        );
    }

    #[test]
    fn every_setting_cycles_through_its_allowed_values_and_returns() {
        for field in SettingField::ALL {
            let mut settings = AppSettings::default();
            let start = settings;
            let mut seen = Vec::new();
            for _ in 0..12 {
                field.cycle(&mut settings);
                seen.push(field.value(&settings));
                if settings == start {
                    break;
                }
            }
            assert_eq!(
                settings, start,
                "{field:?} did not return to where it started: {seen:?}"
            );
            assert!(seen.len() > 1, "{field:?} has only one value");
        }
    }

    #[test]
    fn cycling_stays_inside_the_allowed_sets() {
        let mut settings = AppSettings::default();
        for _ in 0..20 {
            SettingField::FontScale.cycle(&mut settings);
            assert!(
                FONT_SCALES
                    .iter()
                    .any(|scale| (scale - settings.font_scale).abs() < f64::EPSILON),
                "{} is not an allowed scale",
                settings.font_scale
            );

            SettingField::MarginSize.cycle(&mut settings);
            assert!(MARGIN_SIZES.contains(&settings.margin_size));

            SettingField::CodeDensity.cycle(&mut settings);
            assert!((1..=5).contains(&settings.code_density.get()));
        }
    }

    #[test]
    fn values_read_the_way_the_panel_shows_them() {
        let mut settings = AppSettings::default();
        assert_eq!(
            SettingField::RenderMode.value(&settings),
            "code (typescript)"
        );
        settings.render_mode = RenderMode::Plain;
        assert_eq!(SettingField::RenderMode.value(&settings), "plain");

        settings.font_scale = 1.15;
        assert_eq!(SettingField::FontScale.value(&settings), "1.15×");
        settings.margin_size = 12;
        assert_eq!(SettingField::MarginSize.value(&settings), "12 columns");
        settings.line_spacing = LineSpacing::Relaxed;
        assert_eq!(SettingField::LineSpacing.value(&settings), "relaxed");
        settings.plain_highlight = false;
        assert_eq!(SettingField::PlainHighlight.value(&settings), "off");
        settings.progress_visibility = ProgressVisibility::Hidden;
        assert_eq!(SettingField::ProgressVisibility.value(&settings), "hidden");
        settings.code_language = CodeLanguage::Rust;
        assert_eq!(SettingField::CodeLanguage.value(&settings), "rust");
    }

    #[test]
    fn every_field_belongs_to_exactly_one_tab() {
        for tab in SettingsTab::ALL {
            let count = SettingField::ALL
                .into_iter()
                .filter(|field| field.tab() == tab)
                .count();
            assert!(count > 0, "{tab:?} has no settings");
        }
        assert_eq!(SettingField::ALL.len(), 11);
    }

    #[test]
    fn settings_can_be_searched_by_name_or_value() {
        let mut state = reader();
        state.settings_tab = SettingsTab::Layout;
        let storage = reader_storage::Storage::open_in_memory().expect("database");
        state.overlay = crate::state::Overlay::Settings;

        state.overlay_search.buffer = "margin".into();
        let found = crate::overlay::visible_entries(&state, &storage, 0);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].display.contains("Margin"));
    }
}

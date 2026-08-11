# Kindle-style Reader Settings

## Goal

Replace the flat searchable settings list with a Kindle-inspired, tabbed TUI that keeps changes in a draft, previews them immediately, and applies them atomically.

## Terminal adaptation

A terminal application cannot portably change the emulator's real font face or point size. The UI therefore uses a `Text size` control that narrows the effective wrapping width. This produces the reading-density effect of a larger font without terminal-specific escape sequences.

## Tabs

| Tab | Controls |
|---|---|
| `Themes` | Appearance theme, colorscheme |
| `Reading` | Plain/stealth reading mode, dialogue highlight |
| `Layout` | Text size, page margins, line spacing, code density |
| `More` | Progress display, mouse capture |

## Interaction

- `Left` / `Right` or `h` / `l`: change tab and reset its selection.
- `Up` / `Down` or `j` / `k`: select a control within the active tab.
- `Space`: cycle the selected draft value.
- `/`: search within the active tab.
- `Enter`: apply and persist the complete draft.
- `Esc`: discard the draft.

## Layout controls

- `Text size`: `Standard`, `Medium`, `Large`, `Extra large`; implemented as effective wrap scales `1`, `1.15`, `1.3`, and `1.5`.
- `Page margins`: equal horizontal margins from `0` to `24` terminal columns.
- `Line spacing`: `Compact`, `Normal`, or `Relaxed`. Normal preserves the existing renderer spacing; Compact reduces paragraph gaps; Relaxed also separates wrapped lines.

All three settings are stored in the existing `settings` table. Existing installations receive backward-compatible defaults: `Standard`, no added margin, and `Normal` spacing.

## Preview and commit model

The panel preview is rendered from `SettingsPanelDraft`, including the draft theme, reading mode, and layout labels. The underlying reader state is unchanged until `Enter`; `Esc` closes the panel without mutating or persisting the reader settings.

# UX: Wide Mode (Two Columns)

## Goal
On terminals at least 120 columns wide, display the text in two side-by-side columns — simulating an IDE layout with two code panes open.

## Context
`src/tui.ts` — `draw`, `currentLines`.
`src/screen.ts` — `getViewportLayout`, `renderBody`.
`src/types.ts` — `AppState`.

## Design

### Activation
- `/wide` or the `W` key (uppercase) → toggles `state.wideMode`.
- Requires `process.stdout.columns >= 120`. Below that → status: `"Wide mode requires at least 120 columns"`.

### Column Layout
```
┌─────────────────────────────────────────────────────────┐
│ status bar (full width)                                 │
├──────────────────────┬──────────────────────────────────┤
│ Column A             │ Column B                         │
│ (blockOffset + 0..N) │ (blockOffset + bodyHeight..2N)   │
│                      │                                  │
├──────────────────────┴──────────────────────────────────┤
│ footer (full width)                                     │
└─────────────────────────────────────────────────────────┘
```

- `columnWidth = Math.floor((totalWidth - 3) / 2)` (3 = the `│` separator plus margins).
- Column A: lines `blockOffset..blockOffset + bodyHeight`.
- Column B: lines `blockOffset + bodyHeight..blockOffset + 2 * bodyHeight`.
- Scrolling advances `bodyHeight * 2` lines at a time (the page size doubles).
- Scrollbar: based on the total line count with `effectivePageSize = bodyHeight * 2`.

### Column Separator
A column of `│` characters in `theme.border` separating the two columns.

### Rendering
In `draw()`:
```ts
if (state.wideMode && width >= 120) {
  // render two columns
} else {
  // render a single column (current behavior)
}
```

### Overlays in Wide Mode
Overlays (chapters, books, themes) keep the current layout (full width or right side) — they are not split into columns.

### Focus Mode in Wide Mode
Incompatible — if `focusMode` is active, ignore `wideMode`.

## Files to Modify
- `src/types.ts`: `wideMode: boolean` field on `AppState`
- `src/tui.ts`: rendering branch in `draw`
- `src/screen.ts`: `getViewportLayout` returns the `wideMode` layout when active
- `src/input.ts`: the `W` key
- `src/commands.ts`: `/wide`
- `src/help.ts`: document `W`

## Acceptance Criteria
- The two columns show contiguous book content (not duplicated).
- Scrolling advances correctly (skipping twice as many lines).
- Automatically disabled on narrow terminals, with a warning.
- Overlays are not broken.

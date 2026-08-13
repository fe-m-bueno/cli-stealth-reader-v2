# UX: Text Overhead Adjustment for Large Fonts (/fontsize)

## Goal
Let the user declare their terminal's font scale factor (for example, a large font means visually wider characters), adjusting `TEXT_OVERHEAD` dynamically to avoid overflow at larger font sizes.

## Context
`src/renderers.ts` — the `TEXT_OVERHEAD = 42` constant.
`src/screen.ts` — `getViewportLayout` uses `process.stdout.columns`.

## Problem
`TEXT_OVERHEAD` is fixed at 42. If the user has a large font and the terminal reports 80 columns but the lines look visually narrower, the text may appear to break at an odd point. The opposite is also true: a narrow monospaced font at 200 columns → lines that are far too long.

## Design

### Command
`/fontsize <scale>`, where scale is `1.0` (default), `1.5`, `2.0`, `0.75`, and so on.
Also accept integer values: `/fontsize 2` = scale 2.0.

### What It Adjusts
The terminal's font cannot be controlled through escape codes.
What the app can control:
1. **`textWidth`** in `renderCode`: `Math.max(width / scale - TEXT_OVERHEAD, 20)` — reduces the effective width for larger fonts.
2. **Implicit margin** — with scale > 1, apply `Math.floor((scale - 1) * 10)` columns of automatic margin.
3. **Wrapping in plain mode** — `wrapText(text, width / scale)` to break earlier.

### State
```ts
// AppSettings / AppState
fontScale: number; // default 1.0
```

### Implementation
Pass `fontScale` to `renderBlocks` and `getViewportLayout`:
```ts
// renderers.ts
const textWidth = Math.max(Math.floor(width / state.fontScale) - TEXT_OVERHEAD, 20);

// screen.ts
const effectiveColumns = Math.floor(columns / fontScale);
```

### Feedback
`/fontsize 1.5` → `"Font scale set to 1.5x (effective width: 80 → 53 columns)"`.

### Persistence
`storage.setSetting("fontScale", String(scale))`.

### Shortcut Keys (optional)
`+` / `-` to increase/decrease the scale by 0.25.
> Careful: `+` is not currently in use — check for a collision.

## Files to Modify
- `src/types.ts`: `fontScale` on `AppSettings` and `AppState`
- `src/storage.ts`: read/write `fontScale`
- `src/renderers.ts`: use `fontScale` when computing `textWidth`
- `src/screen.ts`: `getViewportLayout` receives and uses `fontScale`
- `src/tui.ts`: initialize and pass `fontScale`
- `src/commands.ts`: `/fontsize`
- `src/executor.ts`: implementation

## Acceptance Criteria
- `/fontsize 2` effectively halves the content width.
- `/fontsize 1` restores the default behavior.
- It persists across sessions.
- It works together with `/margin`.

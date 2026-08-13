# Reading: Focus Mode

## Goal
Show only one paragraph at a time, centered on the screen, removing every distraction.
In code mode, it looks as if the user is debugging or reading a single isolated function.

## Context
`src/tui.ts` — `currentLines`, `draw`.
`src/types.ts` — `AppState`.
`src/input.ts` — navigation.

## Design

### State
```ts
// types.ts — AppState
focusMode: boolean;
focusBlockIndex: number; // index of the current block within the chapter
```

### Rendering in Focus Mode
Replace `currentLines` when `focusMode === true`:
1. Take `chapter.blocks[state.focusBlockIndex]`.
2. Render it with `renderBlocks([block], mode, width, theme)`.
3. Center it vertically: top padding = `Math.floor((bodyHeight - lines.length) / 2)`.
4. Optionally show the block number and total in the footer: `§ 42 / 318`.

### Navigation in Focus Mode
- `k` / `Space` → next block (`focusBlockIndex++`); at the end of the chapter → next chapter.
- `j` → previous block.
- Reaching the end of the block and pressing k again → show the chapter transition (the same mechanism as today).
- `g` / `G` → first/last block of the chapter.
- `Esc` or `f` → leave focus mode.

### Activation
- The `f` key → toggles `state.focusMode`.
- On entering focus mode, `focusBlockIndex` is computed from the current `blockOffset` (mapping the offset to the nearest visible block index).

### Status Bar
Show `[FOCUS]` next to the renderMode.

### No Scrollbar in Focus Mode
`renderScrollbar` returns `[]` when `focusMode === true`.

## Files to Modify
- `src/types.ts`: fields on `AppState`
- `src/tui.ts`: `currentLines` and `draw` with a branch for focus mode
- `src/input.ts`: the `f` key, block-by-block navigation
- `src/screen.ts`: `renderStatusBar` or `renderFooter` to show `[FOCUS]`
- `src/help.ts`: document `f`

## Acceptance Criteria
- Focus mode shows exactly one centered block.
- `k`/`j` move forward/back by block, not by line.
- Toggling `f` returns to the equivalent position in normal mode (visible block → blockOffset).
- Works in both plain and code mode.

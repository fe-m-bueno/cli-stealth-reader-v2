# Navigation: Position History ([ and ])

## Goal

Move backward and forward through the history of visited positions, the way a browser or code editor does.

## Context

`src/types.ts` — `AppState`.
`src/input.ts` — key handlers.

## Design

### State

```ts
// types.ts
export interface NavHistoryEntry {
  chapterIndex: number;
  blockOffset: number;
}

// AppState:
navHistory: NavHistoryEntry[];
navHistoryCursor: number; // current index in the history
```

### Rules

- Every "intentional" position change (changing chapter, `/goto`, clicking a bookmark, Enter in an overlay) adds an entry to the history.
- Normal scrolling (j/k/Space) does NOT add to the history (it would create spam).
- The history is capped at 50 entries (drop the oldest when adding beyond the limit).
- When a new entry is added while `navHistoryCursor < navHistory.length - 1`, discard the entries after the cursor (like a browser).

### Keys

- `[` → go back in the history (to `navHistory[cursor - 1]`)
- `]` → go forward in the history (to `navHistory[cursor + 1]`)
- At the start/end, show `status = "No history to go back"` / `"No history to go forward"`.

### Helper Function

```ts
function pushNavHistory(state: AppState): void {
  const entry = { chapterIndex: state.chapterIndex, blockOffset: state.blockOffset };
  // discard forward history if the cursor is not at the end
  state.navHistory = state.navHistory.slice(0, state.navHistoryCursor + 1);
  state.navHistory.push(entry);
  if (state.navHistory.length > 50) state.navHistory.shift();
  state.navHistoryCursor = state.navHistory.length - 1;
}
```

Call `pushNavHistory` before jumps in `executor.ts` (goto, chapter select, bookmark navigate).

## Files to Modify

- `src/types.ts`: `NavHistoryEntry`, fields on `AppState`
- `src/input.ts`: the `[` and `]` keys
- `src/executor.ts`: call `pushNavHistory` on jumps
- `src/tui.ts`: initialize `navHistory: [], navHistoryCursor: -1`
- `src/help.ts`: document `[` and `]`

## Acceptance Criteria

- `[` and `]` work correctly after chapter changes and gotos.
- Normal scrolling does not pollute the history.
- The history does not persist across sessions (in memory only).

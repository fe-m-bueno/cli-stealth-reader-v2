# Navigation: Search (/search)

## Goal

Search for a text term within the current chapter or the whole book, with navigation between results.

## Context

`src/commands.ts` — the slash command system.
`src/executor.ts` — command execution.
`src/types.ts` — `AppState`.
`src/tui.ts` — rendering and overlays.

## Design

### Command

`/search <term>` — searches the current chapter.
`/search -g <term>` (or `/search --global`) — searches every chapter.

### State

```ts
// types.ts
export interface SearchState {
  query: string;
  global: boolean;
  results: Array<{ chapterIndex: number; blockIndex: number; lineIndex: number }>;
  cursor: number; // current result
}
// AppState: searchState: SearchState | null
```

### Flow

1. `/search term` → scan `chapter.blocks[].text` with `text.toLowerCase().includes(query)`.
2. Each match → record `{ chapterIndex, blockIndex }`.
3. Navigate to the first result: set `state.chapterIndex` and `state.blockOffset` to the matching block.
4. The `n` / `N` keys → next/previous result (when `searchState !== null`).
5. `Esc` or a new `/search` clears the search state.

### Highlight

In `renderPlain` and `renderCode`, when `searchState` is active, wrap occurrences of the term with `bg(theme.warning, match)`.

### Status Bar

Show `[3/12] "term"` in the status bar while a search is active.

### Overlay (optional)

For global searches with many results, show a chapters-style overlay listing `Chapter X: match preview`.

## Files to Modify

- `src/types.ts`: `SearchState`, field on `AppState`
- `src/commands.ts`: definition of `/search`
- `src/executor.ts`: search implementation
- `src/renderers.ts`: match highlighting
- `src/input.ts`: the `n` / `N` keys
- `src/tui.ts`: show the search state in the status bar

## Acceptance Criteria

- The search is case-insensitive.
- `n`/`N` cycle circularly.
- Global search crosses chapters.
- The highlight is visible in both modes (plain and code).

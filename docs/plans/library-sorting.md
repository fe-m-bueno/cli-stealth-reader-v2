# Library: Sorting the Book List

## Goal
Allow the book overlay to be sorted by different criteria: progress, title, author, date opened.

## Context
`src/storage.ts` — `listBooksWithProgress()`.
`src/tui.ts` — the `"books"` overlay.
`src/input.ts` — keys inside the overlay.

## Design

### Sort Criteria
```ts
export type LibrarySortKey = "lastOpened" | "title" | "author" | "progress";
export type SortDirection = "asc" | "desc";
```

Default: `lastOpened desc` (the current behavior).

### State
```ts
// AppState
librarySortKey: LibrarySortKey;
librarySortDir: SortDirection;
```

### Implementation
`listBooksWithProgress()` already returns the data needed.
Add `listBooksWithProgress(sort: LibrarySortKey, dir: SortDirection)`, which applies `ORDER BY` in SQLite (or sorts in memory for `progress`, which is computed).

```sql
-- For title/author/lastOpened: ORDER BY in the query
-- For progress: sort in memory after the query
```

### Keys Inside the Books Overlay
When `state.overlay === "books"`:
- `s` → cycles through the sort criterion: `lastOpened → title → author → progress → lastOpened`
- `r` → reverses the direction (asc/desc)

### Overlay Header
The first line of the books overlay shows the current criterion:
```
  Sort: Last Opened ↓   (Press s to change, r to reverse)
```

### Command
`/books --sort title` — opens the overlay already sorted by title.

## Files to Modify
- `src/storage.ts`: sort parameter on `listBooksWithProgress`
- `src/types.ts`: `LibrarySortKey`, `SortDirection`, fields on `AppState`
- `src/tui.ts`: initialize the sort, pass it to `renderOverlay`, show the header
- `src/input.ts`: `s` and `r` keys inside the books overlay
- `src/commands.ts`: `--sort` flag on `/books`

## Acceptance Criteria
- Sorting by title is alphabetical (case-insensitive).
- Sorting by progress puts unstarted books last.
- The direction persists while the overlay is open and resets when it closes.

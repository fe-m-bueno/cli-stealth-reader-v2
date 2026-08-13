# Navigation: Bookmarks (/mark, /marks)

## Goal

Save specific positions within chapters and navigate to them.

## Context

`src/storage.ts` — SQLite, existing tables.
`src/commands.ts`, `src/executor.ts`, `src/tui.ts`.

## Design

### SQLite Table

```sql
CREATE TABLE IF NOT EXISTS bookmarks (
  id TEXT PRIMARY KEY,
  book_id TEXT NOT NULL,
  chapter_index INTEGER NOT NULL,
  block_offset INTEGER NOT NULL,
  label TEXT,           -- optional bookmark name
  created_at INTEGER NOT NULL
);
```

### Commands

- `/mark` — creates a bookmark at the current position (auto label: `Ch.3 §42`)
- `/mark <label>` — creates one with a custom label
- `/marks` — opens an overlay listing the current book's bookmarks
- `/delmark <id|label>` — removes a bookmark

### Bookmarks Overlay

Reuse the existing overlay mechanism (`state.overlay = "bookmarks"`).
List: `> Ch.3 §42 — "bookmark label"  [2 days ago]`
Enter → navigates to the position.
`d` on an item → deletes it.

### Shortcut Key

`B` (uppercase) → opens the bookmarks overlay (the way `T` opens chapters).
`m` is already taken (toggle mode), so use `B` for "bookmark".

### Storage

Add methods to `Storage`:

```ts
addBookmark(bookId, chapterIndex, blockOffset, label?): Bookmark
listBookmarks(bookId): Bookmark[]
deleteBookmark(id): void
```

## Files to Modify

- `src/storage.ts`: new table and methods
- `src/types.ts`: `Bookmark`, `OverlayKind` += `"bookmarks"`, `AppState`
- `src/commands.ts`: `/mark`, `/marks`, `/delmark`
- `src/executor.ts`: implementation
- `src/tui.ts`: render the bookmarks overlay
- `src/input.ts`: the `B` key
- `src/help.ts`: update KEYBOARD_SHORTCUTS

## Acceptance Criteria

- Bookmarks persist across sessions.
- Navigating to a bookmark restores the chapter and offset exactly.
- The overlay shows the label and a relative date.

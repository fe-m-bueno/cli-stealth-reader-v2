# Library: Tags and Notes (/tag, /note)

## Goal
Allow books to be categorized with tags and free-form notes to be added per book or per position.

## Context
`src/storage.ts` — SQLite.
`src/commands.ts`, `src/executor.ts`.
`src/tui.ts` — the books overlay.

## Design

### SQLite Tables

```sql
CREATE TABLE IF NOT EXISTS book_tags (
  book_id TEXT NOT NULL,
  tag TEXT NOT NULL,
  PRIMARY KEY (book_id, tag)
);

CREATE TABLE IF NOT EXISTS notes (
  id TEXT PRIMARY KEY,
  book_id TEXT NOT NULL,
  chapter_index INTEGER,  -- null = note for the whole book
  block_offset INTEGER,
  content TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
```

### Tag Commands
- `/tag <tag>` — adds a tag to the current book (for example, `/tag fiction`, `/tag reading`)
- `/tag -d <tag>` — removes a tag
- `/tags` — lists the current book's tags

### Note Commands
- `/note <text>` — adds a note at the current position (chapterIndex + blockOffset)
- `/note -l` — lists the current book's notes in an overlay
- `/note -d <id>` — deletes a note

### Notes Overlay
`state.overlay = "notes"` — the list:
```
> Ch.3 §42  "Really good passage about..."   [3 days ago]
  Ch.1 §0   "Relevant historical context"    [1 week ago]
```
Enter → navigates to the note's position.

### Filtering the Library by Tag
In the `books` overlay, add a filter line: `/books fiction` → filters by tag.
Or the `f` key inside the books overlay to enter filter mode.

### Display in the Books Overlay
Add tags next to the title:
```
> Dom Casmurro — Machado de Assis  [Ch.3 · 42%]  #classic #reading
```

## Files to Modify
- `src/storage.ts`: new tables and `addTag`, `removeTag`, `listTags`, `addNote`, `listNotes`, `deleteNote` methods
- `src/types.ts`: `Note`, `OverlayKind` += `"notes"`, fields on `AppState`
- `src/commands.ts`: `/tag`, `/tags`, `/note`
- `src/executor.ts`: implementation
- `src/tui.ts`: notes overlay, display tags in the books overlay

## Acceptance Criteria
- Tags and notes persist across sessions.
- Navigating to a note restores the exact position.
- `/tag` with no argument shows the current book's tag list.

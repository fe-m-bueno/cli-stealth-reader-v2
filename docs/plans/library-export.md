# Library: Export Progress/Position as JSON

## Goal
Export and import reading state (positions, bookmarks, notes, tags) as JSON to sync across machines without depending on an external service.

## Context
`src/storage.ts` — the SQLite source of truth.
`src/commands.ts`, `src/executor.ts`.

## Design

### Export Format
```json
{
  "version": 1,
  "exportedAt": "2026-04-14T12:00:00Z",
  "positions": [
    {
      "bookImportHash": "sha256...",
      "bookTitle": "Dom Casmurro",
      "chapterIndex": 3,
      "blockOffset": 42,
      "bookProgress": 0.31
    }
  ],
  "bookmarks": [...],
  "notes": [...],
  "tags": [...]
}
```

Use `importHash` (already present on `CanonicalBook`) as the portable key — it does not depend on a local path.

### Commands
- `/export` — writes `stealth-reader-export.json` to the current directory (`state.cwd`)
- `/export <path>` — writes to the specified path
- `/import <path>` — reads the JSON and merges the positions (does not overwrite when the local one is newer)

### Merge Strategy on Import
- For each entry in the JSON: if `importHash` matches a local book → update the position if `exportedAt` is newer than the saved local position.
- Bookmarks/notes/tags: add them if they don't exist (additive merge).
- Books in the JSON that don't exist locally: skip (the book itself cannot be imported).

### Feedback
```
Exported 3 books to ./stealth-reader-export.json
Imported: 2 positions updated, 5 bookmarks added, 0 conflicts
```

## Files to Modify
- `src/storage.ts`: `exportAll(): ExportData` and `importMerge(data: ExportData): ImportResult` methods
- `src/commands.ts`: `/export`, `/import`
- `src/executor.ts`: implementation (using `fs.writeFileSync` / `fs.readFileSync`)
- `src/types.ts`: `ExportData` and `ImportResult` types

## Acceptance Criteria
- The generated JSON is human-readable and valid.
- Import does not erase newer local data.
- `/export` without write permission → a friendly error status.
- `importHash` guarantees matching without depending on file paths.

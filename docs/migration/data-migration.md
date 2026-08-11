# Reader-data migration

Wayfinder context: [Design a safe v1 reader-data migration](https://github.com/fe-m-bueno/cli-stealth-reader/issues/31).

## Decision

**There is no migration. Both implementations use the same database.**

v2 opens the v1 `library.db` in place, at the same XDG path, with the same
tables and columns. A reader can run either binary on any day of the beta and
find their books, positions, bookmarks, notes, and tags where they left them.

This is the safest option available because it removes the failure mode a
migration would introduce: there is no copy step to interrupt, no second file to
diverge, and no moment where the two implementations disagree about which store
is authoritative.

## What v2 changes on open

Three changes are applied when the database is opened. Each is idempotent, each
preserves every row, and each leaves the file readable by v1.

| Change | Why | Reversible |
| --- | --- | --- |
| `chapters` primary key becomes `(book_id, id)` | v1 could not store two books whose chapter hrefs collided; see [improvements](improvements.md) | Yes — v1 ignores the key change; its inserts and queries name unchanged columns |
| Four indexes added | Chapter loads and bookmark lists were full scans | Yes — an index is invisible to queries |
| Toggl keys in `command_history` rewritten to `<redacted>` | v1 already did this on open; v2 keeps the behavior | No, and deliberately: a leaked key should not come back |

The chapter-key rebuild runs inside one transaction, so an interruption leaves
the original table in place.

## Rollback

Going back to v1 needs no action: stop running v2 and run v1. Its
`CREATE TABLE IF NOT EXISTS` finds the tables already there, and every column it
reads or writes is unchanged.

The one asymmetry is the one worth having: a book that v2 stored *because* the
chapter-key fix allowed it — two books sharing chapter hrefs — will be visible to
v1 but v1 will fail to re-import it, exactly as it did before. Nothing is lost;
the old limitation simply returns with the old binary.

## Backups

The reader never had a backup story and does not need a new one, because the data
that matters is already portable: `/export` writes positions, bookmarks, notes,
and tags as version-1 JSON keyed by content hash, and `/import` merges it back
additively. That file is the supported way to move a reading history between
machines, and it round-trips between v1 and v2 — proven by
`exporting_a_v1_database_reproduces_the_v1_export`, which exports a real v1
database from v2 and compares it to what v1 produced.

Readers who want a snapshot before trying the beta can copy the file:

```bash
cp ~/.local/share/cli-stealth-reader/library.db ~/library.db.backup
```

## Evidence

`crates/reader-storage/tests/v1_compatibility.rs` runs against a database written
by the TypeScript v1, committed as a fixture. It asserts that v2 reads back the
settings, book list, chapters and blocks, positions, bookmarks, notes, tags, and
pace exactly as v1 reported them; that the export matches v1's export; that the
chapter-key migration preserves every row and is idempotent; and that the file
stays writable afterwards.

## When this decision expires

The shared database holds while both implementations are in use. Once v2 takes
over the `stealth-reader` command and v1 is retired, the constraint to stay
readable by v1 lifts, and schema changes that would break it — a real foreign
key on `chapters.book_id`, dropping the redundant `chapter_index`, storing blocks
in a column type other than JSON text — become available. None of them are needed
now, and none should be done while a rollback is still a supported action.

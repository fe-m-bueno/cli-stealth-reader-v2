# Deliberate improvements over v1

Parity is the default: v2 reproduces v1's observable behavior, including quirks,
and the golden fixtures prove it. This file lists the places where v2 knowingly
behaves differently, why, and what protects the change from regressing.

Anything not listed here is a bug, not a decision.

## Fixed: two books could not share chapter ids

**v1 behavior.** `chapters.id` was a global primary key, while the id was derived
as `sha1("<href>:<index>")` — from the chapter alone, with no reference to the
book. Two books whose chapter files share an href and position collide, and
`text/ch1.xhtml` at index 0 is an ordinary thing for two EPUBs to have. The
second import failed with `UNIQUE constraint failed: chapters.id`, and the book
could not be added at all.

**v2 behavior.** The primary key is `(book_id, id)`. The id derivation is
unchanged, so stored chapters keep their identifiers and everything that reads
them is unaffected; only the uniqueness scope moved.

**Migration.** Opening a v1 database rebuilds `chapters` with the composite key
inside one transaction, copying every row. It runs once, is idempotent, and a v1
build can still open the result: its `CREATE TABLE IF NOT EXISTS` sees the table
already exists, and its inserts and queries name columns that did not change.

**Protected by.** `two_books_can_share_chapter_ids` in `reader-storage`, and
`opening_a_v1_database_migrates_the_chapter_key_without_losing_rows`, which runs
against the committed v1 database fixture.

## Fixed: list items lost their indent when wrapped

**v1 behavior.** The bullet was folded into the text (`"  · " + text`) and the
result was word-wrapped. Wrapping splits on whitespace and drops empty pieces, so
the leading indent never survived — a list item rendered as `· item` at column
zero, and any wrapped continuation lost the association entirely:

```
· first item of the list that is long enough to
wrap onto another line
```

**v2 behavior.** The marker is kept out of the wrapped text and re-applied per
line, with continuation lines aligned under the first word:

```
  · first item of the list that is long enough
    to wrap onto another line
```

**Scope.** Plain mode only. Code mode never used the marker, so the disguises are
untouched, and the wrapped text still fits the same column width.

**Protected by.** `a_wrapped_list_item_keeps_its_indent_and_hangs_under_the_text`
and `a_list_item_wraps_the_same_way_with_highlighting_off` in `reader-core`, plus
`improves_on_v1_for_list_items` in the render parity suite, which asserts both
the old and the new shape so the change stays visible in review. The parity suite
allows divergence only for the list-item cases it names.

## Added: indexes v1 did not have

`chapters(book_id, chapter_index)`, `bookmarks(book_id, created_at DESC)`,
`diagnostics(book_id)`, and `books(last_opened_at DESC)`. These change no result,
only the work behind it: loading a book's chapters and listing its bookmarks were
full table scans. Creating them is idempotent and leaves the database readable by
v1.

## Structural changes that are not behavior changes

- **Rendering produces styled spans, not ANSI strings.** v1 emitted escape
  sequences and later had to parse them back to highlight search matches. v2
  renders structural spans and maps them to terminal attributes once, at the
  edge. The stripped text is identical, which the render parity fixture proves.
  Repaints now borrow each span's text when adapting it to Ratatui, so the cached
  `String`s are not cloned again for every visible line. The borrowed shape is
  guarded by `a_converted_line_borrows_its_text_from_the_render_cache`.
- **Render caches invalidate themselves from complete keys.** Chapter navigation,
  search jumps, history, settings changes, and terminal resizes no longer depend
  on command handlers remembering to discard cached data. Whole-book line counts
  are keyed by book id and import hash, geometry-affecting settings, and content
  width; rendered chapters use a separate key containing only inputs that can
  change their spans. This means progress visibility, mouse capture, and settings
  irrelevant to the active renderer no longer evict useful work. Line counts use
  an allocation-free render path, and whole-book progress reads prefix sums in
  constant time instead of summing every chapter on every frame. The parity test
  asserts that allocation-free counts equal full rendering across modes,
  languages, spacing policies, trailing gaps, and overlapping pattern indices.
  Changing only terminal height recomputes the cheap viewport counts in the
  existing allocation instead of rendering every chapter again. Explicit
  clearing is reserved for removing or replacing book content. State and
  executor tests assert height reuse, content-hash invalidation, and reuse across
  chapter navigation. On the 40-chapter benchmark, the replacement layout pass
  takes roughly 5.4 ms instead of the 49.6 ms needed to fully render and discard
  the same book's lines.
- **Plain rendering avoids per-character allocation.** Paragraphs without quote
  or dash dialogue take a direct wrapping path. Paragraphs with dialogue build
  one string per contiguous style run rather than one temporary string per
  character. On the committed 265,820-word benchmark corpus this reduced the
  median plain chapter render from 1.16 ms to roughly 0.5 ms, while render parity
  remained unchanged.
- **EPUB parsing walks borrowed DOM nodes.** The HTML adapter no longer clones
  every child handle, allocates every tag name, or builds raw and normalized text
  as separate trees of temporary strings. Container, package, and NCX parsing
  compares borrowed qualified-name bytes instead of allocating a lowercased
  `String` for every XML event. Cached archive blocks are borrowed and cloned
  only when a chapter actually takes ownership. The same corpus's large EPUB
  import moved from 25.25 ms to roughly 22 ms; import golden files remain
  byte-for-byte equal. Stable IDs also write hexadecimal digits directly into
  their final buffers instead of allocating once per digest byte.
- **PDF metadata and pages share one parsed document.** The importer previously
  loaded the same PDF once through `pdf-extract` for pages and again through
  `lopdf` for metadata. It now decrypts and reuses one document, while paragraph
  normalization and word counting run in one pass without intermediate joined
  vectors. Page/chapter/diagnostic behavior is unchanged and remains protected
  by fixture tests.
- **Storage reuses SQL plans and streams large chapter rows.** Repeated chapter,
  diagnostic, settings, and book statements use SQLite's prepared-statement
  cache. Loading a book deserializes each chapter as its row arrives instead of
  retaining every chapter JSON at once, and tag filtering uses a hash set rather
  than a quadratic vector scan. Position plus `last_opened_at` now commit in one
  atomic transaction instead of two autocommits. In the added 265,820-word book
  benchmark, median re-save moved from 4.93 ms to roughly 4.05 ms and reload from
  3.14 ms to roughly 2.82 ms.
- **Repaints borrow footer text and avoid unrelated work.** Non-navigation input
  no longer asks layout metrics for scroll bounds. Percentage footers calculate
  only the requested scope; time footers keep word totals as scalars instead of
  allocating a shadow chapter vector. Status, command input, and timer-only text
  are borrowed by Ratatui when they fit unchanged.
- **Timestamps and terminal geometry are parameters.** Storage writes and command
  execution take `now` and the viewport rather than reading the clock or
  `process.stdout`, which is what makes the executor contract deterministic.
- **PDF text extraction uses `pdf-extract` instead of pdf.js.** Both decode
  content streams rather than performing OCR, but paragraph breaks on complex
  layouts can differ. This is a dependency change with a visible edge, recorded
  here rather than in the parity matrix.

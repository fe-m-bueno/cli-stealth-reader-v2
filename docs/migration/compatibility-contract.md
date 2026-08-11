# v1 observable compatibility contract

The TypeScript test suite is the behavioral source of truth for the Rust migration. Internal helper names and module boundaries are not contractual.

The contract is pinned to [`fe-m-bueno/cli-stealth-reader` at `2b89907546e740525360f65b6a500287760029ba`](https://github.com/fe-m-bueno/cli-stealth-reader/tree/2b89907546e740525360f65b6a500287760029ba). Test paths below are relative to that revision. Moving the pin requires an explicit parity review.

## Parity matrix

| Seam | Observable contract | Primary v1 evidence | Rust target |
| --- | --- | --- | --- |
| Canonical model | Stable books, chapters, blocks, diagnostics, word counts, source paths, and import hashes | `test/epub.test.ts`, `test/html.test.ts`, `test/library-cbz-pdf.test.ts` | `reader-core`, `reader-formats` |
| EPUB | Container/OPF validation, EPUB3 nav with NCX and spine fallbacks, fragment handling, deterministic canonical blocks | `test/epub.test.ts`, `test/html.test.ts` | `reader-formats` fixture tests |
| CBZ | Numeric image ordering, one chapter per image, ignored non-images, diagnostics for empty/invalid archives | `test/library-cbz-pdf.test.ts` | `reader-formats` fixture tests |
| PDF | Extractable page text, image-only placeholders, metadata fallback, diagnostic behavior | `test/library-cbz-pdf.test.ts` | `reader-formats` fixture tests |
| Storage | Existing XDG paths and SQLite data, atomic settings, positions, pace, bookmarks, notes, tags, redaction, cascaded removal | `test/storage.test.ts`, `test/io-performance.test.ts`, `test/library-tags-notes.test.ts` | `reader-storage` migration and integration tests |
| Export/import | JSON version 1, import-hash identity, additive merge/deduplication, timestamp conflict rules | `test/library-export.test.ts` | shared JSON fixtures across v1/v2 |
| Commands | Quoted tokenization, flags and aliases, contextual completion/help, status/error behavior | `test/commands.test.ts`, `test/executor-contract.test.ts` | `reader-core` table tests |
| Navigation | Bounded chapter movement, history, goto percentages, focus behavior, resume/removal edge cases | `test/executor-contract.test.ts` | deterministic state-transition tests |
| Rendering | Plain dialogue highlighting; deterministic TypeScript/Python/Rust disguises; width bounds, escaping, block semantics | `test/renderers.test.ts`, `test/code-renderers-contract.test.ts` | golden text and property tests |
| Screen/TUI | Incremental repaint, resize invalidation, modal geometry, scrolling, progress, focus, mouse hit testing | `test/screen.test.ts`, `test/modal.test.ts`, `test/overlay-modals.test.ts`, `test/library-modal.test.ts` | Ratatui `TestBackend` buffer tests |
| Settings lifecycle | Open via command/shortcut, draft-only live preview, save/persist, and cancel behavior | `test/file-picker.test.ts`, `test/storage.test.ts` | application state tests plus Ratatui buffer tests |
| Shortcuts | Grouping, collapse, search, scrolling, footer state, keyboard protocols, and mouse hit testing | `test/shortcuts-panel.test.ts`, `test/file-picker.test.ts`, `test/screen.test.ts` | input-state and Ratatui buffer tests |
| Library UX | Recursive discovery, fuzzy filtering, sorting, file selection, tags, notes, and bookmarks | `test/file-picker.test.ts`, `test/fuzzy.test.ts`, `test/library-modal.test.ts`, `test/library-sorting.test.ts`, `test/library-tags-notes.test.ts`, `test/overlay-modals.test.ts` | application and TUI tests |
| Reading pace | Idle/outlier bounds, weighted pace, estimates, cross-book persistence | `test/reading-pace.test.ts`, `test/reading-pace-integration.test.ts` | pure `reader-core` and storage integration tests |
| Hot-path I/O | Prepared-statement reuse, leading/trailing write throttling, forced flushes, and cache-write avoidance | `test/io-performance.test.ts` | storage instrumentation and deterministic clock tests |
| Render cache | Memoized chapter/focus layout, invalidation inputs, resize repaint, and changed-line-only output | `test/render-cache.test.ts` | render-state tests and buffer diffs |
| Command history | Persistence plus current and legacy Toggl-token redaction | `test/storage.test.ts` | storage migration and redaction tests |
| Layout rendering | Compact/normal/relaxed spacing, wrapped-line spacing, margins, width, and font-scale behavior | `test/renderers.test.ts`, `test/screen.test.ts` | golden text and Ratatui buffer tests |
| Toggl | Bearer authentication, organization setup, paging, project resolution, quota handling, timer race safety | `test/toggl.test.ts` | mock-server adapter tests |
| Locale/themes | English UI collation, Brazilian Portuguese relative time, stable palette variants | `test/locale.test.ts`, `test/themes.test.ts` | snapshot/value tests |
| Startup CLI | `--resume` intent and explicit continue-reading startup | `src/index.ts`, `test/tui-startup.test.ts` | CLI parse and startup-state tests |

## Parity status

| Seam | Status | Evidence in v2 |
| --- | --- | --- |
| Canonical model | Done | `reader-core` unit tests, `reader-formats/tests/import_parity.rs` |
| EPUB | Done | 8 committed fixtures compared against v1's canonical books |
| CBZ | Done | 2 committed fixtures compared against v1's canonical books |
| PDF | Done, with a documented extractor change | `reader-formats/src/pdf.rs` tests |
| Storage | Done | `reader-storage/tests/v1_compatibility.rs` opens a v1 `library.db` |
| Export/import | Done | v1 export reproduced from the v1 database; merge rules unit-tested |
| Commands | Done | `reader-core/tests/command_parity.rs`, 263 v1 cases |
| Navigation | Done | `reader-app/tests/executor_contract.rs` |
| Rendering | Done | `reader-core/tests/render_parity.rs`, 12,050 v1 cases |
| Reading pace | Done | `reader-core/src/pace.rs` tests, executor pace tests |
| Command history | Done | redaction on write and on open, including legacy rows |
| Locale/themes | Done | `reader-core/src/locale.rs`, `theme.rs` tests |
| Screen/TUI | Done | `reader-tui` frame and session tests over a `TestBackend` |
| Settings lifecycle | Done | `reader-app/src/settings_panel.rs`, tab and cancel session tests |
| Shortcuts | Done | `reader-app/src/shortcuts_panel.rs`, folding and search tests |
| Library UX | Done | `reader-app/src/overlay.rs`, fuzzy filtering over every overlay |
| Hot-path I/O | Done | `reader-core/src/throttle.rs`, 1.5s coalescing driven by the loop |
| Render cache | Partial | chapter line-count cache done; changed-line-only output is not |
| Layout rendering | Done | geometry and frame composition, asserted from buffers |
| Toggl | Done | `reader-integrations`, 46 tests against recorded responses |
| Startup CLI | Done | `--resume`, an optional file, and the argument tests |

### What the remaining partial is

- **Changed-line-only repaint.** Ratatui already diffs its buffer, so the
  observable behavior matches; v1's explicit line cache has no v2 equivalent.

### Refinements closed since the first pass

The four seams that were partial are now complete, and each one came out a little
better than v1 rather than merely equal:

- **Settings live preview.** v1 previewed a draft and discarded it on cancel; v2
  does the same, backing the draft with `settings_backup` so `Esc` restores
  exactly what was on screen before the panel opened, and `Enter` is the only
  point the database hears about it.
- **Shortcut panel collapsing and search.** Categories fold with `z`, and the
  same `/` filter as every other overlay applies here, matching a binding by its
  keys or its description.
- **Fuzzy filtering inside overlays.** Every overlay is now built from one
  `OverlayEntry` list, so `/` narrows chapters, books, bookmarks, notes, and
  shortcuts alike — and confirming acts on the visible row, which the previous
  index-based lookup could get wrong once a list was filtered.
- **Write throttling.** `WriteThrottle` coalesces on the same 1.5-second window
  as v1, but with the leading edge kept: the first change of a burst is written
  immediately, so a crash mid-scroll loses at most one window rather than the
  whole session. The clock is a parameter, so the whole thing is unit-tested
  without sleeping.

## Defects found in v1 and fixed here

Auditing v1 for parity surfaced two defects. Both are fixed rather than
reproduced, and both are documented with their migration and their guard tests in
[the improvements record](improvements.md):

- Two books whose chapter hrefs collided could not both be stored, because
  `chapters.id` was a global primary key derived without the book. The key is now
  per book, and opening a v1 database migrates it without losing rows.
- A list item lost its indent when wrapped, since the bullet was folded into the
  text before the word split. The marker is now applied per line, with
  continuation lines hanging under the text.

The render parity suite names the cases where this divergence is expected and
asserts both the v1 and v2 shapes, so the improvement cannot silently regress and
cannot silently spread.

## Acceptance rule

A seam reaches parity when its Rust tests cover the observable cases above against shared fixtures or equivalent golden outputs. The complete transition requires every seam in this matrix and a reversible data migration. Performance is a separate pending gate: [Measure the v1 performance baseline](https://github.com/fe-m-bueno/cli-stealth-reader/issues/32) defines the procedure and raw baseline, then [Set measurable v2 performance acceptance budgets](https://github.com/fe-m-bueno/cli-stealth-reader/issues/30) records the numeric thresholds. Files in `docs/plans/` that are not implemented in v1 remain roadmap items rather than parity requirements.

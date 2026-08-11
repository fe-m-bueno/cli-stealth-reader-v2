# v1 observable compatibility contract

The TypeScript test suite is the behavioral source of truth for the Rust migration. Internal helper names and module boundaries are not contractual.

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
| Screen/TUI | Incremental repaint, resize invalidation, modal geometry, scrolling, progress, focus, mouse hit testing | `test/screen.test.ts`, modal/panel tests | Ratatui `TestBackend` buffer tests |
| Library UX | Recursive discovery, fuzzy filtering, sorting, file selection, tags/notes/bookmarks | library and overlay test files | application and TUI tests |
| Reading pace | Idle/outlier bounds, weighted pace, estimates, cross-book persistence | reading-pace test files | pure `reader-core` tests |
| Toggl | Bearer authentication, organization setup, paging, project resolution, quota handling, timer race safety | `test/toggl.test.ts` | mock-server adapter tests |
| Locale/themes | English UI collation, Brazilian Portuguese relative time, stable palette variants | `test/locale.test.ts`, `test/themes.test.ts` | snapshot/value tests |
| Startup CLI | `--resume` intent and explicit continue-reading startup | `src/index.ts`, `test/tui-startup.test.ts` | CLI parse and startup-state tests |

## Acceptance rule

A seam reaches parity when its Rust tests cover the observable cases above against shared fixtures or equivalent golden outputs. The complete transition requires every seam in this matrix, a reversible data migration, and passing performance budgets. Files in `docs/plans/` that are not implemented in v1 remain roadmap items rather than parity requirements.

# Rust migration architecture

## Direction

The v2 repository is a Cargo workspace. The executable is a thin composition root; observable reading behavior belongs to testable libraries. Crate boundaries follow capabilities with distinct dependencies and failure modes, not every TypeScript source file.

The intended dependency flow is:

```text
stealth-reader -> reader-tui -> reader-app -> reader-core
                              -> reader-formats -> reader-core
                              -> reader-storage -> reader-core
                              -> reader-integrations -> reader-core
```

Dependencies must not point back toward the composition root or TUI. `reader-core` stays synchronous and independent from terminal, database, archive, PDF, and HTTP libraries.

## Crates

- `reader-core`: canonical books, settings, reading positions, commands, state transitions, render inputs, reading pace, and other deterministic domain behavior.
- `reader-app`: use cases that coordinate ports for storage, parsing, clocks, and integrations.
- `reader-formats`: EPUB, CBZ, PDF, HTML, and XML adapters that produce canonical books.
- `reader-storage`: XDG paths, SQLite schema compatibility, migrations, and import/export.
- `reader-integrations`: Toggl Focus API adapter and quota cache.
- `reader-tui`: Ratatui widgets, terminal lifecycle, input mapping, overlays, and screen-buffer tests.
- `stealth-reader`: argument parsing, dependency construction, and process exit behavior.

Crates are introduced only when a working vertical slice needs them. This keeps the workspace explicit without committing empty abstractions.

## Engineering rules

- Forbid unsafe Rust workspace-wide unless a future ADR documents a narrow exception.
- Library crates expose typed errors; the binary adds user-facing context and owns exit codes.
- Keep domain updates separate from rendering. The TUI renders shared references to state and handles input outside the render pass.
- Keep `Cargo.lock` committed and toolchain/lints centralized at the workspace root.
- Test the smallest affected crate during development; run formatting, Clippy with warnings denied, and all workspace tests before every migration milestone.
- Port behavioral contracts and fixtures, not TypeScript helper structure.
- Measure startup, import, render, and memory before claiming a performance improvement.

## Compatibility and release

The v1 database and files remain untouched during beta. The Rust binary initially runs side by side and does not assume the `stealth-reader` command until parity, migration safety, and performance gates pass. Linux and macOS are the first release targets.

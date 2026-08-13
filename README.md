# stealth-reader

A full-screen book reader for the terminal, written in Rust. `stealth-reader`
opens EPUB, CBZ, and PDF files and offers two reading styles:

- **Plain**: clean prose, with headings, quotes, lists, and scene breaks
  formatted for reading;
- **Stealth**: the same content disguised as plausible TypeScript, Python, or
  Rust code.

This is the native implementation that replaces the earlier project, referred to
here as [`stealth-reader-v0`](https://github.com/fe-m-bueno/cli-stealth-reader).
It keeps the v0 database format: positions, bookmarks, notes, tags, and settings
remain available without a manual migration.

![Stealth mode in TypeScript](docs/screenshots/stealth-code-mode.png)

## Features

- full-screen TUI reading, with Ratatui and mouse support;
- EPUB3 import, with fallbacks to NCX and spine order;
- CBZ and PDF support, including diagnostics for pages without text;
- recursive discovery of `.epub`, `.cbz`, and `.pdf` files;
- plain mode with optional dialogue highlighting;
- TypeScript, Python, and Rust stealth modes;
- stealth code density configurable from 1 to 5;
- focus mode, which centers a single reading block;
- search within the current chapter or the whole book;
- chapter navigation, history, table of contents, and progress;
- bookmarks, notes, and tags per book;
- a persistent SQLite library, sortable by title, author, progress, or last
  opened;
- reading state export and import as JSON;
- five color schemes and six appearance variants;
- a learned reading pace and time-remaining estimate;
- optional Toggl Track Focus integration;
- a command and shortcut manual available inside the reader itself.

## Installation

### Requirements

- Linux or macOS;
- [Rust](https://www.rust-lang.org/tools/install) 1.85 or later;
- `rustup` is recommended. The repository pins the `1.97.1` toolchain in
  [`rust-toolchain.toml`](rust-toolchain.toml), which `rustup` installs and uses
  automatically;
- an interactive terminal with Unicode and ANSI color support. The RGB themes
  work best in terminals with 24-bit color support.

Normal use does not require Node.js. Node.js 20+ is only needed to regenerate
parity fixtures or to run the benchmarks that compare against
`stealth-reader-v0`.

### Installing from Source

```bash
git clone https://github.com/fe-m-bueno/cli-stealth-reader-v2.git stealth-reader
cd stealth-reader
cargo install --path crates/stealth-reader --locked
stealth-reader --version
```

`cargo install` builds and installs the `stealth-reader` binary into
`~/.cargo/bin`. After that, normal use does not involve Cargo:

```bash
stealth-reader
stealth-reader --resume
stealth-reader ./books/dune.epub
```

If `~/.cargo/bin` is not on your `PATH` yet, add it to your shell before opening
a new terminal.

### Installing the Release Binary

Once a release has been published, the recommended installation downloads the
prebuilt binary for your platform, validates the checksum, and requires no Rust:

```bash
curl -fsSL https://raw.githubusercontent.com/fe-m-bueno/cli-stealth-reader-v2/main/install.sh | bash
```

By default the binary lands in `~/.local/bin`. To choose another directory:

```bash
curl -fsSL https://raw.githubusercontent.com/fe-m-bueno/cli-stealth-reader-v2/main/install.sh \
  | STEALTH_READER_INSTALL_DIR="$HOME/bin" bash
```

### Publishing a Release

When you create and push a tag matching `v*`, GitHub Actions builds and
publishes artifacts for Linux x86_64, macOS Intel, and macOS Apple Silicon:

```bash
git tag v0.1.0
git push origin v0.1.0
```

Each release includes one `.tar.gz` per platform and its `.sha256` file.

## Quick Start

```bash
stealth-reader                         # opens the library
stealth-reader --resume                # resumes the most recently opened book
stealth-reader ./books/dune.epub       # imports and opens a file
stealth-reader ./comics/comic.cbz
stealth-reader ./article.pdf
stealth-reader update                  # updates to the latest release
```

The binary's global options are also available:

```text
stealth-reader --help
stealth-reader --version
stealth-reader --resume
stealth-reader [--resume] <FILE>
```

If a file is given alongside `--resume`, the explicit file takes precedence.

The `stealth-reader update` command (or `stealth-reader upgrade`) downloads the
latest release artifact, verifies the SHA-256, and replaces the installed
binary.

### What Happens on Startup

With no arguments, the reader **never reopens a book on its own** — it offers a
choice. In order:

1. a file passed on the command line is imported and opened;
2. with `--resume`, the most recent book reopens at its saved position;
3. otherwise, the directory configured via `/librarydir` is scanned
   recursively, and:
   - if the library already has books, the library picker opens so you can
     choose;
   - if it is empty but there are EPUB/CBZ/PDF files in the directory, the file
     picker opens with the files it found;
   - if there is nothing, the footer explains how to point at another directory.

Automatically opening the last book is **opt-in** via `--resume` rather than the
default: a session that starts in the wrong place costs more than one extra
keystroke.

### First Session

1. Start `stealth-reader`.
2. Press `/` and run `/librarydir ~/Books` to choose your library root, or use
   `/add /path/to/book.epub` to import a file directly.
3. With no path, `/add` opens a recursive picker starting from the configured
   root.
4. Use `j`/`k`, the arrow keys, `Space`/`b`, or the mouse wheel to navigate.
5. Press `m` to switch between plain, TypeScript, Python, and Rust.
6. Press `f` to enable focus mode.
7. Press `?` to look up the shortcuts.
8. Press `q` to quit. Your position is saved automatically.

The `/help` command opens the full manual inside the reader. `/help <command>`
shows the reference for a specific command.

## Supported Formats

### EPUB

The importer validates the container, reads the OPF, and resolves the table of
contents in this order:

1. EPUB3 `nav.xhtml`;
2. NCX;
3. spine order, when there is no usable navigation.

Anchor fragments, chapters sharing files, front matter, and headings marked up
only as paragraphs are all handled during normalization. Recoverable problems
are recorded as diagnostics without preventing the rest of the book from
opening.

### CBZ

Each image in the CBZ file becomes a navigable page. The reader does not do OCR:
pages are displayed as image placeholders and the book gets a diagnostic warning
that no text is available for the reading modes.

### PDF

Each page becomes a chapter. The importer extracts text from the content streams
and separates paragraphs on blank lines; it does not do OCR. Image-only pages
get a placeholder and a diagnostic rather than disappearing.

## Reading Modes

### Plain

Plain mode prioritizes readability:

```text
CHAPTER 1 — DOWN THE RABBIT-HOLE

Alice was beginning to get very tired of sitting by her sister on the
bank, and of having nothing to do.

▏ The day was very hot and made her feel sleepy. Alice began to feel
▏ very sleepy and stupid.

· · · · · · ·
```

Headings are emphasized, quotes use a sidebar, lists preserve their indentation,
and scene breaks are visually separated. Dialogue highlighting can be turned on
or off with `/highlight on` and `/highlight off`.

### Stealth

The text is reformatted as code in a chosen language. The `m` key's sequence is:

```text
plain → typescript → python → rust → plain
```

Use `/mode typescript`, `/mode python`, `/mode rust`, or `/mode plain` to select
one directly. The choice is saved in the settings.

Density controls how much synthetic structure appears in the code:

```text
/density 1    # more comments and explanatory text
/density 3    # the default balance
/density 5    # more code and fewer comments
```

`d` cycles quickly between 1, 3, and 5 while a stealth mode is active.

## Keyboard Shortcuts

The shortcuts depend on the current screen. In command mode, letters are typed
into the text; in overlays, `j` and `k` move the selection.

### Navigation

| Key | Action |
| --- | --- |
| `j` / `↑` | Scroll up |
| `k` / `↓` | Scroll down |
| `Space` / `PgDn` | Page forward |
| `b` / `PgUp` | Page back |
| `Home` | Go to the start of the chapter |
| `End` | Go to the end of the chapter |
| `←` / `→` | Previous / next chapter |
| `Shift+T` | Open the table of contents |
| `Shift+B` | Open bookmarks |
| `[` / `]` | Go back / forward in navigation history |
| `wheel` | Scroll the page |
| `g` | Go to the top |
| `Shift+G` | Go to the end |

### Commands and Overlays

| Key | Action |
| --- | --- |
| `/` | Focus the command bar |
| `Enter` | Run a command or confirm the selected item |
| `Tab` | Complete a command or cycle suggestions |
| `Esc` | Close an overlay, cancel a search, or unfocus the bar |
| `n` / `Shift+N` | Next / previous search result |
| `d` | Delete the selected bookmark or note |
| `s` (library) | Change the sort criterion |
| `r` (library) | Reverse the sort direction |
| `?` / `Ctrl+.` / `Ctrl+X` | Open shortcuts |
| `Ctrl+C` | Quit the reader |

### View

| Key | Action |
| --- | --- |
| `m` | Switch rendering mode |
| `f` | Toggle focus mode |
| `c` | Open the color scheme picker |
| `Shift+C` | Open the theme picker |
| `Shift+S` | Open settings |
| `p` | Change the progress display |
| `q` | Quit the reader or close an overlay |

In the shortcuts panel, `z` collapses or expands every group. In the settings
panel, `←`/`h` and `→`/`l` switch tabs and `Space` changes the current field.

## Slash Commands

Press `/` to open the bar. Commands can be typed with or without the leading
slash once the bar is active; arguments containing spaces must use single or
double quotes. `Tab` completes names and flags.

### Navigation

```text
/prev [count]                          previous chapter
/next [count]                          next chapter
/chapters [query] [--current] [--flat] table of contents
/goto <n|%> [--chapter]                position by chapter or percentage
/search [-g|--global] <term>           search the chapter or the whole book
```

Examples:

```text
/prev 2
/chapters introduction --current
/goto 5
/goto 42%
/goto 30% --chapter
/goto 30%c
/search "chapter one"
/search -g mordor
```

By default, `/search` searches only the current chapter. Use `-g` or `--global`
to search the whole book, then `n`/`Shift+N` to step through the results.

### Library and Books

```text
/changebook [query] [--recent] [--cwd] [--sort <key>]
/book [query] [--recent] [--cwd] [--sort <key>]
/resume [book-query] [--latest]
/add [path] [--cwd] [--force]
/librarydir [path] [--cwd]
/bookdir [path] [--cwd]
/remove [book-query] [--current]
/removecurrent [--confirm]
```

`/book` is an alias for `/changebook` and `/bookdir` is an alias for
`/librarydir`. `--sort` accepts `lastOpened`, `title`, `author`, or `progress`.

```text
/changebook dune
/changebook --recent
/changebook --sort progress
/resume --latest
/add ./books/example.epub
/add ./comics/comic.cbz --force
/add --cwd
/librarydir ~/Books
/librarydir --cwd
/remove dune
/remove --current
/removecurrent --confirm
```

`/remove` only removes the book from the local library; it never deletes the
original file on disk.

### Appearance and Reading

```text
/mode [plain|typescript|python|rust]
/density [1-5]
/highlight <on|off>
/toggleprogress [time-chapter|time-book|book|both|chapter|hidden]
/colorscheme [scheme] [--preview] [--list]
/theme [theme] [--list]
/mouse [on|off]
/settings
```

#### Mouse and Text Selection

Mouse capture is **off** by default, and that is deliberate: with it off the
terminal still owns the pointer, so dragging the cursor over the text selects
and copies it the way it does for any other terminal output. Only the wheel
reaches the reader, and it scrolls the page normally.

With `/mouse on`, the reader starts receiving clicks and drags:

- clicking the scrollbar track jumps to the corresponding point in the chapter,
  and dragging the bar's cursor moves the reading position continuously;
- clicking a row in an overlay moves the selection to it; in the shortcut
  panel's groups, clicking the header folds or unfolds the group;
- clicking `[×]` closes the modal and clicking the search line starts filtering.

Any drag outside the scrollbar still belongs to the terminal. On terminals that
reserve a plain drag for the application (most of them), native selection
remains available with **Shift+drag** while capture is on; where the terminal
does not offer that shortcut, `/mouse off` gives normal selection back.

Color schemes:

```text
codex     claude     graphite     amber     forest
```

Appearance themes:

```text
dark     light     dark-colorblind     light-colorblind
dark-ansi     light-ansi
```

`/colorscheme` and `/theme` with no argument open their respective pickers. The
`--preview` flag on `/colorscheme` is accepted for compatibility; `--list` shows
the full list.

Progress can show an estimated time or percentages. The `p` key's cycle order
is:

```text
time-chapter → time-book → book → both → chapter → hidden
```

`/settings` opens a panel with a transactional preview and four tabs: `Themes`,
`Reading`, `Layout`, and `More`. `Enter` saves; `Esc` cancels and restores the
previous state. The options include text scale, margins, spacing, dialogue
highlighting, and mouse capture.

### Bookmarks, Notes, and Tags

```text
/mark [label]
/marks
/delmark <id|label>

/note [text]
/note -l
/note -d <id>

/tag [tag]
/tag -d <tag>
/tags
```

Examples:

```text
/mark "come back to this passage"
/marks
/delmark "come back to this passage"
/note "check this quote"
/note -l
/tag favorite
/tag -d favorite
/tags
```

Bookmarks and notes can be selected in the overlays and opened with `Enter`.
Inside them, `d` removes the selected item.

### Export and Import

```text
/export [path]
/import [path]
```

`/export` saves positions, bookmarks, notes, and tags as JSON. With no path, it
uses the application's default file. `/import` merges the exported file; book
identity uses the content hash rather than the absolute path, which makes the
file suitable for syncing across machines.

### Toggl Track

The integration is optional and uses the Toggl Track 2.0 Focus API:

```text
/toggl auth
/toggl auth <toggl_sk_...>
/toggl setup
/toggl sync
/toggl recent
/toggl start "Book" --project "Reading books"
/toggl stop
/toggl log "Book" --duration 45m --project "Reading books"
/toggl --disconnect
/toggl auth --open
```

Durations accept formats like `25m`, `1.5h`, and `900s`. The token is stored in
the local settings database; command history replaces credentials with
`<redacted>`.

### Help

```text
/help
/help mode
/help --all
/keyboardshortcuts
/keys
/keys --category navigation
/keyboardshortcuts --category commands
```

`/keys` is an alias for `/keyboardshortcuts`. The accepted categories are
`navigation`, `commands`, and `view`.

## Data and v0 Compatibility

The current command is `stealth-reader`, but the data directory is still named
`cli-stealth-reader` on purpose. That is the contract that lets v2 open the same
database as [`stealth-reader-v0`](https://github.com/fe-m-bueno/cli-stealth-reader)
without copying or converting the library.

| Data | Default path |
| --- | --- |
| SQLite database | `~/.local/share/cli-stealth-reader/library.db` |
| Chapter cache | `~/.cache/cli-stealth-reader/books/` |

When set, `XDG_DATA_HOME` and `XDG_CACHE_HOME` override the respective default
directories:

```text
$XDG_DATA_HOME/cli-stealth-reader/library.db
$XDG_CACHE_HOME/cli-stealth-reader/books/
```

The cache is rebuildable. The database holds settings, books, chapters,
positions, bookmarks, notes, tags, diagnostics, and command history.

### Using an Existing Library

If the old checkout was renamed to `~/Development/stealth-reader-v0`, usage is
straightforward:

```bash
stealth-reader
```

To take a backup before the first run:

```bash
cp ~/.local/share/cli-stealth-reader/library.db ~/library.db.backup
```

v2 opens the database in place and applies only compatible, idempotent changes
(indexes and the fix to the composite chapter key). v0 remains able to open the
database. `/export` and `/import` are the recommended way to move state between
machines.

There is no need to re-import books that are already present. Use `/add --force`
only when you want to reprocess a file or when the parser has been updated.

## Code Architecture

The workspace is split by responsibility:

```text
stealth-reader
└── reader-tui             terminal, Ratatui, input, and overlays
    └── reader-app         state, layout, and command execution
        ├── reader-core    domain, rendering, themes, pace, and command parser
        ├── reader-formats EPUB, CBZ, PDF, HTML, XML, and file discovery
        ├── reader-storage SQLite, XDG paths, compatibility, and export/import
        └── reader-integrations  Toggl Track Focus integration
```

The `stealth-reader` binary is only the composition root: it reads arguments,
opens storage, assembles the initial state, and starts the TUI. `reader-core`
does not depend on the terminal, database, ZIP, PDF, or HTTP, which keeps the
core logic deterministic and testable.

## Development

### Local Checks

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

During development you can test a single crate:

```bash
cargo test -p reader-core
cargo test -p reader-formats
cargo test -p reader-storage
cargo test -p reader-tui
```

### Benchmarks

```bash
cargo build --release -p reader-bench -p stealth-reader
cargo run --release -p reader-bench -- --json
```

The comparison benchmarks require a build of `stealth-reader-v0` and Node.js:

```bash
cd ~/Development/stealth-reader-v0
npm install
npm run build

cd ~/Development/stealth-reader
V1_DIR=~/Development/stealth-reader-v0 node bench/v1-baseline.mjs --json
cargo run --release -p reader-bench -- --json
```

The name `V1_DIR` is kept by the comparison scripts for compatibility with the
migration's history; the path it points at is now the `stealth-reader-v0`
checkout. The results and the full procedure are in
[`docs/migration/performance-baseline.md`](docs/migration/performance-baseline.md).

### Parity Fixtures

The versioned fixtures make it possible to run the Rust suite without installing
Node.js. To regenerate them using v0:

```bash
V1_DIR=~/Development/stealth-reader-v0 node tools/generate-render-golden.mjs
V1_DIR=~/Development/stealth-reader-v0 node tools/generate-command-golden.mjs
V1_DIR=~/Development/stealth-reader-v0 node tools/generate-import-golden.mjs
V1_DIR=~/Development/stealth-reader-v0 node tools/generate-storage-fixture.mjs
```

Regeneration is deliberate and should be reviewed alongside the changes. The
main coverage is:

| Fixture | Coverage |
| --- | --- |
| `reader-core/tests/golden/render-parity.json` | rendering across modes, languages, densities, widths, and spacings |
| `reader-core/tests/golden/command-parity.json` | parsing, errors, suggestions, help, and aliases |
| `reader-formats/tests/golden/import-parity.json` | canonical import of EPUBs and CBZs |
| `reader-storage/tests/fixtures/v1-library.db` | field-by-field reading of a v0 database |

More details are in [`docs/migration/compatibility-contract.md`](docs/migration/compatibility-contract.md).

## Performance

On the reference corpus, measured on the same machine, the Rust implementation
substantially reduced the reader's cost:

| Measurement | v0 | `stealth-reader` |
| --- | ---: | ---: |
| Startup | 339 ms | 0.9 ms |
| Importing a 266k-word EPUB | 192 ms | 27 ms |
| Rendering a chapter in stealth mode | 5.0 ms | 1.8 ms |
| Peak memory | 157 MB | 15 MB |

The full numbers, corpus, and the comparison's limitations are in
[`docs/migration/performance-baseline.md`](docs/migration/performance-baseline.md).

## Further Documentation

- [Migration architecture](docs/migration/architecture.md)
- [Compatibility contract](docs/migration/compatibility-contract.md)
- [Data, backup, and rollback](docs/migration/data-migration.md)
- [Deliberate improvements over v0](docs/migration/improvements.md)
- [Performance baseline](docs/migration/performance-baseline.md)
- [How to test](docs/testing.md)
- [Architecture research](docs/research/codex-and-grok-build-architecture.md)

## Contributing

Contributions are welcome. Preserve the separation between domain, adapters,
storage, and terminal; add tests for behavior changes and run the workspace
checks before opening a pull request.

# Performance baseline and v2 budgets

Wayfinder context: [Measure the v1 performance baseline](https://github.com/fe-m-bueno/cli-stealth-reader/issues/32) and [Set measurable v2 performance acceptance budgets](https://github.com/fe-m-bueno/cli-stealth-reader/issues/30).

## Procedure

Both runtimes are measured against the same generated corpus so numbers are comparable across languages.

```bash
# v1 (requires `npm run build` in the v1 checkout)
V1_DIR=~/Development/stealth-reader-v0 node bench/v1-baseline.mjs --json

# v2
cargo run --release -p reader-bench -- --json
```

`bench/fixtures.mjs` writes the corpus to `$BENCH_FIXTURE_DIR` (default
`/tmp/stealth-reader-bench-fixtures`) and reuses it when the files already exist:

| Fixture | Shape |
| --- | --- |
| `small.epub` | 3 chapters, 12 paragraphs each |
| `large.epub` | 40 chapters, 120 blocks each, 265,820 words, 2.0 MB |
| `comic.cbz` | 200 image pages, 480 KB |
| `doc.pdf` | 50 text pages, 28 lines each, 148 KB |

Every measurement repeats and reports median with min/max. Startup is measured as
a cold process that loads the whole module graph, because a TUI process does not
exit on its own. Memory is the RSS of a process that imports `large.epub` and
renders every chapter in code mode.

## v1 raw results

Environment: Linux 7.1.5 x86_64, 11th Gen Intel Core i5-1135G7, Node v22.22.2.

| Measurement | Median | Min | Max |
| --- | --- | --- | --- |
| Cold process wall time (module graph) | 339.25 ms | 323.02 ms | 369.83 ms |
| Module graph load (in-process) | 226.43 ms | 217.28 ms | 247.07 ms |
| SQLite storage open | 0.51 ms | 0.38 ms | 5.29 ms |
| Library discovery (4 files) | 0.12 ms | 0.05 ms | 1.47 ms |
| Import `small.epub` | 5.45 ms | 4.11 ms | 36.93 ms |
| Import `large.epub` | 191.95 ms | 178.37 ms | 248.46 ms |
| Import `comic.cbz` | 4.90 ms | 3.42 ms | 6.20 ms |
| Import `doc.pdf` | 75.79 ms | 68.58 ms | 236.50 ms |
| Render one chapter, plain (142 blocks) | 4.53 ms | 3.48 ms | 21.08 ms |
| Render one chapter, code | 5.02 ms | 3.82 ms | 13.46 ms |
| Render all 40 chapters, plain | 191.46 ms | 173.93 ms | 197.45 ms |
| Peak RSS (import + render whole book) | 156.82 MB | — | — |
| Peak heap used | 27.91 MB | — | — |

## v2 acceptance budgets

The migration goal is at least a 100% improvement, so each budget is at most half
the v1 median. Budgets are medians on the same machine and corpus.

| Measurement | v1 median | v2 budget (≤) | Required gain |
| --- | --- | --- | --- |
| Cold process startup | 339.25 ms | 20 ms | 16.9× |
| Import `large.epub` | 191.95 ms | 95 ms | 2.0× |
| Import `doc.pdf` | 75.79 ms | 37 ms | 2.0× |
| Import `comic.cbz` | 4.90 ms | 2.45 ms | 2.0× |
| Import `small.epub` | 5.45 ms | 2.70 ms | 2.0× |
| Render one chapter, plain | 4.53 ms | 2.25 ms | 2.0× |
| Render one chapter, code | 5.02 ms | 2.50 ms | 2.0× |
| Render all chapters, plain | 191.46 ms | 95 ms | 2.0× |
| Peak RSS | 156.82 MB | 78 MB | 2.0× |

Startup gets a stricter absolute budget instead of a halving rule: a native
binary has no interpreter warm-up, so 20 ms is the realistic bar rather than
170 ms. Storage open and discovery are already sub-millisecond in v1 and are
tracked for regression only, with a budget of "no slower than v1".

A budget is met when the v2 harness reports a median at or below the value above
in three consecutive runs on an otherwise idle machine.

## v2 results

Same machine and corpus, release build, three consecutive runs. Every budget is
met, and every measurement improves on v1 by more than the 100% the migration set
out to achieve.

| Measurement | v1 median | Budget (≤) | Run 1 | Run 2 | Run 3 | Gain |
| --- | --- | --- | --- | --- | --- | --- |
| Cold process startup | 339.25 ms | 20 ms | 0.98 ms | 0.88 ms | 0.90 ms | 376× |
| Import `large.epub` | 191.95 ms | 95 ms | 27.52 ms | 27.92 ms | 26.82 ms | 7.0× |
| Import `doc.pdf` | 75.79 ms | 37 ms | 11.76 ms | 12.82 ms | 11.88 ms | 6.2× |
| Import `comic.cbz` | 4.90 ms | 2.45 ms | 0.56 ms | 0.70 ms | 0.59 ms | 8.1× |
| Import `small.epub` | 5.45 ms | 2.70 ms | 0.28 ms | 0.27 ms | 0.28 ms | 19.8× |
| Render one chapter, plain | 4.53 ms | 2.25 ms | 1.18 ms | 1.25 ms | 1.22 ms | 3.7× |
| Render one chapter, code | 5.02 ms | 2.50 ms | 1.72 ms | 1.87 ms | 1.86 ms | 2.8× |
| Render all 40 chapters, plain | 191.46 ms | 95 ms | 49.60 ms | 50.15 ms | 48.89 ms | 3.9× |
| Peak RSS | 156.82 MB | 78 MB | 15.00 MB | 15.19 MB | 14.92 MB | 10.4× |
| SQLite storage open | 0.51 ms | 0.51 ms | 0.26 ms | 0.21 ms | 0.22 ms | 2.3× |

**Verdict: every budget met.** The weakest result is the code disguise at 2.8×,
which is the most allocation-heavy path in the reader — every line becomes a
handful of styled spans. Two changes brought it from 1.7× to 2.8×: word
extraction borrows from the line instead of allocating a `String` per word, and
the line builder moves its spans out instead of cloning them.

Startup is the number a reader actually feels, and it is the one that changed
character rather than degree: 339 ms of Node module loading became a 1 ms
process start, so the reader is on screen before the terminal has finished
drawing its own frame.

## Reproducing

```bash
# 1. Build both sides.
(cd ~/Development/stealth-reader-v0 && npm run build)
cargo build --release -p reader-bench -p stealth-reader

# 2. Measure. The first run generates the shared corpus; later runs reuse it.
V1_DIR=~/Development/stealth-reader-v0 node bench/v1-baseline.mjs --json
./target/release/reader-bench
```

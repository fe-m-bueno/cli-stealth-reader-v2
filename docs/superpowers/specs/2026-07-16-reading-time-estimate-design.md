# Design: Estimated Reading Time (Kindle-like)

**Date:** 2026-07-16  
**Status:** Approved for planning  
**Repo:** cli-stealth-reader  
**Primary focus:** Time-left estimates driven by learned reading pace

## Problem

The reader already shows positional progress (book/chapter %) via view offsets and word counts per chapter. It does **not** estimate how long remaining reading will take. Kindle’s “Time left in chapter / book” is driven by observed reading speed; that is the experience we want.

## Goals

1. Estimate **time left in chapter** and **time left in book** from words remaining ÷ effective WPM.
2. Learn WPM from **active reading time** and **words advanced** (Kindle-like), not from a fixed manual slider (v1).
3. Persist pace so estimates improve across sessions.
4. Expose display modes via the existing progress cycle (`p` / `/toggleprogress` / settings), Kindle-style one-at-a-time.

## Non-goals (v1)

- Location / page-in-book metrics
- Manual WPM setting in the settings panel
- “Calibrating…” label (cold start uses default WPM silently)
- Raw sample history table (only aggregated pace state)
- Syncing pace from Toggl (Toggl remains an external timer only)
- Full Kindle-like settings chrome (tabbed Themes / Font / Layout / More, font family, margins, spacing, Word Wise, etc.) — **deferred follow-up** after this feature ships; only extend the existing progress cycle / settings item as needed for time modes

## Product decisions

| Topic | Decision |
|-------|----------|
| Pace measurement | Active time + forward words advanced |
| Pace scope | Hybrid: global base + per-book blend |
| Cold start | Default **230 WPM** until ~3–5 min global active reading |
| Idle | Cap active time at **2 minutes** without input |
| Display | Cycle modes (one primary progress line at a time) |
| Modes in v1 | `time-chapter`, `time-book`, existing `%` modes, `hidden` |

## Architecture

```
input / navigation
        │
        ▼
 reading-pace tracker (AppState memory)
        │  samples: wordsAdvanced, activeMs
        ▼
 src/reading-pace.ts  (pure functions)
        │  updateWpm, blend, estimate, formatDuration
        ▼
 storage (settings + reading_pace table)
        │
        ▼
 formatProgress / footer  (screen.ts)
```

### Module: `src/reading-pace.ts`

Pure, unit-tested logic. No TUI imports.

Responsibilities:

- Accept pace state + a sample → new pace state
- Blend global vs book WPM
- Compute remaining words and time estimates
- Format durations for the footer
- Filter invalid samples / outlier instantaneous WPM

### Integration points

| Area | Change |
|------|--------|
| `types.ts` | Extend `ProgressVisibility`; pace-related types |
| `storage.ts` | Settings keys; `reading_pace` table; get/set helpers |
| `tui.ts` / `AppState` | In-memory tracker; load/save pace with book |
| Input / navigation path | On forward position change → sample |
| `screen.ts` | `formatProgress` time modes |
| `commands.ts` / `executor.ts` | `/toggleprogress` cycle includes new modes |
| `settings-panel.ts` | Progress display labels for new modes |
| `help.ts` | Optional short description update |

## Algorithm

### Core formula

```
remainingMinutes = remainingWords / effectiveWpm
```

### Word position

Reuse existing chapter progress semantics:

```
chapterProgress = blockOffset / chapterMaxOffset   // same as today
wordsReadInChapter ≈ chapterProgress × chapter.wordCount
wordsRemainingInChapter = max(0, wordCount − wordsReadInChapter)
wordsRemainingInBook = wordsRemainingInChapter + sum(wordCount of later chapters)
```

### Sample (on forward navigation)

When chapter index and/or block offset advances:

1. Compute absolute word cursor for previous and current positions.
2. `wordsAdvanced = max(0, currentWords − previousWords)`.
3. `elapsedMs = now − lastSampleAt`.
4. `activeMs = min(elapsedMs, IDLE_MS)` where `IDLE_MS = 2 * 60 * 1000`.
5. If `wordsAdvanced == 0` or `activeMs == 0` → no pace update (still update last position / timestamp as appropriate for idle handling).
6. Instantaneous WPM = `wordsAdvanced / (activeMs / 60000)`.
7. If instantaneous WPM &lt; 50 or &gt; 800 → discard sample for pace update.
8. Else update EWMA (or mass-weighted average by `activeMs`) for **global** and **current book**.

Backward scroll / re-read: do not produce negative samples; only forward word gains count.

### Effective WPM (hybrid)

```
DEFAULT_WPM = 230
COLD_START_MS ≈ 3–5 minutes global active

if globalActiveMs < COLD_START_MS:
  // Linear blend from DEFAULT toward measured global as cold-start fills
  t = globalActiveMs / COLD_START_MS
  base = (1 − t) * DEFAULT_WPM + t * globalWpm
else:
  base = globalWpm

bookWeight = clamp(bookActiveMs / BOOK_BLEND_MS, 0, 1)
effectiveWpm = (1 − bookWeight) * base + bookWeight * bookWpm
```

Constants are defined once in `reading-pace.ts` and pinned by unit tests.

### Idle

- No key/mouse/scroll for **≥ 2 minutes** → subsequent sample’s `activeMs` is capped at 2 minutes (time while idle does not inflate denominator).
- While overlays that block reading (e.g. settings, help) are open → do not emit reading samples.

### Book switch

- Flush/persist global + previous book pace.
- Load target book’s `reading_pace` row (or defaults).
- Reset in-memory last word cursor to the restored position (no artificial huge sample).

### Zero word count

For chapters/books with `wordCount === 0` (e.g. CBZ/image-heavy):

- Time estimates unavailable → show `—` in time modes when remaining words cannot be computed.

## Persistence

### Settings keys

| Key | Type | Purpose |
|-----|------|---------|
| `progressVisibility` | enum string | Includes new time modes |
| `globalWpm` | number string | Aggregated global WPM |
| `globalActiveMs` | number string | Active ms for cold start / blend |

### Table `reading_pace`

```sql
CREATE TABLE IF NOT EXISTS reading_pace (
  book_id TEXT PRIMARY KEY,
  wpm REAL NOT NULL,
  active_ms INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  FOREIGN KEY (book_id) REFERENCES books(id)
);
```

No raw sample log in v1.

### Persist cadence

- Update memory on each accepted sample.
- Persist to SQLite on: book switch, app exit/clean shutdown if already hooked, and periodically or on position save (prefer **alongside existing position save** to avoid extra write paths).

## Progress display UX

### Extended `ProgressVisibility`

Cycle order (`p`, `/toggleprogress`, settings item):

1. `time-chapter` — e.g. `12 min left in chapter`
2. `time-book` — e.g. `1h 24m left in book`
3. `book` — existing book % bar
4. `both` — existing book + chapter %
5. `chapter` — existing chapter % bar
6. `hidden`

**Default for new installs:** `time-chapter`.  
Existing users who already have `book` / `both` / etc. keep their stored setting.

### Duration formatting

- &lt; 1 min → `1 min` (ceil; avoid “0 min”)
- Minutes only when &lt; 60 → `N min`
- ≥ 60 min → `Hh Mm` (omit zero minutes: `2h`)
- English strings for v1 to match current UI language (`left in chapter` / `left in book`)

## Data flow (session)

1. Open book → load position + `reading_pace` + global settings into tracker.
2. User navigates forward → sample → update EWMA → re-render footer if time mode.
3. Position save path also persists pace aggregates.
4. Close / change book → persist pace.

## Error handling

- Missing pace row → treat as no book-specific data (`bookWeight = 0`).
- Corrupt setting numbers → fall back to `DEFAULT_WPM` / `0` active ms.
- Outlier samples → ignore for WPM update only.
- DB write failure → keep in-memory estimate; do not crash the TUI (status warning optional).

## Testing

Unit tests (new `test/reading-pace.test.ts` or similar):

- EWMA / weighted update moves WPM in the expected direction
- Idle cap limits `activeMs`
- Outlier WPM rejected
- Hybrid blend weights with book active time
- Cold start uses default until threshold
- Remaining words and time for fixed chapter/book fixtures
- Duration formatting table
- Backward navigation produces no negative sample contribution

Command / settings tests:

- `progressVisibility` cycle includes new values
- Labels in settings panel cover new modes

## File impact summary

| File | Role |
|------|------|
| `src/reading-pace.ts` | **New** — pure algorithm |
| `src/types.ts` | Types / enum |
| `src/storage.ts` | Schema + CRUD |
| `src/screen.ts` | Footer formatting |
| `src/tui.ts` / input path | Tracker hooks |
| `src/executor.ts`, `commands.ts` | Toggle cycle |
| `src/settings-panel.ts` | UI labels |
| `test/reading-pace.test.ts` | **New** — unit tests |

## Success criteria

1. With a normal forward-reading session, time left decreases as the user advances.
2. Leaving the app idle for &gt; 2 minutes does not tank effective WPM.
3. After enough reading, estimates feel personal (differ from pure 230 WPM).
4. `p` cycles through time modes and existing percent modes cleanly.
5. All new pure logic covered by unit tests; existing test suite remains green.

## Open constants (implementation may tune; tests pin chosen values)

| Constant | Starting value |
|----------|----------------|
| `DEFAULT_WPM` | 230 |
| `IDLE_MS` | 120_000 |
| `COLD_START_MS` | 240_000 (4 min) |
| `BOOK_BLEND_MS` | 600_000 (10 min) |
| Instantaneous WPM filter | 50–800 |
| Pace update | Mass-weighted average by `activeMs` (not a free alpha) |

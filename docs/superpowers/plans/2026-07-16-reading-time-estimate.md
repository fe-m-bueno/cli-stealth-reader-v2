# Reading Time Estimate Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show Kindle-like time remaining (chapter and book) from a learned reading pace (active time + words advanced), with cycleable progress display modes.

**Architecture:** Pure `src/reading-pace.ts` owns WPM updates, hybrid blend, remaining-word math, and duration formatting. `syncPosition` in `tui.ts` feeds samples on every navigation. SQLite stores global WPM settings plus per-book `reading_pace`. Footer `formatProgress` and `/toggleprogress` gain `time-chapter` / `time-book` modes.

**Tech Stack:** TypeScript, Node native test runner (`node --import tsx --test`), better-sqlite3, existing TUI (`tui.ts` / `screen.ts` / `input.ts`).

**Spec:** `docs/superpowers/specs/2026-07-16-reading-time-estimate-design.md`

**Out of scope:** Full Kindle tabbed settings (Themes/Font/Layout/More) — separate follow-up after this plan.

---

## File map

| File | Responsibility |
|------|----------------|
| `src/reading-pace.ts` | **Create** — pure pace algorithm + formatting |
| `test/reading-pace.test.ts` | **Create** — unit tests for algorithm |
| `src/types.ts` | Extend `ProgressVisibility`; add pace types |
| `src/storage.ts` | `reading_pace` table; get/set pace; default progress |
| `test/storage.test.ts` | Persist/load reading pace |
| `src/screen.ts` | Time modes in `formatProgress` |
| `src/tui.ts` | Tracker on `AppState`; sample inside `syncPosition`; init |
| `src/executor.ts` | Load/flush pace on open book; extend toggle cycle |
| `src/commands.ts` | `/toggleprogress` usage/docs |
| `src/settings-panel.ts` | Labels for new progress modes |
| `src/help.ts` | Optional progress key description if needed |

---

### Task 1: Pure reading-pace module (TDD)

**Files:**
- Create: `src/reading-pace.ts`
- Create: `test/reading-pace.test.ts`

- [ ] **Step 1: Write failing tests for constants, sample update, and duration format**

Create `test/reading-pace.test.ts`:

```ts
import test from "node:test";
import assert from "node:assert/strict";
import {
  DEFAULT_WPM,
  IDLE_MS,
  COLD_START_MS,
  BOOK_BLEND_MS,
  MIN_INSTANT_WPM,
  MAX_INSTANT_WPM,
  createEmptyPaceState,
  applySample,
  effectiveWpm,
  remainingWordsInChapter,
  remainingWordsInBook,
  estimateMinutes,
  formatDuration,
  formatTimeLeft,
  absoluteWordCursor,
  type PaceState,
  type ChapterWordInfo
} from "../src/reading-pace.js";

test("formatDuration ceils sub-minute and formats hours", () => {
  assert.equal(formatDuration(0), "1 min");
  assert.equal(formatDuration(30), "1 min");
  assert.equal(formatDuration(90), "2 min");
  assert.equal(formatDuration(3600), "1h");
  assert.equal(formatDuration(3720), "1h 2m");
});

test("applySample ignores zero words and zero active time", () => {
  const base = createEmptyPaceState();
  const same = applySample(base, { wordsAdvanced: 0, activeMs: 60_000 });
  assert.equal(same.globalWpm, base.globalWpm);
  assert.equal(same.globalActiveMs, base.globalActiveMs);

  const same2 = applySample(base, { wordsAdvanced: 100, activeMs: 0 });
  assert.equal(same2.globalActiveMs, base.globalActiveMs);
});

test("applySample caps activeMs at IDLE_MS", () => {
  const base = createEmptyPaceState();
  const next = applySample(base, { wordsAdvanced: 400, activeMs: IDLE_MS * 3 });
  // Instantaneous WPM uses capped activeMs: 400 words / 2 min = 200 wpm
  assert.ok(next.globalActiveMs <= IDLE_MS);
  assert.ok(next.globalWpm > 0);
});

test("applySample rejects outlier instantaneous WPM", () => {
  const base = createEmptyPaceState();
  // 5000 words in 1 second => absurd WPM
  const next = applySample(base, { wordsAdvanced: 5000, activeMs: 1000 });
  assert.equal(next.globalWpm, base.globalWpm);
  assert.equal(next.globalActiveMs, base.globalActiveMs);
});

test("applySample mass-weights WPM toward observed speed", () => {
  let state = createEmptyPaceState();
  // 400 words in 2 minutes = 200 wpm
  state = applySample(state, { wordsAdvanced: 400, activeMs: 120_000 });
  assert.ok(Math.abs(state.globalWpm - 200) < 1);
  assert.equal(state.globalActiveMs, 120_000);
  assert.ok(Math.abs(state.bookWpm - 200) < 1);
  assert.equal(state.bookActiveMs, 120_000);
});

test("effectiveWpm uses default during cold start then global", () => {
  const cold: PaceState = {
    ...createEmptyPaceState(),
    globalWpm: 100,
    globalActiveMs: 0,
    bookWpm: 100,
    bookActiveMs: 0
  };
  assert.ok(Math.abs(effectiveWpm(cold) - DEFAULT_WPM) < 1);

  const warm: PaceState = {
    ...createEmptyPaceState(),
    globalWpm: 180,
    globalActiveMs: COLD_START_MS,
    bookWpm: 180,
    bookActiveMs: 0
  };
  assert.ok(Math.abs(effectiveWpm(warm) - 180) < 1);
});

test("effectiveWpm blends book pace as bookActiveMs grows", () => {
  const state: PaceState = {
    ...createEmptyPaceState(),
    globalWpm: 200,
    globalActiveMs: COLD_START_MS,
    bookWpm: 100,
    bookActiveMs: BOOK_BLEND_MS
  };
  assert.ok(Math.abs(effectiveWpm(state) - 100) < 1);
});

test("remaining words and estimates", () => {
  const chapters: ChapterWordInfo[] = [
    { wordCount: 1000 },
    { wordCount: 2000 },
    { wordCount: 500 }
  ];
  assert.equal(remainingWordsInChapter(chapters, 0, 0.25), 750);
  assert.equal(remainingWordsInBook(chapters, 0, 0.25), 750 + 2000 + 500);
  assert.equal(estimateMinutes(750, 250), 3);
});

test("absoluteWordCursor accumulates prior chapters", () => {
  const chapters: ChapterWordInfo[] = [
    { wordCount: 1000 },
    { wordCount: 2000 }
  ];
  assert.equal(absoluteWordCursor(chapters, 1, 0.5), 1000 + 1000);
});

test("formatTimeLeft returns em dash when words unavailable", () => {
  assert.equal(formatTimeLeft(0, 200, "chapter"), "—");
  assert.equal(formatTimeLeft(500, 0, "chapter"), "—");
  assert.match(formatTimeLeft(500, 250, "chapter"), /left in chapter/);
  assert.match(formatTimeLeft(500, 250, "book"), /left in book/);
});
```

- [ ] **Step 2: Run tests — expect FAIL (module missing)**

```bash
node --import tsx --test test/reading-pace.test.ts
```

Expected: cannot find module `../src/reading-pace.js` (or similar).

- [ ] **Step 3: Implement `src/reading-pace.ts`**

```ts
export const DEFAULT_WPM = 230;
export const IDLE_MS = 120_000;
export const COLD_START_MS = 240_000;
export const BOOK_BLEND_MS = 600_000;
export const MIN_INSTANT_WPM = 50;
export const MAX_INSTANT_WPM = 800;

export interface PaceState {
  globalWpm: number;
  globalActiveMs: number;
  bookId: string | null;
  bookWpm: number;
  bookActiveMs: number;
  /** Absolute word cursor at last sample (for forward-only delta). */
  lastWordCursor: number | null;
  lastSampleAt: number | null;
}

export interface PaceSample {
  wordsAdvanced: number;
  activeMs: number;
}

export interface ChapterWordInfo {
  wordCount: number;
}

export function createEmptyPaceState(partial?: Partial<PaceState>): PaceState {
  return {
    globalWpm: DEFAULT_WPM,
    globalActiveMs: 0,
    bookId: null,
    bookWpm: DEFAULT_WPM,
    bookActiveMs: 0,
    lastWordCursor: null,
    lastSampleAt: null,
    ...partial
  };
}

function clamp(n: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, n));
}

function massWeightedWpm(prevWpm: number, prevMs: number, sampleWpm: number, sampleMs: number): number {
  const total = prevMs + sampleMs;
  if (total <= 0) {
    return sampleWpm;
  }
  return (prevWpm * prevMs + sampleWpm * sampleMs) / total;
}

export function applySample(state: PaceState, sample: PaceSample): PaceState {
  const activeMs = Math.min(Math.max(0, sample.activeMs), IDLE_MS);
  const wordsAdvanced = Math.max(0, sample.wordsAdvanced);
  if (wordsAdvanced <= 0 || activeMs <= 0) {
    return state;
  }
  const minutes = activeMs / 60_000;
  const instantWpm = wordsAdvanced / minutes;
  if (instantWpm < MIN_INSTANT_WPM || instantWpm > MAX_INSTANT_WPM) {
    return state;
  }

  const nextGlobalMs = state.globalActiveMs + activeMs;
  const nextBookMs = state.bookActiveMs + activeMs;
  return {
    ...state,
    globalWpm: massWeightedWpm(state.globalWpm, state.globalActiveMs, instantWpm, activeMs),
    globalActiveMs: nextGlobalMs,
    bookWpm: massWeightedWpm(state.bookWpm, state.bookActiveMs, instantWpm, activeMs),
    bookActiveMs: nextBookMs
  };
}

export function effectiveWpm(state: PaceState): number {
  let base: number;
  if (state.globalActiveMs < COLD_START_MS) {
    const t = state.globalActiveMs / COLD_START_MS;
    base = (1 - t) * DEFAULT_WPM + t * state.globalWpm;
  } else {
    base = state.globalWpm;
  }
  const bookWeight = clamp(state.bookActiveMs / BOOK_BLEND_MS, 0, 1);
  return (1 - bookWeight) * base + bookWeight * state.bookWpm;
}

export function absoluteWordCursor(
  chapters: ChapterWordInfo[],
  chapterIndex: number,
  chapterProgress: number
): number {
  let words = 0;
  const safeIndex = clamp(chapterIndex, 0, Math.max(0, chapters.length - 1));
  for (let i = 0; i < safeIndex; i += 1) {
    words += Math.max(0, chapters[i]?.wordCount ?? 0);
  }
  const chapterWords = Math.max(0, chapters[safeIndex]?.wordCount ?? 0);
  words += clamp(chapterProgress, 0, 1) * chapterWords;
  return words;
}

export function remainingWordsInChapter(
  chapters: ChapterWordInfo[],
  chapterIndex: number,
  chapterProgress: number
): number {
  const chapterWords = Math.max(0, chapters[chapterIndex]?.wordCount ?? 0);
  return Math.max(0, chapterWords * (1 - clamp(chapterProgress, 0, 1)));
}

export function remainingWordsInBook(
  chapters: ChapterWordInfo[],
  chapterIndex: number,
  chapterProgress: number
): number {
  let remaining = remainingWordsInChapter(chapters, chapterIndex, chapterProgress);
  for (let i = chapterIndex + 1; i < chapters.length; i += 1) {
    remaining += Math.max(0, chapters[i]?.wordCount ?? 0);
  }
  return remaining;
}

export function estimateMinutes(remainingWords: number, wpm: number): number {
  if (remainingWords <= 0 || wpm <= 0) {
    return 0;
  }
  return remainingWords / wpm;
}

/** Format a duration in seconds for the footer. */
export function formatDuration(totalSeconds: number): string {
  const seconds = Math.max(0, totalSeconds);
  const totalMinutes = Math.max(1, Math.ceil(seconds / 60));
  if (totalMinutes < 60) {
    return `${totalMinutes} min`;
  }
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  if (minutes === 0) {
    return `${hours}h`;
  }
  return `${hours}h ${minutes}m`;
}

export function formatTimeLeft(
  remainingWords: number,
  wpm: number,
  scope: "chapter" | "book"
): string {
  if (remainingWords <= 0 || wpm <= 0) {
    return "—";
  }
  const minutes = estimateMinutes(remainingWords, wpm);
  const label = formatDuration(minutes * 60);
  return scope === "chapter" ? `${label} left in chapter` : `${label} left in book`;
}

/**
 * Given prior tracker fields + new position, produce sample deltas and updated cursor clocks.
 * Pure helper for syncPosition wiring.
 */
export function prepareSample(args: {
  state: PaceState;
  now: number;
  wordCursor: number;
  readingActive: boolean;
}): { sample: PaceSample | null; nextMeta: Pick<PaceState, "lastWordCursor" | "lastSampleAt"> } {
  const { state, now, wordCursor, readingActive } = args;
  if (!readingActive) {
    return {
      sample: null,
      nextMeta: { lastWordCursor: wordCursor, lastSampleAt: state.lastSampleAt }
    };
  }
  if (state.lastWordCursor === null || state.lastSampleAt === null) {
    return {
      sample: null,
      nextMeta: { lastWordCursor: wordCursor, lastSampleAt: now }
    };
  }
  const wordsAdvanced = Math.max(0, wordCursor - state.lastWordCursor);
  const activeMs = Math.max(0, now - state.lastSampleAt);
  return {
    sample: { wordsAdvanced, activeMs },
    nextMeta: { lastWordCursor: wordCursor, lastSampleAt: now }
  };
}
```

Note: `applySample` with first warm-up sample sets globalWpm from mass-weighted formula. When `prevMs` is 0, `massWeightedWpm` returns `sampleWpm` — first sample of 400 words / 2 min → 200 wpm. Tests expect that.

Cold-start test: `globalActiveMs: 0` → effective = DEFAULT_WPM regardless of globalWpm. After first sample, `globalActiveMs` increases and `globalWpm` updates — blend applies. The cold test uses `globalActiveMs: 0` only.

For "applySample mass-weights" first sample from empty: empty has `globalActiveMs: 0` and `globalWpm: DEFAULT_WPM`. First sample mass-weights: `(230 * 0 + 200 * 120000) / 120000 = 200`. Good.

- [ ] **Step 4: Run tests — expect PASS**

```bash
node --import tsx --test test/reading-pace.test.ts
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/reading-pace.ts test/reading-pace.test.ts
git commit -m "feat: add pure reading-pace module for time-left estimates"
```

---

### Task 2: Types — extend ProgressVisibility and pace fields

**Files:**
- Modify: `src/types.ts`

- [ ] **Step 1: Update `ProgressVisibility` and `AppState`**

In `src/types.ts`, change:

```ts
export type ProgressVisibility =
  | "time-chapter"
  | "time-book"
  | "book"
  | "both"
  | "chapter"
  | "hidden";
```

Add (near other interfaces):

```ts
export interface BookReadingPace {
  bookId: string;
  wpm: number;
  activeMs: number;
  updatedAt: number;
}
```

On `AppState`, add:

```ts
  readingPace: import("./reading-pace.js").PaceState;
```

(Or import `PaceState` at top if the project prefers static imports — match local style; `storage` uses `import()` only for Storage. Prefer top-level:

```ts
import type { PaceState } from "./reading-pace.js";
```

only if types file already imports from src modules — it currently does **not**. Keep pace state as an inline structural type or re-export from reading-pace and use:

```ts
  /** Runtime tracker; see reading-pace.ts */
  readingPace: {
    globalWpm: number;
    globalActiveMs: number;
    bookId: string | null;
    bookWpm: number;
    bookActiveMs: number;
    lastWordCursor: number | null;
    lastSampleAt: number | null;
  };
```

Prefer importing type from reading-pace to avoid drift:

```ts
import type { PaceState } from "./reading-pace.js";
// ...
  readingPace: PaceState;
```

If circular dependency appears (reading-pace must not import types), keep `PaceState` only in `reading-pace.ts` and import it into `types.ts` — `reading-pace` should not import `types.ts`.

- [ ] **Step 2: Fix compile fallout from the union change**

Search for `ProgressVisibility` exhaustive switches / arrays:

```bash
rg 'ProgressVisibility|"book".*"both".*"chapter".*"hidden"' src test
```

Update every cycle array to the new order (do minimal updates so `tsc` still fails only where Task 3–5 will finish wiring — or fix arrays now to compile).

Cycle order (canonical):

```ts
const PROGRESS_VALUES: ProgressVisibility[] = [
  "time-chapter",
  "time-book",
  "book",
  "both",
  "chapter",
  "hidden"
];
```

Update at least:
- `src/executor.ts` `toggleprogress` values array
- `src/settings-panel.ts` `PROGRESS_VALUES` + `progressLabel`
- `src/commands.ts` usage string
- Any test fixtures constructing `AppState` (add `readingPace: createEmptyPaceState()`)

- [ ] **Step 3: Commit types scaffolding**

```bash
git add src/types.ts src/executor.ts src/settings-panel.ts src/commands.ts test
git commit -m "feat: extend progress visibility with time-left modes"
```

(If tests fail until Task 4–5, only commit files that keep the suite green. Prefer completing Task 3 storage before broad AppState fixture edits.)

**Safer split:** In this task only change the type alias and add `BookReadingPace`. Defer `AppState.readingPace` and array updates to Tasks 3–5 if needed to keep green commits. **Recommended:** do type + all cycle arrays + `progressLabel` + command help here; add `readingPace` on AppState when tui is wired (Task 5) so tests that build AppState do not break early.

**Revised Task 2 scope (keep suite green):**
1. Extend `ProgressVisibility` union
2. Update cycle arrays, labels, command docs
3. Do **not** add `readingPace` to AppState until Task 5

- [ ] **Step 4: Run tests**

```bash
npm test
```

Expected: pass (toggleprogress tests may need updating if any assert old cycle only).

- [ ] **Step 5: Commit**

```bash
git add src/types.ts src/executor.ts src/settings-panel.ts src/commands.ts
git commit -m "feat: add time-chapter and time-book progress visibility modes"
```

---

### Task 3: Storage for pace aggregates

**Files:**
- Modify: `src/storage.ts`
- Modify: `test/storage.test.ts`

- [ ] **Step 1: Write failing storage tests**

Append to `test/storage.test.ts`:

```ts
test("getReadingPace returns null when missing and upserts book pace", () => {
  const { storage, cleanup } = makeTempStorage();
  try {
    assert.equal(storage.getReadingPace("book-1"), null);
    storage.saveReadingPace({
      bookId: "book-1",
      wpm: 210.5,
      activeMs: 90_000,
      updatedAt: 1_700_000_000_000
    });
    const row = storage.getReadingPace("book-1");
    assert.ok(row);
    assert.equal(row.bookId, "book-1");
    assert.ok(Math.abs(row.wpm - 210.5) < 0.001);
    assert.equal(row.activeMs, 90_000);
  } finally {
    cleanup();
  }
});

test("global pace settings round-trip via raw settings", () => {
  const { storage, cleanup } = makeTempStorage();
  try {
    storage.setRawSetting("globalWpm", "215.25");
    storage.setRawSetting("globalActiveMs", "120000");
    assert.equal(storage.getSetting("globalWpm"), "215.25");
    assert.equal(storage.getSetting("globalActiveMs"), "120000");
  } finally {
    cleanup();
  }
});
```

- [ ] **Step 2: Run test — expect FAIL**

```bash
node --import tsx --test test/storage.test.ts
```

Expected: `getReadingPace` is not a function.

- [ ] **Step 3: Implement table + methods in `storage.ts`**

In constructor `db.exec` schema, add:

```sql
CREATE TABLE IF NOT EXISTS reading_pace (
  book_id TEXT PRIMARY KEY,
  wpm REAL NOT NULL,
  active_ms INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
```

Import `BookReadingPace` from types.

Methods:

```ts
  getReadingPace(bookId: string): BookReadingPace | null {
    const row = this.db.prepare(`
      SELECT book_id AS bookId, wpm, active_ms AS activeMs, updated_at AS updatedAt
      FROM reading_pace WHERE book_id = ?
    `).get(bookId) as BookReadingPace | undefined;
    return row ?? null;
  }

  saveReadingPace(pace: BookReadingPace): void {
    this.db.prepare(`
      INSERT INTO reading_pace (book_id, wpm, active_ms, updated_at)
      VALUES (?, ?, ?, ?)
      ON CONFLICT(book_id) DO UPDATE SET
        wpm = excluded.wpm,
        active_ms = excluded.active_ms,
        updated_at = excluded.updated_at
    `).run(pace.bookId, pace.wpm, pace.activeMs, pace.updatedAt);
  }
```

Change default progress for **new** installs:

```ts
const DEFAULT_SETTINGS: AppSettings = {
  // ...
  progressVisibility: "time-chapter",
  // ...
};
```

Note: `INSERT OR IGNORE` seed means existing DBs keep old `progressVisibility`; only fresh DBs get `time-chapter`. Matches spec.

- [ ] **Step 4: Run storage tests — PASS**

```bash
node --import tsx --test test/storage.test.ts
```

- [ ] **Step 5: Commit**

```bash
git add src/storage.ts test/storage.test.ts
git commit -m "feat: persist per-book reading pace in SQLite"
```

---

### Task 4: Footer formatting for time modes

**Files:**
- Modify: `src/screen.ts` (`formatProgress`)
- Optionally add pure tests that call `formatProgress` with a minimal state stub

- [ ] **Step 1: Extend `formatProgress`**

Replace body of `formatProgress` in `src/screen.ts` to branch on time modes:

```ts
export function formatProgress(state: AppState, mainWidth: number, bodyHeight: number): string {
  if (!state.currentBook || state.progressVisibility === "hidden") {
    return "";
  }

  if (state.progressVisibility === "time-chapter" || state.progressVisibility === "time-book") {
    const book = state.currentBook;
    const chapters = book.chapters.map((c) => ({ wordCount: c.wordCount }));
    const chapterProgress = computeChapterProgress(state, mainWidth, bodyHeight);
    const remaining =
      state.progressVisibility === "time-chapter"
        ? remainingWordsInChapter(chapters, state.chapterIndex, chapterProgress)
        : remainingWordsInBook(chapters, state.chapterIndex, chapterProgress);
    const wpm = effectiveWpm(state.readingPace);
    const scope = state.progressVisibility === "time-chapter" ? "chapter" : "book";
    // If entire remaining word pool is 0 (e.g. image book), show em dash
    if (chapters.every((c) => c.wordCount === 0)) {
      return "—";
    }
    return formatTimeLeft(remaining, wpm, scope);
  }

  const parts: string[] = [];
  if (state.progressVisibility === "book" || state.progressVisibility === "both") {
    const bookProg = computeBookProgress(state, mainWidth, bodyHeight);
    parts.push(`book ${progressBar(bookProg, PROGRESS_BAR_WIDTH, state.theme)} ${Math.round(bookProg * 100)}%`);
  }
  if (state.progressVisibility === "chapter" || state.progressVisibility === "both") {
    const chapterProg = computeChapterProgress(state, mainWidth, bodyHeight);
    parts.push(`ch ${progressBar(chapterProg, PROGRESS_BAR_WIDTH, state.theme)} ${Math.round(chapterProg * 100)}%`);
  }
  return parts.join(` ${fg(state.theme.border, "·")} `);
}
```

Import from `./reading-pace.js`: `effectiveWpm`, `remainingWordsInChapter`, `remainingWordsInBook`, `formatTimeLeft`.

**Dependency:** This needs `state.readingPace`. If Task 5 has not landed, add temporary:

```ts
const pace = state.readingPace ?? createEmptyPaceState();
```

Or complete Task 5 first (recommended order: Task 5 before finishing this step if AppState lacks the field).

**Order adjustment:** Implement Task 5 AppState + syncPosition before requiring formatProgress to use `state.readingPace`. If coding linearly, stub:

```ts
import { createEmptyPaceState, effectiveWpm, ... } from "./reading-pace.js";
// ...
const pace = "readingPace" in state && state.readingPace
  ? state.readingPace
  : createEmptyPaceState();
```

Cleaner: do Task 5 then Task 4, or combine 4+5. **This plan keeps Task 4 after Task 5.**

**Renumber:** Treat Task 4 as footer **after** Task 5 wiring. For the document, swap execution order: complete Task 5, then Task 4.

---

### Task 5: Runtime tracker + sample on `syncPosition`

**Files:**
- Modify: `src/types.ts` (`readingPace` on AppState)
- Modify: `src/tui.ts` (init + `syncPosition`)
- Modify: `src/executor.ts` (`openBook` load/flush)
- Modify: test fixtures that construct `AppState` (`test/tui-startup.test.ts`, others)

- [ ] **Step 1: Add `readingPace` to AppState and fix fixtures**

```ts
import type { PaceState } from "./reading-pace.js";
// AppState:
  readingPace: PaceState;
```

In `test/tui-startup.test.ts` and any other `AppState` object literals, add:

```ts
import { createEmptyPaceState } from "../src/reading-pace.js";
// ...
readingPace: createEmptyPaceState(),
```

- [ ] **Step 2: Initialize pace in `runTui`**

```ts
import {
  createEmptyPaceState,
  applySample,
  absoluteWordCursor,
  prepareSample
} from "./reading-pace.js";

function loadGlobalPace(storage: Storage): Pick<PaceState, "globalWpm" | "globalActiveMs"> {
  const wpm = Number(storage.getSetting("globalWpm"));
  const activeMs = Number(storage.getSetting("globalActiveMs"));
  return {
    globalWpm: Number.isFinite(wpm) && wpm > 0 ? wpm : DEFAULT_WPM,
    globalActiveMs: Number.isFinite(activeMs) && activeMs >= 0 ? activeMs : 0
  };
}

function persistPace(state: AppState): void {
  state.storage.setRawSetting("globalWpm", String(state.readingPace.globalWpm));
  state.storage.setRawSetting("globalActiveMs", String(state.readingPace.globalActiveMs));
  if (state.readingPace.bookId) {
    state.storage.saveReadingPace({
      bookId: state.readingPace.bookId,
      wpm: state.readingPace.bookWpm,
      activeMs: state.readingPace.bookActiveMs,
      updatedAt: Date.now()
    });
  }
}
```

In `runTui` state init:

```ts
readingPace: createEmptyPaceState(loadGlobalPace(storage)),
```

- [ ] **Step 3: Sample inside `syncPosition`**

```ts
function syncPosition(state: AppState): void {
  if (!state.currentBook) {
    return;
  }
  const width = process.stdout.columns || 120;
  const height = process.stdout.rows || 40;
  const layout = getViewportLayout(state, width, height);
  const originalOffset = state.blockOffset;
  if (state.focusMode) {
    state.focusBlockIndex = clampFocusBlockIndex(state, state.focusBlockIndex);
    state.blockOffset = mapFocusIndexToBlockOffset(state, layout.contentWidth, state.focusBlockIndex);
  }

  const chapterProgress = computeChapterProgress(state, layout.contentWidth, layout.bodyHeight);
  const bookProgress = computeBookProgress(state, layout.contentWidth, layout.bodyHeight);

  const chapters = state.currentBook.chapters.map((c) => ({ wordCount: c.wordCount }));
  const wordCursor = absoluteWordCursor(chapters, state.chapterIndex, chapterProgress);
  const readingActive = state.overlay === "none" && !state.commandMode;
  const now = Date.now();
  const { sample, nextMeta } = prepareSample({
    state: state.readingPace,
    now,
    wordCursor,
    readingActive
  });
  let pace = { ...state.readingPace, ...nextMeta };
  if (sample) {
    pace = { ...applySample(pace, sample), ...nextMeta };
  }
  state.readingPace = pace;
  persistPace(state);

  state.storage.savePosition({
    bookId: state.currentBook.id,
    chapterIndex: state.chapterIndex,
    chapterProgress,
    bookProgress,
    blockOffset: state.blockOffset
  });
  if (state.focusMode) {
    state.blockOffset = originalOffset;
  }
}
```

- [ ] **Step 4: Load/flush book pace in `openBook`**

At start of `openBook`, if switching away from a previous book with pace data, persist (call shared `persistPace` exported from tui or a tiny `src/pace-session.ts` helper to avoid circular imports).

**Avoid cycles:** put `loadGlobalPace` / `persistPace` / `bindBookPace` in `src/reading-pace.ts` as pure+storage-agnostic, and keep storage calls in executor:

```ts
// executor openBook — before assigning new book:
if (state.currentBook && state.readingPace.bookId) {
  state.storage.setRawSetting("globalWpm", String(state.readingPace.globalWpm));
  state.storage.setRawSetting("globalActiveMs", String(state.readingPace.globalActiveMs));
  state.storage.saveReadingPace({
    bookId: state.readingPace.bookId,
    wpm: state.readingPace.bookWpm,
    activeMs: state.readingPace.bookActiveMs,
    updatedAt: Date.now()
  });
}

// after position restore:
const row = state.storage.getReadingPace(book.id);
const globalWpm = Number(state.storage.getSetting("globalWpm"));
const globalActiveMs = Number(state.storage.getSetting("globalActiveMs"));
state.readingPace = createEmptyPaceState({
  globalWpm: Number.isFinite(globalWpm) && globalWpm > 0 ? globalWpm : DEFAULT_WPM,
  globalActiveMs: Number.isFinite(globalActiveMs) && globalActiveMs >= 0 ? globalActiveMs : 0,
  bookId: book.id,
  bookWpm: row?.wpm ?? DEFAULT_WPM,
  bookActiveMs: row?.activeMs ?? 0,
  lastWordCursor: null,
  lastSampleAt: null
});
```

On quit (`shouldQuit` / `exitTui`), ensure one last `persistPace` if not already covered by last `syncPosition`.

- [ ] **Step 5: Run full test suite**

```bash
npm test
```

Expected: pass.

- [ ] **Step 6: Commit**

```bash
git add src/types.ts src/tui.ts src/executor.ts test
git commit -m "feat: track reading pace samples on position sync"
```

---

### Task 4 (after Task 5): Footer time modes

**Files:**
- Modify: `src/screen.ts`

- [ ] **Step 1: Implement `formatProgress` time branches** (code in Task 4 section above) using `state.readingPace`.

- [ ] **Step 2: Add a focused unit test** in `test/reading-pace.test.ts` or `test/screen-progress.test.ts` that builds a minimal mock AppState if practical; otherwise rely on pure `formatTimeLeft` tests already in Task 1 and a thin integration assertion if fixtures exist.

Minimal screen test (optional but preferred):

```ts
// Only if constructing AppState is already done in tests — otherwise skip.
```

- [ ] **Step 3: `npm test` + commit**

```bash
git add src/screen.ts
git commit -m "feat: show time left in chapter/book in progress footer"
```

---

### Task 6: Commands, settings labels, help polish

**Files:**
- Modify: `src/commands.ts` (if not fully done in Task 2)
- Modify: `src/settings-panel.ts` (if not fully done in Task 2)
- Modify: `src/executor.ts` (validate explicit `/toggleprogress time-chapter`)
- Modify: `src/help.ts` if progress description should mention time modes

- [ ] **Step 1: Ensure command definition**

```ts
usage: "/toggleprogress [time-chapter|time-book|book|both|chapter|hidden]",
details: [
  "With no argument, cycles through progress display modes.",
  "time-chapter and time-book show estimated remaining reading time from learned pace.",
  "book/chapter/both show percentage bars; hidden disables the footer progress line."
],
```

- [ ] **Step 2: Settings labels**

```ts
function progressLabel(value: AppSettings["progressVisibility"]): string {
  switch (value) {
    case "time-chapter":
      return "Time left in chapter";
    case "time-book":
      return "Time left in book";
    case "book":
      return "Book %";
    case "both":
      return "Book + chapter %";
    case "chapter":
      return "Chapter %";
    case "hidden":
      return "Hidden";
  }
}
```

- [ ] **Step 3: Run tests + commit**

```bash
npm test
git add src/commands.ts src/settings-panel.ts src/executor.ts src/help.ts
git commit -m "feat: document and label reading time progress modes"
```

---

### Task 7: Manual smoke + verification

- [ ] **Step 1: Typecheck / build**

```bash
npm run build
```

Expected: clean compile.

- [ ] **Step 2: Manual smoke (optional if no EPUB handy)**

```bash
npm run dev -- path/to/book.epub
```

Checks:
1. Footer shows `N min left in chapter` (or default mode).
2. `p` cycles: time-chapter → time-book → book % → both → chapter % → hidden → …
3. After scrolling for a bit, estimate moves; idle ~2+ min then resume does not collapse WPM to near-zero.
4. Image-only / zero wordCount book shows `—` in time modes.

- [ ] **Step 3: Final commit only if docs/README need a line**

Update `README.md` progress section briefly:

```md
- Progresso: tempo restante no capítulo/livro (ritmo aprendido) ou barras %; ciclar com `p`
```

```bash
git add README.md
git commit -m "docs: mention learned reading-time progress modes"
```

---

## Self-review (plan vs spec)

| Spec requirement | Task |
|------------------|------|
| Pure pace module | Task 1 |
| Active time + forward words | Task 1 `prepareSample` + Task 5 |
| Hybrid global/book WPM | Task 1 `effectiveWpm` |
| Cold start 230 / 4 min | Task 1 constants |
| Idle 2 min cap | Task 1 `IDLE_MS` |
| Persist global + per-book | Task 3 + Task 5 |
| time-chapter / time-book display | Task 2 + Task 4 |
| Cycle with p / settings | Task 2 + Task 6 |
| Zero wordCount → — | Task 1 + Task 4 |
| Overlay not sampling | Task 5 `readingActive` |
| Outlier filter | Task 1 |
| Default time-chapter new installs | Task 3 DEFAULT_SETTINGS |
| No Kindle full menu | Explicitly out of scope |
| Unit tests | Tasks 1, 3 |

No TBD placeholders remaining in task steps.

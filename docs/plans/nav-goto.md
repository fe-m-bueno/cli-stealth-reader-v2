# Navigation: Jump to a Percentage (/goto)

## Goal

Jump to a specific position in the book or chapter using a percentage or a chapter number.

## Context

`src/commands.ts`, `src/executor.ts`.
`src/screen.ts` — `computeBookProgress`, `computeChapterMaxOffset`.

## Design

### Command Variants

- `/goto 42%` — jumps to 42% of the book (computes the chapter plus a proportional offset)
- `/goto 42%c` or `/goto 42% --chapter` — jumps to 42% of the current chapter
- `/goto 5` — jumps to chapter 5 (a shortcut for `/chapters` + Enter)

### Calculation Logic

**Book percentage:**

1. Compute the book's total word count: `sum(chapter.wordCount)`.
2. `targetWord = totalWords * 0.42`.
3. Iterate over the chapters accumulating wordCount until you find the chapter containing `targetWord`.
4. Within the chapter: `blockOffset = Math.floor((targetWord - accumulated) / chapterWordCount * chapterMaxOffset)`.

**Chapter percentage:**

1. `blockOffset = Math.floor(percentage * chapterMaxOffset)`.

**Chapter number:**

1. Validate the range, then set `chapterIndex = n - 1`, `blockOffset = 0`.

### Feedback

After the jump: `status = "Jumped to 42% (Ch.7 · §123)"`.

### Pushing to Navigation History

Call `pushNavHistory` before the jump (this integrates with the history plan).

## Files to Modify

- `src/commands.ts`: definition of `/goto` with a `position` argument
- `src/executor.ts`: implement the three goto modes
- `src/input.ts`: no key change required (it goes through the command bar)

## Acceptance Criteria

- `/goto 0%` → the start of the book, `/goto 100%` → the last block of the last chapter.
- `/goto 3` in a book with 2 chapters → a friendly error status.
- Works correctly for books with chapters whose wordCount is 0.

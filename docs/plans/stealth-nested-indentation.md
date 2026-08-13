# Stealth: Nested Indentation and Blank Lines

## Goal
Make code-mode output look like a real file, with nested functions, conditional blocks, and strategic blank lines — not a uniform sequence of statements.

## Context
`src/renderers.ts` — `renderCode`, structural blocks (import/interface/function/async).
Today every block has an indentation depth of 0 or 1.

## Design

### Blank Lines Between Blocks
In `renderBlocks`, instead of always emitting `""` between blocks, vary it:
- 70% → one blank line (current behavior)
- 20% → no blank line (statements inside a function)
- 10% → two blank lines (between "functions")

Use `lineHash(index, 999) % 10` to decide deterministically.

### Variable Indentation Inside Structural Blocks
When a structural block opens a function or if:
- The first N/2 wrapped lines get a `  ` indent (inside the function)
- The last line: the closing `}`

Add a conditional block:
```ts
// blockIndex % 41
if (condName) {
  // first lines with indent
} else {
  // last lines with indent
}
```

### Optional Nested Blocks
When a block is inside a function (`structLines.length > 0`), roughly 30% of the body lines get a `    ` (double) indent to simulate code inside an inner if/for.

Use `lineHash(blockIndex, lineIndex + 50) % 3 === 0` to decide.

### "for loop" Pattern (new structural block, `blockIndex % 43`)
```ts
for (const item of items) {
  // the block's lines
}
```

### "try/catch" Pattern (new structural block, `blockIndex % 47`)
```ts
try {
  // first lines
} catch (err) {
  // last lines
}
```

## Files to Modify
- `src/renderers.ts`: spacing logic in `renderBlocks`, new structural blocks, variable indentation in `renderCode`

## Acceptance Criteria
- The output looks like a real JS/TS file when scrolled quickly.
- No line exceeds `width` (the extra indentation is deducted from `textWidth`).
- Deterministic.

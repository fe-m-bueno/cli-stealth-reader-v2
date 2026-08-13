# Stealth: Advanced TypeScript

## Goal
Make code mode more convincing by adding real TypeScript constructs beyond simple assignments.

## Context
`src/renderers.ts` — the `renderCode` and `disguiseLine` functions, and `LINE_PATTERNS`.
Today there are 12 line patterns (patConst, patLet, patComment, and so on) and 4 structural blocks (import, interface, function, async function).

## What to Add

### New Line Patterns
- `patCast`: `const x = value as TypeName;`
- `patGenericCall`: `const x = processItems<TypeName>(arg);`
- `patDestructure`: `const { prop1, prop2 } = state;`
- `patSpread`: `const next = { ...ctx, key: "value" };`
- `patTernary`: `const x = cond ? "value" : fallback;`

### New Structural Blocks
- **Enum** (for example, `blockIndex % 17`):
  ```ts
  enum StateName { Active, Pending, Resolved }
  ```
- **Decorator + class method** (for example, `blockIndex % 31`):
  ```ts
  @Injectable()
  class ServiceName { … }
  ```
- **Generic function** (for example, `blockIndex % 37`):
  ```ts
  function process<T extends TypeName>(item: T): Promise<T> { … }
  ```
- **Conditional block** (`if/else`) with the text distributed across both branches.

### Generic Type Names
Extend `toTypeName` to append `<T>`, `<T, K>`, or `<T extends Base>` in roughly 30% of uses.

## Files to Modify
- `src/renderers.ts`: add the new `pat*` functions and structural blocks, and include them in the `LINE_PATTERNS` array and in the structure selection logic.

## Acceptance Criteria
- No line exceeds `width` columns (use a revised `TEXT_OVERHEAD` if needed).
- The pattern is deterministic: the same `blockIndex` → the same output.
- Plain mode is not broken.

# DONE

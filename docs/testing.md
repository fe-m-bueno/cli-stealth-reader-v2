# Testing and migration contracts

Run the fast suite while developing:

```bash
npm test
```

Run the source-only coverage gate before merging:

```bash
npm run test:coverage
```

The gate measures `src/**/*.ts` only. Test files are deliberately excluded so that adding assertions cannot inflate the application metric. The minimums are 90% lines, 83% branches, and 92% functions.

## Contracts for a Rust rewrite

Treat tests as behavioral specifications, not as a requirement to reproduce the TypeScript implementation.

- `test/code-renderers-contract.test.ts` defines deterministic, width-bounded rendering for TypeScript, Python, and Rust stealth modes, including escaping and canonical structural blocks.
- `test/executor-contract.test.ts` defines command behavior for navigation, history, bookmarks, search, positions, preferences, focus mode, resume, and removal.
- `test/library-cbz-pdf.test.ts`, `test/epub.test.ts`, and `test/html.test.ts` define import behavior and the canonical book/chapter/block output.
- `test/storage.test.ts` and the library integration tests define persistence, redaction, atomic settings, export/import, tags, notes, and reading positions.

For cross-language parity, port these cases to data-driven Rust tests and compare observable results: canonical data, stripped terminal text, state transitions, status messages, and persisted values. Avoid copying private TypeScript helper structure into the Rust test design.

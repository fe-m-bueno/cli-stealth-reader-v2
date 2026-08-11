# cli-stealth-reader-v2

The Rust implementation of `cli-stealth-reader`, a full-screen terminal reader for EPUB, CBZ, and PDF books with plain and code-disguised reading modes.

The migration preserves the observable behavior and user data of the TypeScript v1 while replacing the Node.js runtime with native binaries. The imported `docs/` directory is the product and compatibility reference; unshipped plans are not part of the initial parity gate.

## Development

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The workspace starts with a domain crate and a thin composition-root binary. Format parsing, storage, integrations, and the TUI will be added as focused crates when their compatibility seams are ported.

See [the migration architecture](docs/migration/architecture.md), [the v1 compatibility contract](docs/migration/compatibility-contract.md), and [the reference-repository research](docs/research/codex-and-grok-build-architecture.md).

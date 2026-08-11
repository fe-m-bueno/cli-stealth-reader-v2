# Rust architecture references: Codex and Grok Build

Wayfinder context: [Research Rust architecture standards from Codex and Grok Build](https://github.com/fe-m-bueno/cli-stealth-reader/issues/33).

## Primary-source observations

- OpenAI Codex uses a Cargo workspace with a reusable `core` crate, separate headless and TUI frontends, and a thin CLI that composes them. Its contributor workflow scopes checks and tests to touched crates before the full suite. Sources: [Codex Rust README](https://github.com/openai/codex/blob/main/codex-rs/README.md), [Codex build instructions](https://github.com/openai/codex/blob/main/docs/install.md), and [Codex TUI manifest](https://github.com/openai/codex/blob/main/codex-rs/tui/Cargo.toml).
- Codex's TUI is a library as well as a binary and uses Ratatui with dedicated test dependencies, supporting a renderable/testable UI outside the process entry point. Source: [Codex TUI manifest](https://github.com/openai/codex/blob/main/codex-rs/tui/Cargo.toml).
- Grok Build separates its composition-root binary, TUI, runtime, tools, and host/workspace adapters into focused crates. It centralizes workspace dependency versions, lints, formatting, and release profiles, and recommends per-crate checks during development. Source: [Grok Build repository README](https://github.com/xai-org/grok-build).
- Both projects build native terminal applications as workspaces and keep terminal presentation separate from core behavior. Both also favor pinned, reproducible tooling and targeted developer feedback loops before full-workspace validation.

## Adaptation for cli-stealth-reader-v2

This reader needs the same direction but not the same scale. It will use a small capability workspace, introduce crates only with working vertical slices, and avoid generated manifests, vendored third-party trees, Bazel, or a bespoke tool bootstrap. The useful standards are dependency direction, thin entry points, testable TUI state/rendering, centralized lints, committed lockfiles, and scoped checks.

Ratatui's current documented pattern reinforces this separation: update state outside rendering, implement widgets over shared references, and verify output with `TestBackend` buffers. Source: [Ratatui repository and examples](https://github.com/ratatui/ratatui).

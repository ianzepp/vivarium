# Vivarium Agent Guide

## Project
Vivarium is a local-first email archive, retrieval, sync, indexing, and write
layer for private agents. The public CLI binary is `vivi`.

The codebase is Rust, Edition 2024, with the toolchain pinned in
`rust-toolchain.toml`. Treat `Cargo.toml`, `README.md`, and the code as the
source of truth when these instructions drift.

## Current Shape
- Package: `vivarium`
- Binary: `vivi`, defined explicitly with `autobins = false`
- Optional feature: `outbox`
- Storage: raw `.eml` blobs plus SQLite metadata, indexes, and embeddings
- Providers: standard IMAP/SMTP, Proton Bridge-style config, and direct Proton API paths

## Coding Standards
- Prefer existing modules and helper APIs before adding new abstractions.
- Keep production errors in `VivariumError` with `thiserror`; do not introduce `anyhow`.
- Avoid panics in production paths. Return `Result` where practical, and only use
  `unreachable!` or `unwrap` for truly invariant conditions.
- Use `clap` derive for CLI parsing.
- Use `tracing` and `tracing-subscriber` for logging.
- Use `tokio` for async work; the CLI entrypoint uses `#[tokio::main]`.
- Keep files and functions small. Hygiene tests enforce a 1000-line file ceiling
  and a 60-line function ceiling for checked `src/**/*.rs` files.
- Prefer `module.rs` files until a module genuinely needs submodules. When it
  does, use `module/leaf.rs` files with a parent `module.rs` rather than
  `module/mod.rs`.
- Put inline unit tests in `#[cfg(test)]` modules near the code when that keeps
  behavior easy to understand. Use `tests/` for integration and CLI behavior.

## Entry Point
`src/main.rs` should keep the current shape:

- `main()` parses `Cli`, initializes tracing, calls `run(...)`, prints errors to
  stderr, and exits nonzero on failure.
- `run(...)` returns `Result<(), VivariumError>`.
- Shared library behavior belongs under `src/lib.rs` modules; CLI dispatch
  glue can stay in `src/main.rs` and runner modules.

## Configuration
- Default home is `~/.vivarium`.
- `VIVI_HOME` overrides the home directory and supports `~/...` expansion.
- `config.toml` is general configuration.
- `accounts.toml` contains accounts and credentials and should be mode `600`
  unless `--ignore-permissions` is explicitly used.
- Config is `serde` + `toml`.

## Validation
Before finishing code changes, run the narrowest useful check first, then widen
as risk increases:

- `cargo fmt --check`
- `cargo test --test hygiene`
- `cargo test`
- For feature-gated outbox work: `cargo test --features outbox`

For documentation-only edits, at least inspect links or run a small local
Markdown link scan when the touched file contains links.

## Dependency Policy
Use the dependencies already in `Cargo.toml` when possible. Important current
crates include:

- CLI and runtime: `clap`, `tokio`, `tracing`, `tracing-subscriber`
- Config and errors: `serde`, `serde_json`, `toml`, `thiserror`
- Mail: `async-imap`, `lettre`, `mail-parser`, `mail-builder`, `notify`
- Storage and indexing: `rusqlite`, `sha2`, `hex`
- Proton/direct API: `reqwest`, `proton-srp`, `pgp`, `base64`, `rand`

Do not add a new dependency for small local logic that is already covered by
the standard library or existing crates.

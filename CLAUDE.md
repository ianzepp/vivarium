# Vivarium — Coding Standards

## Project
Local-first, file-native IMAP-to-Maildir email sync tool written in Rust.

## Conventions
- **Edition 2024**, resolver "3", toolchain pinned to 1.93
- **Flat src/ layout**: `module.rs` not `module/mod.rs` until a module genuinely needs submodules
- **clap derive** with `#[derive(Parser)]` + `#[derive(Subcommand)]`
- **Entry point**: `main()` calls `run()` which returns `Result<(), VivariumError>`; main prints error to stderr and exits 1
- **Error handling**: `thiserror` for the library error enum — no `anyhow`
- **No panics** in production code — return Results
- **Small files**: target 200 lines, ceiling 400
- **Short functions**: target 30 lines, ceiling 60
- **Inline tests**: `#[cfg(test)] mod tests` at bottom of files
- **Config**: serde + toml, `~/.config/vivarium/config.toml`, tilde expansion via dirs crate
- **Logging**: `tracing` + `tracing-subscriber`
- **Async**: `tokio` with full features, `#[tokio::main]`

## Dependencies
See Cargo.toml — key crates: async-imap, lettre, maildir, notify, mail-parser, mail-builder, clap, serde, tokio, tracing, thiserror, chrono.

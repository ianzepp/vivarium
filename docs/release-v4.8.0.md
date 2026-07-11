# Vivarium 4.8.0

Vivarium 4.8.0 hardens multi-project mailspace CLI ergonomics for agents and
fleet operators on top of the 4.7 identity-rename surface.

## Highlights

### Global `--project`

- Adds a **global** `--project <ROOT>` flag on `vivi` that applies to
  mailspace commands (`board`, `mailspace`, `mail`, `task`, `need`, `want`).
- Both placements work and fill the same command-local project field:

  ```sh
  vivi board --project /path/to/project --for mind
  vivi --project /path/to/project board --for mind
  ```

- Fixes a common LLM failure mode where agents put `--project` before the
  subcommand and clap rejected it as unknown, even though `board --project`
  already existed.

### `vivi mail list` timestamps and JSON

- Human `vivi mail list` output now includes the message **date** between
  handle and from, matching board-style rows:

  ```text
  <handle>  <date>  <from>  <subject>
  ```

- Adds `--json` on `vivi mail list`, emitting an array of objects with
  `handle`, `date`, `from`, `to`, `subject`, and `role`.
- Improves cheap multi-fleet and operator→mind inbox scans without requiring
  `mail show` for every handle.

### Repo hygiene

- Ignores project-local `.vivi/` fleet/mailspace overlays in git so board
  state is not committed with the source tree.

## Migration

No schema or config migration. Existing `.vivi/` mailspaces and
`mailspace.toml` files continue to work unchanged. Callers that scrape plain
`mail list` text should expect a new date column between handle and from;
prefer `--json` for stable agent parsing.

## Release Checks

Before publishing the tag, run:

```sh
cargo fmt --check
cargo test --test hygiene
cargo test
cargo build --release
target/release/vivi --version
```

This release does not change provider routing, sync, or send paths, so live
provider smoke checks from `docs/release-smoke-checks.md` are optional rather
than required for 4.8.0.

## Publication

GitHub release assets and the Homebrew tap (`ianzepp/homebrew-tap` formula
`vivarium`) are part of this release contract. Crates.io is not.

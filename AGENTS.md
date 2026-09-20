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
- Project mailspaces (`.vivi/mail.sqlite`): tasks, needs, wants, mail, memos,
  roles, **goal path registry** (pointers to on-disk factory/campaign goal
  files), and **executable work graphs** (Mermaid import, ready frontier,
  task-attempt binding)
- Providers: standard IMAP/SMTP, Proton Bridge-style config, and direct Proton API paths

## Agent skill

Agent-facing `vivi` CLI guidance is [`skills/vivi/SKILL.md`](skills/vivi/SKILL.md).
Load that skill for mailspace, role, goal, graph, and email command usage.
Verify live `--help` before exact flags.

## Work Graphs (project mailspace)

Executable topology lives in project-local `mail.sqlite` tables
(`work_graphs`, `work_graph_nodes`, `work_graph_edges`, revisions, events,
attempts). Mermaid is import/export evidence; readiness is derived from
normalized edges + node state.

| Command family | Role |
| --- | --- |
| `vivi graph import` / `apply` | Atomic create / revise from narrow Mermaid `flowchart` |
| `vivi graph show` / `export` | Mermaid topology only |
| `vivi graph ready` | Compact ready/blocked/active/gates frontier (status loops; `counts` line, `--kind` filter) |
| `vivi graph audit [--repair]` | Backlog citizenship check; repair mints/completes/settles drifted items |
| `vivi graph connect` | Post-hoc prerequisite edge between two backlog items |
| `vivi graph complete` / `activate` | Lifecycle receipts; activate binds a task attempt and refuses operator gates |
| `vivi board --graph` | Frontier projection without replacing task/need board items |
| `vivi need bind` | Lowering: bind unit tasks to a need; join completes the need |
| `vivi step [--apply <handle>]` | Manifest of dispatches/exceptions over the `backlog` graph; apply completes settled items |
| `vivi goal add` / `list` / `show` / `drop` | Register goal file paths; board always surfaces them |
| `vivi trace` | **Communication** tree — not work-graph topology |

**Backlog citizenship.** Every `task` / `need` / `want` send mints an open
node in the per-mailspace `backlog` graph (`source_id` = item handle).
`--depends-on` (task/need/want handles, validated before send) becomes a
prerequisite edge (`graph connect` adds one post-hoc). Lifecycle moves keep
node state in step: done moves complete nodes and unlock dependents; reopen
re-locks; `want promote` never changes node state, and wants never dispatch
in `vivi step` before explicit promotion. The `need bind` join completes
the need's node **and** settles its mailbox item (`via=join`); reopening a
bound unit restores both. Node completions write a `step_decision` graph
event in the same transaction
(`via=lifecycle|graph-complete|step-apply|repair|join`). `graph audit`
detects and repairs citizenship drift — including items sent by vivi
binaries too old to mint nodes (check the version in `mailspace status`;
stale PATH-shadowing installs have caused silent untracked items). No
schema beyond the graph tables.

**Judgment provider (shadow screens).** User-level `[judgment]` in
`config.toml` (`provider`/`endpoint`/`model`/`timeout_ms`/`key_cmd`) enables
TypeSafe System One screens on `vivi step --apply` only: one Noul per
`done_when` clause plus a completion-honesty Noul, answers appended to
`.vivi/judgment-corpus.jsonl`. Shadow by design — provider answers never
gate mechanical completions. Auth is exclusively `key_cmd` (`sh -c`,
`password_cmd` semantics; no envvar, no inline key). Absent/failing/timed-out
providers degrade to `judgment=skipped(<class>)`. No read path makes
provider calls.

Invariant: Vivi decides which graph nodes are eligible (ready). The Mind
dispatches. Fleet (`prepare --node` → claim → activate) proves execution.
Do not encode Fleet roles inside Vivi core.

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

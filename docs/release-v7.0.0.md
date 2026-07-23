# Vivarium 7.0.0

Vivarium 7.0.0 makes **executable work graphs** first-class project-local
control-plane state. A Mind can import a Mermaid flowchart, revise it, complete
nodes, bind task attempts, and read the ready frontier from `vivi graph` and
`vivi board --graph`. Fleet remains responsible for dispatch and claim/settle;
Vivi owns topology and readiness.

This is a major-version release because project mailspaces gain a new durable
schema surface (`work_graphs` and related tables, storage schema **3**) and a
new primary CLI command family (`vivi graph`).

## Highlights

### Executable Mermaid work graphs

```sh
# Validate without writing
vivi graph import --code mir-swarm-wave-2 --file wave.mmd --check --json

# Atomic first import (or idempotent re-import of identical source)
vivi graph import --code mir-swarm-wave-2 --file wave.mmd --json

# Additive revision with source-id reconciliation
vivi graph apply mir-swarm-wave-2 --file wave-v2.mmd --json

vivi graph show mir-swarm-wave-2 --json
vivi graph export mir-swarm-wave-2 --include-state

# Lifecycle
vivi graph complete mir-swarm-wave-2:verify --json
vivi graph activate mir-swarm-wave-2:verify --task <task-handle> --json

# Small explicit mutations
vivi graph node add --graph mir-swarm-wave-2 --id u4 --label "G-P-10/U4"
vivi graph edge add --graph mir-swarm-wave-2 --from accept --to u4
```

Import accepts a narrow Mermaid `flowchart` / `graph` profile with `-->` edges.
Vivi assigns immutable handles, stores Mermaid as revision evidence, and derives
ready / blocked from topology plus node state. Completing a node unlocks
successors and emits `node_ready` events; it does not spawn agents.

### Board and watch surfaces

```sh
vivi board --graph --json
vivi board --graph --project /path/to/project --for mind --json

# Graph lifecycle events (with other kinds as needed)
vivi mailspace watch --kinds graph --events node_ready --json
```

`board --graph` adds a `graphs[]` field (text section + JSON) without removing
existing board fields. Each node entry includes lifecycle state, readiness,
blocked-by handles, and successors.

### Authority split (Vivi / Mind / Fleet)

| Concern | Authority |
| --- | --- |
| Planning topology + ready frontier | `vivi graph` (project `mail.sqlite`) |
| Communication history | `vivi trace` |
| Standalone task deps (6.4) | `task send --depends-on` / `task list --blocked` |
| Who to spawn / when | Mind + Fleet (`prepare --node` → claim → settle) |

Task `--depends-on` remains for pairwise task links. Work graphs are the
project-wide executable topology.

### Housekeeping

Production/test boundary extraction: inline unit tests moved to companion
`*_test.rs` files with path-only wiring. Hygiene ratchets now scan
**production-only** sources and enforce zero inline `#[cfg(test)]` modules and
zero `#[test]` attributes in production files.

## Breaking Framing

The major-version bump reflects:

- **New top-level CLI** `vivi graph` (import, apply, show, export, complete,
  activate, node/edge add).
- **Storage schema 2 → 3** on project mailspaces: new tables
  `work_graphs`, `work_graph_revisions`, `work_graph_nodes`, `work_graph_edges`,
  `work_graph_events`, `work_graph_attempts`. Opening a mailspace with a current
  `vivi` upgrades schema automatically.
- **Board JSON growth**: clients that assume a fixed board shape must tolerate
  the optional `graphs[]` field when `--graph` is used.
- **Operational contract**: readiness lives in Vivi; Fleet must not invent
  graph eligibility outside `prepare --node` / claim paths.

## Compatibility

- Existing mail, task, need, want, memo, role, board (without `--graph`), and
  watch flows continue to work.
- Schema upgrade is additive (new tables). No rewrite of messages or blobs.
- Older Vivi binaries that only understand schema 2 should not open a schema-3
  mailspace for write work after upgrade; upgrade operators and agents together.
- `vivi-pty` versions in lockstep (both report **7.0.0**) and continues to ship
  in the same release archives; the Homebrew formula installs both binaries.

## Installation

```sh
# Homebrew
brew upgrade ianzepp/tap/vivarium

# curl installer
curl -fsSL https://raw.githubusercontent.com/ianzepp/vivarium/main/install.sh | bash

# From source
cargo install --path .
cargo install --path crates/vivi-pty
```

## Release Checks

Before publishing the tag, run:

```sh
cargo fmt --check
cargo test --test hygiene
cargo test
cargo test --features outbox
cargo clippy --all-targets -- -D warnings
cargo build --release
cargo build --release -p vivi-pty
target/release/vivi --version
target/release/vivi-pty --version
```

Live provider smoke (`docs/release-smoke-checks.md`) is optional for this cut:
7.0.0 does not change IMAP/SMTP or Proton transport paths.

# Vivarium 7.1.0

Vivarium 7.1.0 tightens the **agent-facing control plane** introduced in 7.0.
Work-graph topology is Mermaid-only, status loops use a compact ready frontier,
lifecycle actions return receipts instead of full topology dumps, and large
stdout exports refuse without an explicit confirm (or a file path).

This is a minor-version release: no new storage schema. Callers that treated
`vivi graph show --json` as a full topology dump, or that streamed unbounded
dump JSON into context, must update.

## Highlights

### Mermaid-only topology (`graph show` / `export`)

```sh
vivi graph show mir-swarm-wave-2
vivi graph show mir-swarm-wave-2 --include-state
vivi graph export mir-swarm-wave-2 --include-state
```

`graph show` no longer offers a JSON topology dump. Topology is always Mermaid.
Add `--include-state` for readiness classes in the diagram. Use `graph ready`
when you need a machine-readable frontier.

### Compact ready frontier (`graph ready`)

```sh
vivi graph ready mir-swarm-wave-2
vivi graph ready mir-swarm-wave-2 --json
vivi graph ready --json   # all graphs in the mailspace
```

Status loops should poll `graph ready` (or `board --graph`), not re-fetch full
show/export output. Ready / blocked / active lists stay small enough for agent
context.

### Compact action receipts

```sh
vivi graph import --code mir-swarm-wave-2 --file wave.mmd --json
vivi graph apply mir-swarm-wave-2 --file wave-v2.mmd --json
vivi graph complete mir-swarm-wave-2:verify --json
vivi graph activate mir-swarm-wave-2:verify --task <task-handle> --json
```

Import, apply, complete, activate, and node/edge mutations emit **receipts**
(counts, roots/ready handles, revision metadata) rather than full node/edge
topology. Topology stays on the Mermaid path.

### Large stdout dumps require confirmation

```sh
# Refuses when the dump exceeds 25 records or 64 KiB on stdout
vivi task dump --for cto --status all

# Explicit confirm, or write to a file
vivi task dump --for cto --status all --confirm-large
vivi task dump --for cto --status all --output audit-tasks.json
```

JSON dumps no longer bypass the size guard. Oversized graph-related JSON
refusals point agents at Mermaid `graph show` / `graph ready` instead of
pulling topology into context.

## Breaking / migration notes

| Before (7.0.x agents) | After (7.1.0) |
| --- | --- |
| `vivi graph show <code> --json` for topology | `vivi graph show <code>` (Mermaid) |
| Ready/blocked via show JSON | `vivi graph ready <code> --json` |
| Import/complete/activate return full show JSON | Compact receipts only |
| Large dump JSON always printed | Needs `--confirm-large` or `--output` |

Fleet / Mind skills that still parse full graph JSON from show/import should
switch to Mermaid topology + `ready` / receipts. README and `Agents.md` already
document the split.

## Compatibility

- Storage schema remains **3** (work graph tables from 7.0). No migration.
- Existing mail, task, need, want, memo, role, and board flows are unchanged
  except dump size gating and graph CLI surfaces above.
- `vivi-pty` versions in lockstep (both report **7.1.0**) and continues to ship
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
7.1.0 does not change IMAP/SMTP or Proton transport paths.

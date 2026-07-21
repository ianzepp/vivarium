# Vivarium 6.3.0

Vivarium 6.3.0 adds **`vivi trace`**, a local-mailspace command that reconstructs
the cross-role communication tree around any handle (task, want, need, mail, or
memo).

## Highlights

### `vivi trace <handle>`

Given any resolvable local mailspace handle, `vivi trace` builds a bounded tree of
related messages across fleet roles and folders:

```sh
vivi trace <handle>
vivi trace <handle> --json --max-depth 5 --limit 100
```

The tree includes:

- **Captured** edges from `In-Reply-To` / `References` reply links.
- **Event** edges from `task from` lifecycle events (`active_tasks=<handle>`).
- **Inferred** edges from body handle citations and stripped subject matching.
- **Copy** collapse: sender `sent` and recipient `inbox` / `tasks` / `wants` /
  `done` copies sharing the same `content_id` are rendered as one logical node
  with the set of `(account, role)` copies listed.

Text output prints a header, per-node metadata, copy list, and labeled edges:

```text
trace b8f17e97 (4 node(s))

## 2026-07-14T10:45:28+00:00 - b8f17e97 [task]
subject: [P2][parser][tree-sitter-faber] parse multiline braced annotations structurally
copies:
  mind sent msg_b8f17e9743091bb1cfab14dd
  hand-2 done msg_d5e0bcf4e5fd4d8cacc6c7af
edges:
  ancestor -> 096bc357 (captured)
  descendant -> c6496a0e (captured)
```

JSON output contains a stable `TraceGraph` with `seed`, `nodes`, and `edges`
including `target`, `source`, and `direction`.

## Compatibility

- No store migration. `vivi trace` reads existing `messages`, `blobs`,
  `mailspace_links`, and `mailspace_events` tables.
- Existing CLI commands and storage schema are unchanged.
- `vivi-pty` versions in lockstep (both report 6.3.0) and continues to ship in
  the same release archives; the Homebrew formula installs both binaries.

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

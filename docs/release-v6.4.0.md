# Vivarium 6.4.0

Vivarium 6.4.0 is a fleet operations release. It adds structured task lifecycle
fields (dependencies and close metadata), agent mutation planning with execution,
memo keyword search, and a mailspace description for fleet-level charters.

## Highlights

### Task dependencies (`--depends-on`, `--blocked`, `--blocking`)

Tasks can now declare structured dependencies on other tasks instead of relying
on body prose:

```sh
vivi task send --to hand-3 --depends-on <handle> --depends-on <handle> ...

# Query blocked tasks (open tasks with unmet dependencies)
vivi task list --for hand-3 --blocked

# Query what depends on a given task
vivi task list --for hand-3 --blocking <handle>
```

Dependencies are persisted as repeated `X-Vivi-Depends-On` headers on the task
message. The board and sensors can surface blocked tasks without body parsing.

### Task close verdict, repo, and tip SHA

Structured fields on `vivi task done` replace prose scavenging:

```sh
vivi task done <handle> --for auditor-1 --verdict clean_pass
vivi task done <handle> --for auditor-1 --verdict residual --note 'P2: ...'
vivi task done <handle> --for hand-2 --repo examples --tip e968cc3 --repo hosts --tip 0de5c36
```

Close metadata is stored as key-value pairs in the `mailspace_item_metadata`
table, making it queryable without body parsing.

### Memo keyword search

`vivi memo search` filters memos by subject and body keywords, replacing the
previous requirement to dump all memos for selective retrieval:

```sh
vivi memo search --for mind "railway deploy"
vivi memo search --for mind --subject "ACCEPT*"
vivi memo search --for mind --json "lowering law"
```

### Agent mutation commands (`--execute`)

Agent commands for archive, delete, move, and flag now support a plan-review-execute
loop. Without `--execute`, the command produces a dry-run mutation plan (JSON audit
record). With `--execute`, it performs the live mutation:

```sh
# Plan (dry-run JSON output)
vivi agent archive <handle> --json
vivi agent delete <handle> --expunge --confirm
vivi agent move <handle> trash
vivi agent flag <handle> --read

# Execute after review
vivi agent archive <handle> --execute
vivi agent delete <handle> --expunge --confirm --execute
vivi agent move <handle> trash --execute
vivi agent flag <handle> --star --execute --json
```

### Mailspace description

A fleet-level charter field analogous to `vivi role charter show`:

```sh
vivi mailspace description --set 'Faber language work. Ship features over purity.'
vivi mailspace description                   # show current
vivi mailspace status                        # includes description when present
```

### Bug fix: subagent PID liveness

Role PID-based liveness now correctly suppresses false "dead" signals for
subagent harnesses where `$$` refers to an ephemeral tool shell rather than
the agent process. Subagent fleet Hands no longer produce spurious
`state=stopped` board signals every cycle.

## Compatibility

- No store migration. New fields use existing `mailspace_item_metadata` and
  header storage.
- Existing CLI commands and storage schema are unchanged.
- `vivi-pty` versions in lockstep (both report 6.4.0) and continues to ship in
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

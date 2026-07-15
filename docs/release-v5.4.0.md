# Vivarium 5.4.0

Vivarium 5.4.0 adds explicit account mutation capabilities for safe remote
mailbox operations. Existing accounts remain full-write by default, while
read-only and archive/read-mostly policies make remote side effects explicit
and enforceable.

## Highlights

### Explicit account mutation policies

Accounts can declare one of three policies in `accounts.toml`:

| Policy | Configuration | Remote behavior |
|---|---|---|
| **Full-write** (default) | `policy = "full-write"` | Existing behavior: archive, moves, trash/delete, expunge, flags, and send |
| **Read-only** | `policy = "read-only"` | Sync, read, search, and show only |
| **Archive** | `policy = "archive"` | Archive, non-trash moves, and flags; denies trash/delete, expunge, and send |

```toml
[[accounts]]
name = "vault"
email = "vault@proton.me"
# ... provider settings ...
policy = "read-only"
```

Remote side effects are authorized from the selected account's capabilities at
execution time. Command names, queue provenance, and provider folder aliases
are not authorization. Enqueue admission and queue execution both enforce the
policy, so stale or manually constructed queued operations cannot bypass it.
`vivi doctor` reports the effective policy in both text and JSON output.

Trash aliases—including `trash`, `deleted`, and provider-specific trash
folders—are normalized before classification, including mixed-case folder
names. Local project mailspace operations remain separate and unrestricted by
external account policy.

### Closed remote-write bypasses

This release closes the policy bypasses found during post-main review:

- `compose`/`reply --append-remote` now requires the account's explicit
  capability before IMAP Drafts APPEND.
- Feature-gated outbox auto-send now requires the account's send capability
  before SMTP dispatch.
- Direct execution and queue execution converge through the same authorization
  seam, including persisted stale queue items.

Denied operations fail with explicit policy errors before provider calls. The
regression suite covers real queue-run execution, append-remote denial, outbox
send denial, mixed-case trash aliases, and local draft behavior.

## Compatibility

Existing account configurations default to `full-write`; no migration is
needed. `vivi-pty` remains version 1.0.0 and ships alongside `vivi` in release
artifacts.

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

# Vivarium 5.5.0

Vivarium 5.5.0 adds an inbound-only IMAP event source, safe local document
rendering, and a Kimi Code driver for `vivi-pty`, alongside a path-safety
hardening pass over sync reset and SMTP send correctness fixes.

## Highlights

### Inbound-only IMAP watch

`vivi` gains an inbound-only IMAP event source that watches mailboxes without
mutating them. Sender addresses are emitted in normalized form, and the sender
authorization boundary is documented so follow-on automation can decide which
senders may trigger work.

### Safe local document rendering

A new local document renderer converts Markdown and related documents with
image validation: local image targets must exist, be regular files, and stay
inside the document's resource root. Tool version output is scrubbed of home
directory paths before display.

### Kimi Code driver for vivi-pty

`vivi-pty` now ships a Kimi Code driver alongside Codex, Pi, OpenCode, and
Grok. Its markers are grounded in live TUI captures: the boxed `>` composer
for waiting state, the moon-phase turn spinner for running state, and the
numbered approval panel. Interruption sends Escape rather than Ctrl-C, because
Ctrl-C is Kimi Code's idle exit gesture; approve confirms with Enter and
reject sends Escape. The driver shares the normalized state, evidence,
capability, and conformance surface with the other built-in drivers.

`vivi-pty` also gains `session.remove`, which drops a session id without a
tombstone so a new `session.start` can rebind command and cwd for the same
identity.

### Sync reset path safety

`vivi sync reset` now rejects custom home, repo, and cwd targets that are
dangerous ancestors of system directories, existing symlink escapes, and
managed-path containment violations, validating canonicalized paths before any
deletion. Managed-home test fixtures are temp-only.

### Send and storage correctness

- SMTP sends strip Bcc headers from DATA while preserving envelope recipients.
- Local mailspace delivery is atomic, with an empty-recipient guard.
- Local send reconciliation durability is hardened.
- Outbox policy is checked before claiming a file, preventing orphan claims.

## Compatibility

No migration is needed. `vivi-pty` now versions in lockstep with `vivi` (both
report 5.5.0) and continues to ship in the same release archives; the Homebrew
formula installs both binaries. Daemon-level `session.submit` remains a
Codex-only acknowledged flow; `session.interrupt` and `session.restart` run
through whichever driver a session selected.

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

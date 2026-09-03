# Vivarium 8.1.0

Vivarium 8.1.0 makes default `vivi board` usable on large project mailspaces
and exports closed work into a configured historical archive.

On a 66k-message, 136-identity mailspace, default `vivi board` dropped from
about 29 seconds to under a second. Roles without cadence no longer scan
outbound mail for schedule health. The board reuses one storage connection.
Latest-from prefers an exact `from_addr` match before a display-name `LIKE`.

This is a minor-version release. Storage schema becomes **6** (additive
`from_addr`/`date` index). Existing mailspaces migrate on first open. Absorb
backfill from 8.0.0 does not re-run.

It also ships the unpublished archive fix: `vivi mailspace archive export`
writes done tasks, needs, and wants, not only absorbed rows.

## Highlights

### Fast default board

```sh
vivi board --project <root>
vivi board --project <root> --for mind
```

Schedule reports still appear for roles with `cadence`. Identities without
cadence stay `schedule.state = none` and skip the last-signal query. Board
item handles are unchanged.

### Archive export of done work

```sh
vivi mailspace archive export --project <root> [--json]
```

Export includes `task` / `need` / `want` rows in `done`, even when they were
never absorbed. Absorbed records of every kind still export. Inbox/sent mail
that is not absorbed still does not.

## Breaking / migration notes

| Before (8.0.0) | After (8.1.0) |
| --- | --- |
| Default `vivi board` scanned last outbound mail for every identity | Only identities with cadence pay that query |
| Storage schema **5** | Schema **6**; index `message_metadata(from_addr, date)` on first open |
| `archive export` skipped non-absorbed done work | Done tasks/needs/wants export alongside absorbed records |

No dump/restore. First open of an 8.0.0 mailspace creates the index and
writes schema version 6. Board JSON field names are unchanged.

## Compatibility

- Storage schema becomes **6**. Additive index only.
- `vivi-pty` versions in lockstep (both report **8.1.0**) and continues to
  ship in the same release archives; the Homebrew formula installs both
  binaries.
- IMAP/SMTP and Proton transport paths are unchanged.
- Absorb, roles, goals, work graphs, and `vivi trace` are unchanged.

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

Live provider smoke (`docs/release-smoke-checks.md`) is optional for this
cut: 8.1.0 does not change IMAP/SMTP or Proton transport paths.

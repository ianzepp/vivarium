# Vivarium 7.3.0

Vivarium 7.3.0 makes project-mailspace list, watch, and dump flags match
what agents already type. `--from` and `--to` work as header filters on
list, kind-specific watch no longer pretends to accept `--kinds`, and
`want dump` uses the same work-dump surface as task and need.

This is a minor-version release. No mailspace schema change.

It also ships the unpublished 7.2.1 board work: role capacity on the
board, and `ACTION REQUIRED` when a schedule is overdue.

## Highlights

### List `--from` / `--to` (mail, task, need, want)

```sh
vivi mail list --from mind
vivi mail list --to hand
vivi mail list --for hand --from mind
vivi task list --from ceo --status all
vivi need list --to cto
vivi want list --from ceo --to hand --status all
```

`--from` matches `From`. `--to` matches `To` or `Cc`. Known roles resolve
to `name@<project>.local`; anything else is a case-insensitive substring
(same as dump). `--for` is still mailbox scope and is optional when
`--from` or `--to` is present. At least one of the three is required.

`task list` and `need list` now accept `--status all`, matching want list
and work dump. `task list --blocked` / `--blocking` still require `--for`.

### Honest watch and want dump

```sh
# Kind is implied by the verb. --kinds is rejected here.
vivi mail watch --for mind --match-from operator --once --json

# Mix kinds only on mailspace watch
vivi mailspace watch --for mind --kinds mail,task,need --once --json

# Work-dump flags, not mail folder/absorb flags
vivi want dump --from ceo --status all --json
```

`vivi mail|task|need|want watch --kinds …` used to parse and ignore the
flag. It is now a clap error. `vivi want dump --folder` and absorb
`--status absorbed` no longer parse.

### Board capacity and overdue action (from unpublished 7.2.1)

Board identities include configured role `model` and `thinking` in text
and JSON. An `overdue` schedule sets `schedule.action_required` to `true`
and prints `ACTION REQUIRED`. A `due` schedule stays advisory.

## Breaking / migration notes

| Before (7.2.0) | After (7.3.0) |
| --- | --- |
| `vivi mail list` required `--for` only | `--for` optional if `--from` or `--to` is set |
| `vivi task/need/want list --from mind` failed | Header filter, same as mail list |
| `vivi task list --status all` failed | Lists open and done tasks |
| `vivi mail watch --kinds task` parsed, ignored `--kinds` | Clap error. Use `mailspace watch --kinds` |
| `vivi want dump --folder inbox` / `--status absorbed` parsed, ignored | Clap error. Use `--status open\|done\|all` |

Agents that copied dump flags onto `want dump`, or `--kinds` onto
kind-specific watch, will now get a visible error instead of a silent
no-op. Update those invocations.

## Compatibility

- No storage schema change (remains **4**).
- `vivi-pty` versions in lockstep (both report **7.3.0**) and continues
  to ship in the same release archives; the Homebrew formula installs
  both binaries.
- IMAP/SMTP and Proton transport paths are unchanged.
- Memo list still requires `--for` and has no `--from`/`--to`.
- Watch sender filter remains `--match-from`, not `--from`.

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
cut: 7.3.0 does not change IMAP/SMTP or Proton transport paths.

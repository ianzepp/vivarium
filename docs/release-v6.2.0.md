# Vivarium 6.2.0

Vivarium 6.2.0 adds **role cadence and schedule health**: roles can declare a
desired maximum silence interval, and `role status` / `board` report whether the
seat is on track, due, or overdue based on the latest outbound mailspace signal.

## Highlights

### Cadence on roles

Optional schedule interval on any role (`s` / `m` / `h`):

```sh
vivi role add head-ceo --kind head --cadence 15m --project <root>
vivi role set head-ceo --cadence 30m --project <root>
vivi role set head-ceo --clear-cadence --project <root>
```

Cadence is general-purpose (not kind-gated). In practice it is most often used
for heads, stewards, and similar advisory seats.

### Schedule health

`vivi role status` and `vivi board` derive schedule state from the age of the
role's latest **outbound** message (name + aliases). Memos and lifecycle events
do not count.

| State | Meaning |
| --- | --- |
| `none` | No cadence configured |
| `never` | Cadence set, no outbound signal yet |
| `ok` | Last signal younger than one cadence (+10% grace) |
| `due` | Silence between one and two cadences |
| `overdue` | Silence at or beyond two cadences |

Schedule is **advisory visibility** for the Mind — not an execution contract.
Process liveness remains separate: a periodic seat may be `not_set`/`dead` while
schedule is healthy.

```sh
vivi role status head-ceo --project <root> [--json]
vivi board --project <root> [--json]
```

Board JSON always includes a `schedule` block per identity; text output prints a
schedule line when state is not `none`.

### Shared duration parser

Human intervals (`30s`, `5m`, `1h`, bare seconds) share one parser used by role
cadence and `sync-events --interval`.

## Compatibility

- No store migration. Optional `cadence` loads as unset on older mailspaces.
- Existing role, board, task, and mail commands keep their contracts; new
  surfaces are additive (`role` cadence fields, `schedule` on status/board).
- `vivi-pty` versions in lockstep (both report 6.2.0) and continues to ship in
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

# Vivarium 6.1.0

Vivarium 6.1.0 adds **role process binding and live status**: roles can
self-register a `pid`/`host` binding, and operators or fleet Minds can query
liveness by role name alone. The board gains a scan mode for the same signal
across seats.

## Highlights

### Process binding on roles

A role self-registers its live process at boot (PID-file semantics):

```sh
vivi role set hand-1 --pid $$ --project <root>
# host defaults to the local hostname when --pid is set
vivi role set hand-1 --clear-pid --project <root>
```

| Field | Meaning |
| --- | --- |
| `pid` | Process id occupying the seat this run |
| `host` | Host where that pid lives (cross-host honesty) |

Only the binding is stored. Liveness is observed at query time and never written
back onto the role row.

### `vivi role status`

```sh
vivi role status hand-1 --project <root> [--json]
```

Reports `state` (`alive`, `zombie`, `dead`, `sleep`, `not_set`, `remote`,
`unknown`), `running`, process `name`, memory, uptime, and approximate CPU
percent. If stored `host` is not the local host, the probe does not invent local
process truth: `state = remote`.

### Board process scan

```sh
vivi board --process --project /path/to/project [--json]
vivi board --process --for hand-1 --project /path/to/project
```

Adds a lightweight process block per role (or one role with `--for`). Board
scans do not sleep for CPU samples (`cpu_percent` is null there); use
`vivi role status <name>` for precise CPU. No binding reads as `not_set` — the
"available to assign" signal.

### Role add correctness

`vivi role add` now errors on duplicate names and applies all flags atomically
instead of partial writes.

### Design goal

Factory goal: [`docs/mailspace-role-pid-status-goal.md`](mailspace-role-pid-status-goal.md).

## Compatibility

- No store migration. Optional `pid` / `host` fields load as unset on older
  mailspaces.
- Existing role, board, task, and mail commands keep their contracts; new
  surfaces are additive (`role status`, `role set --pid` / `--host` /
  `--clear-pid` / `--clear-host`, `board --process`).
- `vivi-pty` versions in lockstep (both report 6.1.0) and continues to ship in
  the same release archives; the Homebrew formula installs both binaries.
- Pedantic Clippy is denied on the library; this is a quality gate, not a CLI
  break.

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

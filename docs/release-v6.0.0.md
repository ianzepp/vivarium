# Vivarium 6.0.0

Vivarium 6.0.0 makes **roles** first-class mailspace seats: a new top-level
`vivi role` command owns kind, labels, status, harness, provider/model/thinking,
and a standing **charter** prompt. This is a major-version release because the
CLI surface gains a new primary command and seats become durable operational
truth for multi-agent fleets (especially sub-agent spawns), not only mailbox
names.

## Highlights

### First-class `vivi role`

```sh
vivi role list [--json]
vivi role show <name> [--json]
vivi role add <name> --kind head --harness subagent --label executive
vivi role set <name> --provider zai --model glm-5.2 --thinking high
vivi role set <name> --harness subagent
vivi role rename <old> <new>
vivi role charter show <name>
vivi role charter set <name> --file personas/ceo.md
# also: --body '...' | --body-file PATH
```

| Field | Meaning |
| --- | --- |
| `name` | Mailbox local-part (`hand-1`, `head-ceo`) |
| `kind` | Process class (`hand`, `head`, `mind`, `operator`, `steward`, or freeform) |
| `status` | Lifecycle (`active`, `parked`, `retired`, or freeform); default `active` |
| `labels` | Freeform slugs (`auditor`, `floater`, …) |
| `harness` | Execution home (`subagent`, `tmux`, `vivi_pty`, …) |
| `provider` / `model` / `thinking` | Desired capacity (not process liveness) |
| `charter` | Standing seat prompt (`.vivi/charters/<name>.md`) |
| `address` | Derived `{name}@{mailspace}.local` |

Mutations are **one role per command**. Bulk capacity flips stay in shell loops
or helper scripts.

### Charter and pointer-style boot

Charters are standing seat definitions, not assignments. Parent agents should
pass pointers instead of pasting persona text:

```text
You are fleet role head-ceo.
Load charter: vivi role charter show head-ceo --project <root>
Load task:    vivi task show <handle> --project <root>
```

### Subagent harness preference

Preferred harness vocabulary includes **`subagent`**: run in the parent TUI /
spawn a child agent. Long-lived `tmux` and `vivi_pty` remain valid harness
values. Updating capacity or harness does not require killing panes as the way
the change is stored.

### Design goal

Factory goal: [`docs/mailspace-role-goal.md`](mailspace-role-goal.md).

## Breaking Framing

The major-version bump reflects:

- **New top-level CLI command** `vivi role` (and `role --help` surface).
- **Seat essence moves into the mailspace**: kind, capacity, harness, and
  charter are first-class Vivi fields. Fleet overlays may still dual-write for
  a time, but Vivi is the intended authority for those fields.
- **Unknown-seat error wording** now says **role** (e.g. “unknown local role”)
  and points agents at `vivi role add`. Address resolution and delivery
  behavior are otherwise unchanged.
- **`mailspace identity`** remains for thin roster add/list/rename; prefer
  `vivi role` for new operational work.

## Compatibility

- Existing `[[identities]]` in `mailspace.toml` load as roles with
  `status = active` and empty optional fields. No store migration is required.
- Mail, task, need, want, memo, board, and watch commands continue to use the
  same name/address resolution.
- `vivi-pty` versions in lockstep (both report 6.0.0) and continues to ship in
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

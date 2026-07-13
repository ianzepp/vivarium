# Vivarium 5.1.0

Vivarium 5.1.0 adds a fifth work kind — `memo` — to Vivi's project-local
mailspace. A memo is a structured, durable record of a role's own observation
or reasoning that persists across sessions. Unlike tasks, needs, wants, and
mail, a memo implies no obligation: no work to do, no decision to request, no
communication to deliver. It is pure context preservation.

## Highlights

### `memo` kind (new)

Roles such as `head-ceo` can now save memos for later reference using a
`save` verb (not `send`), signaling persistence rather than communication.
Memos live in a dedicated `memos` folder role and are excluded from the
actionable bag — `vivi board` does not include them. They have no lifecycle
operations (`done`, `promote`, `reopen`); `save` and `delete` are the only
state transitions.

```sh
vivi memo save  --for head-ceo --subject 'auth complexity trend' --body '...'
vivi memo list  --for head-ceo
vivi memo show  <handle>
vivi memo dump  --for head-ceo
vivi memo delete --for head-ceo <handle>
```

Key design decisions:

- **`save` not `send`**: removes the communication frame entirely; an LLM
  seeing `memo save` records context rather than addressing a recipient.
- **`--for` is required on all subcommands except `show`**: the handle is
  globally unique. `dump` requires `--for` explicitly so LLMs don't
  accidentally dump all identities' memos.
- **No `--to`/`--from`**: a memo has no sender or recipient — `--for` is the
  sole identity axis.
- **Mind has custodial pruning authority**: memos are durable but not
  permanent. Mind may proactively delete memos it judges stale or irrelevant,
  like IT cleaning up a shared network drive.
- **Memo-as-memory guidance**: roles are encouraged to review their memos at
  session start and save memos when they learn something worth carrying
  forward.

`vivi mailspace status` now includes a `memos open` column.

### Mailspace status

- Added `memos_open` count to per-identity and total status output.

## Compatibility

No breaking changes. Existing mail, task, need, want, and mailspace commands
are unchanged. No migration is needed.

`vivi-pty` is unchanged at version 1.0.0 and continues to ship alongside
`vivi` in the same release artifacts.

## Installation

```sh
# Homebrew (updates both binaries)
brew upgrade ianzepp/tap/vivarium

# curl installer (installs both vivi and vivi-pty)
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
cargo test --workspace
cargo build --release
target/release/vivi --version
target/release/vivi-pty --version
```

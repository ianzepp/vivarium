# Write Phase 03: Mutation CLI And Safety

## Interpreted Phase Problem

Phase 02 added safe low-level IMAP mutation primitives, but the default CLI still
has a local-only `archive` command and no scriptable safety surface for agents.
This phase exposes mutation commands that can preview remote writes, execute
remote-first mutations, reconcile local state only after success, and write local
audit records.

## Normalized Phase Spec

### Goal

Expose safe, scriptable mutation commands without giving agents a foot-gun.

### Inputs

- Phase 00 catalog remote identity lookup.
- Phase 01 folder and capability resolution.
- Phase 02 IMAP mutation primitives.
- Existing CLI command structure and Maildir store.

### Expected Outputs

- `vivi archive <handle>` performs a remote archive then updates the local mirror.
- `vivi delete <handle> --trash` is the default delete behavior.
- `vivi delete <handle> --expunge --confirm` is required for hard delete.
- `vivi move <handle> <folder>` moves to a supported local/remote folder role.
- `vivi flag <handle> --read|--unread|--star|--unstar` mutates flags.
- Shared `--dry-run` and `--json` output for agent-safe planning.
- Confirmation behavior for destructive hard expunge.
- Local Maildir/catalog reconciliation only after remote success.
- Mutation audit records under the local Vivi state directory.
- Tests for CLI parsing, dry-run JSON, confirmation, local mirror behavior, and
  audit record shape.

### Out Of Scope

- Offline retry queue.
- SMTP sending.
- Arbitrary provider labels.
- Live mutation against real mail unless a disposable remote fixture is present.

## Repo-Aware Phase Baseline

- The current `Command::Archive` path in `src/main.rs` only calls
  `MailStore::move_message`, so it can desynchronize the local mirror from the
  remote mailbox.
- `src/imap/mutate.rs` owns remote mutation execution and already returns
  reconciliation intent.
- `Catalog::remote_reference` resolves a catalog handle to account/mailbox/UID
  state, but the current user-visible CLI still often exposes Maildir message
  IDs. Phase 03 should accept the catalog handle path and preserve practical
  lookup for existing Maildir IDs where the catalog can disambiguate them.
- `MailStore` currently models Inbox, Archive, Sent, Drafts, and outbox. Trash
  must become a first-class local mirror folder for default delete behavior.
- The repository has Rust hygiene gates: files over 400 lines or functions over
  60 lines fail tests.

## Stage Graph

1. CLI contract
   - Add archive/delete/move/flag arguments.
   - Add `--dry-run`, `--json`, and hard-expunge `--confirm` handling.

2. Planning and audit model
   - Build a JSON-safe mutation plan from account, handle, remote identity,
     folder resolution, and capability discovery.
   - Write mutation audit records for dry-run, executed, and failed attempts.

3. Remote execution and local reconciliation
   - Execute IMAP mutation first.
   - Move/remove/update the local Maildir mirror and catalog only after remote
     success.
   - Keep hard-expunge local removal explicit.

4. Tests and gates
   - Unit-test parser behavior and dry-run plan JSON.
   - Unit-test reconciliation and audit records without touching a live account.
   - Run `cargo fmt --check`, `cargo test`, and clippy.

## Checkpoint Target

The mutation CLI can preview, execute, audit, and locally reconcile safe remote
writes on the Pharos-backed account. Without a disposable remote fixture, the
execution checkpoint may be closed with unit tests and dry-run JSON only; no real
mail may be mutated.

## Safety Stop

Do not run non-dry-run mutation commands against the real account unless the
target is a disposable message fixture. If no fixture is available, validate
execution through local unit tests and dry-run CLI behavior only.

## Phase Checkpoint

### Delivered Outputs

- Replaced the local-only archive dispatch with a remote-first mutation command
  path.
- Added `vivi delete`, `vivi move`, and `vivi flag` command surfaces.
- Added shared `--dry-run` and `--json` planning/result output for mutation
  commands.
- Added `--confirm` enforcement for non-dry-run hard expunge.
- Added `src/mutation_command.rs` and `src/mutation_runner.rs` for planning,
  audit records, remote execution wiring, and local reconciliation.
- Added local Trash Maildir support for default delete-to-trash behavior.
- Added local reconciliation for move/trash/archive, hard expunge, read/unread,
  and star/unstar.
- Added audit JSONL records under `.vivarium/audit/mutations.jsonl` for planned,
  executed, and failed mutation attempts.

### Correctness Pass

- Checked remote-first ordering: non-dry-run commands execute IMAP first, then
  mutate the local Maildir/catalog mirror only after remote success.
- Fixed a catalog correctness bug found during review: `remove_entry` now checks
  account ownership before removing a handle, so a same-handle entry for another
  account is not deleted.
- Fixed exact remote-folder move planning: `vivi move <handle> "All Mail"` can
  resolve a configured remote role name back to the supported local archive
  mirror.
- Checked hard-expunge safety: non-dry-run expunge fails before remote discovery
  unless `--confirm` is present.
- Checked local flag reconciliation: Maildir `S` and `F` flags are updated after
  remote flag success and catalog remote identity is retained.

### Verification Run

- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `git diff --check`
- `cargo run -- --help`
- `cargo run -- archive --help`
- `cargo run -- delete --help`
- `cargo run -- move --help`
- `cargo run -- flag --help`

### Poker Face

- Self estimate: 92%.
- Evaluator mode: self-contained independent pass.
- Evaluator estimate: 90%.
- Largest remaining gap: no live non-dry-run Pharos mutation was executed because
  no disposable message fixture was selected for this phase.
- Verdict: cleared for Phase 03 completion.

### Gate Result

PASS. The CLI can plan, JSON-preview, execute through the Phase 02 mutation
primitives, audit, and locally reconcile mutation outcomes. Live mutation against
the real account was intentionally skipped under the safety stop; provider smoke
validation should use a disposable fixture in a later workflow phase.

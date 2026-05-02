# Write Phase 07: Provider Labels And Advanced Mailbox Semantics

## Interpreted Phase Problem

Vivi now has safe folder moves, flags, drafts, sends, and an agent approval
surface. Labels are provider-specific and should not be conflated with standard
IMAP folders. This phase makes that boundary explicit and provides a command
surface that can report whether label mutations are supported.

## Normalized Phase Spec

### Goal

Expose provider label semantics conservatively: support labels only where the
provider semantics are explicit, otherwise return a clear unsupported result and
point users back to folder moves.

### Inputs

- Phase 01 folder/capability discovery.
- Phase 02/03 mutation primitives and folder move support.
- Account provider and `label_roots` config.

### Expected Outputs

- Label support/reporting command.
- Label apply/remove command that plans or reports unsupported behavior.
- Proton Bridge behavior documented as folder-oriented in the current Vivi
  surface.
- Gmail label behavior scoped separately from standard IMAP folders.
- Tests for provider mapping and unsupported-label behavior.

### Out Of Scope

- Provider private APIs.
- Mailbox rules engine.
- Autonomous labeling.
- Implementing Gmail `X-GM-LABELS` mutation.

## Repo-Aware Phase Baseline

- `vivi move` supports the configured folder roles only.
- `label_roots` exists in account config and folder discovery output, but no
  command consumes it.
- Mutation primitives operate through standard IMAP MOVE/COPY/STORE paths.
- No provider-specific label mutation primitive exists.

## Stage Graph

1. Label semantics module
   - Resolve provider support from account provider and label roots.
   - Keep standard IMAP and Proton Bridge label mutation unsupported.
   - Scope Gmail labels separately and report the missing extension support.

2. CLI surface
   - Add `vivi labels` for support introspection.
   - Add `vivi label <handle> --add|--remove <label>` for plan/error behavior.

3. Dispatch behavior
   - `--dry-run` produces a JSON/text unsupported plan without remote writes.
   - Non-dry-run returns a clear unsupported error unless a provider backend is
     implemented.

4. Tests and gates
   - Provider mapping tests.
   - Unsupported-label behavior tests.
   - Parser/help checks and repo hygiene.

## Checkpoint Target

Vivi explains clearly why the current account only supports folder moves for
folder-like organization, while preserving a future command slot for real label
backends.

## Safety Stop

Do not execute provider-specific label mutations against live mail during this
phase.

## Delivered Outputs

- Added `vivi labels` to report provider label support for the selected account.
- Added `vivi label <handle> --add|--remove <label>`.
- Dry-run or JSON label operations return an unsupported plan instead of
  mutating remote state.
- Non-dry-run label operations return a clear unsupported error until a provider
  backend is implemented.
- Proton Bridge is documented in code as folder-move-only for the current Vivi
  write surface.
- Gmail is scoped separately as requiring future provider-specific
  `X-GM-LABELS` work, not standard folder moves.
- The previous `AgentCommand` enum was moved out of `cli.rs` to keep CLI source
  under hygiene limits as label commands were added.

## Correctness Pass

- Label operations do not call IMAP mutation primitives and cannot silently
  relabel live mail.
- Standard IMAP, Proton Bridge, and Gmail each produce distinct support modes.
- Configured `label_roots` are surfaced in support JSON but do not imply support
  for provider label mutation.
- Live folder probing was not executed in this phase; the command surface is
  ready for `vivi folders --json` evidence when a controlled account is chosen.

## Verification

- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo run -- --help`
- `cargo run -- labels --help`
- `cargo run -- label --help`
- `git diff --check`

## Poker Face Check

- Completion score: 88%.
- Largest gap: live Proton Bridge label probing was not run; provider behavior
  is represented conservatively as unsupported/folder-oriented.
- Gate result: PASS for safe provider semantics and unsupported-label behavior.

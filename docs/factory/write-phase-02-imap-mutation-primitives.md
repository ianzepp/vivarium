# Write Phase 02: IMAP Mutation Primitives

## Interpreted Phase Problem

Vivi now has durable remote identity and read-only folder/capability discovery.
The next slice is a low-level mutation layer that can perform exactly one safe
remote write against a resolved remote reference and report what local
reconciliation must do afterward.

## Normalized Phase Spec

### Goal

Add low-level IMAP mutation primitives for folder moves, safe trashing, hard
expunge, and common flag changes.

### Inputs

- Phase 00 remote identity in the catalog.
- Phase 01 resolved folder and capability discovery.
- Existing authenticated IMAP transport.

### Expected Outputs

- `src/imap/mutate.rs` or equivalent module.
- Archive primitive using `UID MOVE` when available.
- Safe fallback plan using `UID COPY`, `UID STORE +\\Deleted`, and
  `UID EXPUNGE` when MOVE is unavailable but UIDPLUS is available.
- Trash primitive.
- Hard-expunge primitive behind an explicit call site.
- Flag primitives for read, unread, starred, and unstarred.
- Remote-first result type describing source folder, destination folder, UID,
  UIDVALIDITY, command path, and local reconciliation action.
- Unit tests for mutation planning and fallback selection.

### Out Of Scope

- CLI command UX.
- SMTP sending.
- Label-specific provider extensions.
- Offline retry queue.

## Stage Graph

1. Mutation model
   - Define mutation capabilities, plans, operations, command paths, and result
     records.

2. Planning
   - Plan MOVE when the server supports MOVE.
   - Plan COPY + delete + UID EXPUNGE only when UIDPLUS makes scoped expunge
     safe.
   - Refuse unsafe copy/delete fallback without UIDPLUS.
   - Plan flag changes as UID STORE operations.

3. Execution
   - Select the source mailbox.
   - Verify UIDVALIDITY before writing.
   - Execute one mutation primitive.
   - Return remote-first reconciliation metadata.

4. Validation and gates
   - Run unit tests for planning and stale UIDVALIDITY handling.
   - Run `cargo fmt --check`, `cargo test`, and clippy.
   - Run live mutation only against a disposable message.

## Checkpoint Target

Against a disposable message, Vivi can mark read/unread and move the message to
Archive or Trash remotely, then report the resulting state clearly.

## Safety Stop

Live validation must not mutate real mail. If no disposable message is available,
the factory run must pause before live execution or get explicit permission to
create a disposable fixture remotely.

## Phase Checkpoint

### Delivered Outputs

- Added `src/imap/mutate.rs`.
- Added mutation capability projection from folder discovery capabilities.
- Added move planning with `UID MOVE` preference.
- Added safe fallback planning using `UID COPY`, `UID STORE +\Deleted`, and
  `UID EXPUNGE` only when UIDPLUS is available.
- Added flag plans for read, unread, starred, and unstarred.
- Added hard-expunge primitive that requires UIDPLUS and remains an explicit
  call site.
- Execution selects the source mailbox, verifies UIDVALIDITY before writing,
  executes one remote primitive, and returns reconciliation metadata.

### Correctness Pass

- Checked stale UIDVALIDITY handling: execution refuses to write if the selected
  mailbox UIDVALIDITY differs from the stored remote reference.
- Checked unsafe fallback handling: no MOVE plus no UIDPLUS returns an error
  rather than broad EXPUNGE.
- Checked flag operations: read/unread and star/unstar map to scoped UID STORE
  operations.
- Checked hard expunge: requires UIDPLUS for scoped UID EXPUNGE.

### Verification Run

- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets -- -D warnings`

### Poker Face

- Self estimate: 82%.
- Evaluator mode: self-contained independent pass.
- Evaluator estimate: 80%.
- Largest missing requirement: the live disposable-message checkpoint has not
  run.
- Verdict: not cleared for full phase completion.

### Gate Result

NEEDS FURTHER REVIEW. Implementation and local validation are complete, but live
mutation validation is intentionally paused until a disposable message fixture is
available or remote fixture creation is explicitly approved.

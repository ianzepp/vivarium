# Storage Phase 04: Mutation And Reconciliation Cutover

## Interpreted Phase Problem

The storage rewrite is still incomplete in one core runtime path: after remote
archive/delete/flag operations succeed, Vivi reconciles local state by moving or
rewriting Maildir files and then patching compatibility catalog fields.

That violates the rewrite directly:

- local mutations are still implemented as filesystem moves and filename
  rewrites
- mutation JSON still exposes `raw_path` and `maildir_subdir`
- reconciliation still derives local identity from filename/path assumptions
- the core message row is not treated as the mutable mailbox-state authority

This is no longer a documentation issue. It is a real runtime mismatch between
the chosen storage model and the write path.

## Normalized Phase Spec

### Goal

Make successful remote mutations reconcile by updating storage-backed message
rows and remote bindings instead of moving or renaming local Maildir files.

### Inputs

- `docs/hash-addressed-storage-rewrite.md`
- `docs/hash-addressed-storage-factory-plan.md`
- completed storage phases 00 through 03
- current mutation command and runner paths

### Expected Outputs

- archive/trash/move reconciliation updates `messages.local_role`
- flag reconciliation updates `messages.read_state` and `messages.starred`
- expunge removes the local message row without pretending blob deletion is the
  same thing
- mutation reconciliation output stops depending on `raw_path` and
  `maildir_subdir`
- tests cover row-state updates instead of Maildir path rewrites

### Out Of Scope

- short-handle CLI lookup
- full README/doc rewrite
- deleting every Maildir helper still used by drafts or outbox
- remote move UID refresh beyond what the current IMAP mutation result exposes

## Repo-Aware Baseline

After Phase 03:

- sync, read, search, thread, and embeddings use blob-backed storage and
  `storage.sqlite`
- deterministic search/thread state is in `storage.sqlite`
- mutation reconciliation still calls `MailStore::move_message`,
  `set_message_flag`, and `remove_message` for ordinary account mail

That makes mutation the last major runtime subsystem still behaving as if
Maildir were authoritative.

## Stage Graph

1. Storage-backed catalog mutation helpers
   - add message-row update helpers for role/read/star state
   - keep or clear remote bindings deliberately based on the mutation outcome

2. Mutation command cutover
   - stop deriving reconciliation from path rewrites
   - emit storage-native reconciliation payloads

3. Verification
   - unit tests for archive/delete/flag reconciliation
   - targeted live dry-run or real mutation verification only if it is safe and
     materially useful

## Checkpoint Target

Successful remote mutations reconcile by editing `storage.sqlite` message state,
not by moving or renaming local Maildir files.

## Gate Plan

- Correctness pass checks that ordinary account mail mutations no longer depend
  on local Maildir files and that expunge removes only the message row, not the
  immutable blob by accident.
- Poker-face should reject the phase if archive/delete/flag still use
  `raw_path`, `maildir_subdir`, or filename-derived local identity on the main
  reconciliation path.

## Completed Checkpoint

Phase 04 landed as a storage-backed mutation reconciliation cutover:

- archive/trash/move reconciliation now updates `messages.local_role` instead
  of moving local Maildir files
- flag reconciliation now updates `messages.read_state` and `messages.starred`
  instead of renaming files for `:2,` flags
- expunge removes the local message row without treating blob deletion as the
  same operation
- mutation reconciliation JSON now reports storage-native state changes instead
  of `raw_path` / `maildir_subdir`
- compatibility catalog rows are cleared during reconciliation so ordinary reads
  fall back to storage-derived state

Validation completed:

- `cargo test --lib mutation_command::tests`
- `cargo test --lib`

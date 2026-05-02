# Write Phase 01: Remote Folder And Capability Discovery

## Interpreted Phase Problem

Before Vivi can issue remote writes, it must know which remote mailboxes exist
and which IMAP write primitives the server advertises. Provider defaults are not
enough: Pharos/Bridge, Gmail, and standard IMAP can expose different folder
names and capabilities.

## Normalized Phase Spec

### Goal

Add a read-only folder and capability discovery surface that resolves the
mailboxes Vivi will use for Archive, Trash, Sent, and Drafts.

### Inputs

- Existing account/provider config.
- Existing IMAP transport/auth.
- IMAP `CAPABILITY` and `LIST` responses.
- Phase 00 remote identity foundation.

### Expected Outputs

- Account config can override inbox, archive, trash, sent, drafts, and optional
  label roots.
- Provider defaults resolve those folders for ProtonMail, Gmail, and standard
  IMAP.
- `vivi folders` inspects remote folders and key IMAP capabilities.
- Capability probe reports UIDPLUS, MOVE, SPECIAL-USE, APPEND, and IDLE.
- Tests cover folder resolution and capability projection.
- Docs record Pharos/Bridge IMAP and SMTP host/port expectations.

### Out Of Scope

- Changing mailbox state.
- Sending mail.
- Gmail label mutation.

## Repo-Aware Baseline

The existing sync path hard-codes provider folder defaults through
`Account::sent_folder` and `Account::all_mail_folder`. The CLI has no read-only
remote folder inspection command. The IMAP transport already provides an
authenticated `Session`, so discovery can share that connection code and avoid
new credentials handling.

## Stage Graph

1. Config and provider resolution
   - Add optional account folder override fields.
   - Add resolved folder helpers for inbox, archive, trash, sent, drafts, and
     label roots.
   - Update sync to use resolved folder helpers.

2. IMAP discovery module
   - Add a read-only discovery module under `src/imap/`.
   - Collect `CAPABILITY`.
   - Collect `LIST "" *`.
   - Project key booleans for UIDPLUS, MOVE, SPECIAL-USE, APPEND, and IDLE.

3. CLI surface
   - Add `vivi folders`.
   - Support text and JSON output.
   - Keep it read-only.

4. Docs and validation
   - Document Pharos/Bridge IMAP and SMTP host/port expectations.
   - Add unit tests for provider defaults and override behavior.
   - Run `cargo fmt --check`, `cargo test`, and clippy.
   - Run live `vivi folders` against `personal-proton`.

## Checkpoint Target

Vivi can list and resolve the remote folders it will use for Archive, Trash,
Sent, and Drafts on the Pharos-backed Proton Bridge account.

## Gate Plan

- Correctness pass checks that discovery is read-only and that config overrides
  do not break existing provider sync defaults.
- Poker-face requires config resolution, CLI discovery, capability projection,
  docs, tests, and live Pharos output.
- Commit only this phase's spec and implementation.

## Open Questions

- None blocking.

## Phase Checkpoint

### Delivered Outputs

- Account config supports optional `inbox_folder`, `archive_folder`,
  `trash_folder`, `sent_folder`, `drafts_folder`, and `label_roots` overrides.
- Provider folder resolution covers ProtonMail, Gmail, and standard IMAP.
- Sync now uses resolved inbox/sent/archive folders instead of hard-coded remote
  names.
- Added read-only IMAP folder discovery via `vivi folders`.
- Added JSON output via `vivi folders --json`.
- Discovery reports resolved folder roles, remote `LIST` folders, and UIDPLUS,
  MOVE, SPECIAL-USE, APPEND, and IDLE capability projections.
- Added `docs/pharos-bridge-email.md` with Pharos/Bridge host, port, folder, and
  discovery expectations.

### Correctness Pass

- Checked that `folders` only calls `CAPABILITY` and `LIST` after
  authentication and does not select, mutate, append, or send.
- Checked that ProtonMail sync still resolves archive to `All Mail`.
- Checked that account override fields are optional, preserving old
  `accounts.toml` compatibility.
- Checked that APPEND is reported available for IMAP4rev1 servers even when not
  advertised as a separate extension atom.

### Verification Run

- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo run -- --help`
- `cargo run -- folders --help`
- `cargo run -- --account personal-proton folders`
- `cargo run -- --account personal-proton folders --json`

Live Pharos/Bridge result for `personal-proton`: resolved folder roles are
`INBOX`, `All Mail`, `Trash`, `Sent`, and `Drafts`; remote `LIST` includes those
folders; capabilities report UIDPLUS=yes, MOVE=yes, SPECIAL-USE=no, APPEND=yes,
and IDLE=yes.

### Poker Face

- Self estimate: 96%.
- Evaluator mode: self-contained independent pass.
- Evaluator estimate: 94%.
- Largest missing or deferred requirement: provider-specific label behavior is
  only listed and documented as future work; label mutation remains Phase 07.
- Verdict: cleared for checkpoint evaluation.

### Gate Result

PASS. Phase 01 is complete enough to commit and proceed to Write Phase 02.

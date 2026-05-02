# Write Phase 00: Remote Identity Foundation

## Interpreted Phase Problem

Vivi can currently preserve raw `.eml` files and build a local JSON catalog, but
the catalog does not remember the remote IMAP object that produced a local
message. Local handles and filenames are not enough for future upstream writes,
because IMAP UIDs are scoped to a mailbox and UIDVALIDITY epoch.

## Normalized Phase Spec

### Goal

Persist enough remote state for a local handle to resolve to the exact remote
IMAP message reference that future write phases can mutate.

### Inputs

- Existing IMAP sync path in `src/imap/sync.rs`
- Existing remote metadata fetch in `src/imap/query.rs`
- Existing JSON catalog in `src/catalog.rs`
- Existing Maildir store and stable handle/fingerprint behavior

### Expected Outputs

- Catalog entries can optionally store remote identity.
- Remote identity contains account, provider, remote mailbox, local folder, UID,
  UIDVALIDITY, RFC Message-ID, size, and content fingerprint.
- Sync captures UIDVALIDITY and remote metadata for synced folders.
- Catalog update attaches remote identity after raw messages are cataloged.
- Library lookup API resolves a local handle to a remote reference.
- Lookup distinguishes missing local handle, missing remote identity, and stale
  UIDVALIDITY.
- Tests cover stale, missing, and duplicate/matching identity behavior.

### Out Of Scope

- Remote IMAP mutation execution.
- SMTP sending.
- Provider labels.
- Offline mutation queue.
- User-facing mutation CLI.

## Repo-Aware Baseline

Vivi is a Rust 2024 CLI/library. The current durable derived state is
`{mail_root}/.vivarium/catalog.json`; it is rebuilt from raw Maildir files by
`catalog::update_maildir` after `imap::sync_messages` downloads messages.

The current IMAP metadata path already fetches UID, size, and RFC Message-ID.
`async-imap` also returns UIDVALIDITY on `SELECT` through the mailbox response.

The safest phase surface is therefore:

- extend `CatalogEntry` with an optional remote identity field using serde
  defaults so older catalogs keep loading
- extend remote metadata records with UIDVALIDITY
- return remote identity candidates from sync
- attach identities to cataloged entries after `update_maildir`
- expose lookup/status helpers from `Catalog`

## Stage Graph

1. Catalog schema extension
   - Add serializable remote identity structures.
   - Preserve loading of older catalog files.
   - Add lookup/status helpers.

2. IMAP metadata capture
   - Capture UIDVALIDITY during folder selection.
   - Carry UIDVALIDITY through `RemoteMessage`.
   - Return per-message remote identity candidates from sync.

3. Catalog reconciliation
   - Attach candidates to matching catalog entries after raw cataloging.
   - Prefer RFC Message-ID matches inside account/folder.
   - Fall back to local filename UID/size matches.
   - Keep raw bytes and content fingerprint as the source of local proof.

4. Validation and gates
   - Add unit tests for old catalog compatibility, identity lookup, stale
     UIDVALIDITY, missing identity, and sync metadata propagation.
   - Run `cargo fmt`.
   - Run `cargo test`.
   - Run a read-only live sync smoke check if account config is available.

## Checkpoint Target

Given a local message handle, Vivi can produce a remote reference containing
account, mailbox, UID, and UIDVALIDITY, or a clear missing/stale-reference
status.

## Gate Plan

- Correctness pass checks catalog migration safety, identity matching ambiguity,
  and whether UIDVALIDITY absence is represented without corrupting the catalog.
- Poker-face requires the saved catalog schema, sync capture, lookup API, and
  tests to be present.
- Commit only this phase's spec and implementation.

## Open Questions

- None blocking.

## Phase Checkpoint

### Delivered Outputs

- `CatalogEntry` now has optional `remote` identity with serde defaults for old
  catalog compatibility.
- Remote identity stores account, provider, remote mailbox, local folder, UID,
  UIDVALIDITY, RFC Message-ID, remote size, and local content fingerprint.
- IMAP metadata capture carries UIDVALIDITY from `SELECT` into per-message
  remote identity candidates.
- Sync reconciles remote identity candidates into the catalog after raw Maildir
  cataloging.
- `Catalog::remote_reference` and `Catalog::remote_reference_status` resolve a
  handle to a remote reference or distinguish missing handle, missing identity,
  and stale UIDVALIDITY.
- Tests cover old catalog compatibility, identity matching, filename fallback,
  stale UIDVALIDITY, missing identity, missing UIDVALIDITY, ambiguity, and sync
  candidate propagation.

### Correctness Pass

- Checked catalog migration safety: missing `remote` fields load as `None`.
- Checked identity ambiguity: duplicate RFC Message-ID matches do not attach a
  remote identity silently.
- Checked UIDVALIDITY absence: candidates without UIDVALIDITY are counted and
  skipped instead of writing unsafe remote references.
- Checked raw-source invariant: content fingerprint is copied from the local raw
  catalog entry; raw bytes remain unchanged.

### Verification Run

- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo run -- --version`
- `cargo run -- --help`
- `cargo run -- --account personal-proton sync --limit 1 --since 7d`

Live sync result: one bounded read-only sync through the Pharos-backed
`personal-proton` account completed with `new=1`, `cataloged=1`, `extracted=1`,
and remote identity reconciliation `matched=211`, `missing_uidvalidity=0`,
`missing_local=0`, `ambiguous=0`.

### Poker Face

- Self estimate: 95%.
- Evaluator mode: self-contained independent pass.
- Evaluator estimate: 93%.
- Largest missing or deferred requirement: no user-facing command exposes the
  remote reference yet; the phase requested a lookup API, and the library API is
  present.
- Verdict: cleared for checkpoint evaluation.

### Gate Result

PASS. Phase 00 is complete enough to commit and proceed to Write Phase 01.

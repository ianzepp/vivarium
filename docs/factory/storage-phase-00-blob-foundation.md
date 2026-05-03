# Storage Phase 00: Blob Store And Source-Of-Truth Foundation

## Interpreted Phase Problem

`docs/hash-addressed-storage-rewrite.md` fixes the target architecture, but the
live repo still assumes Maildir almost everywhere. Sync writes Maildir files,
catalog rows store `raw_path`, and read/search surfaces reopen messages by path.

That makes a one-shot cutover too large for this first phase. The right
checkpoint here is to land the new blob store and `storage.sqlite` foundation
as the intended primary engine, then use the next phase to flip sync and
catalog directly onto it.

## Normalized Phase Spec

### Goal

Introduce the new durable storage engine and make it capable of owning sync
ingest directly.

### Inputs

- `docs/hash-addressed-storage-rewrite.md`
- `src/sync.rs`
- `src/catalog.rs`
- `src/store.rs`
- current IMAP sync and remote identity foundation

### Expected Outputs

- new `src/storage.rs` module with:
  - `Storage::open`
  - blob-path derivation from full content hash
  - `storage.sqlite` schema initialization
  - ingest/import APIs for catalog-backed messages
- source-of-truth tables for blobs, messages, remote bindings, and parsed
  metadata
- message ingestion that writes one blob per unique content hash and distinct
  message rows for distinct mailbox occurrences
- focused tests for blob dedupe, metadata parsing, fallback message-id
  generation, and remote binding persistence

### Out Of Scope

- deleting or rewriting `catalog.sqlite`
- deleting or rewriting `index.sqlite`
- read/list/search/thread cutover
- mutation cutover
- reset CLI or migration UX

## Repo-Aware Baseline

The repo is still path-coupled, but the user has now explicitly said a clean
break is preferable and redownload is acceptable. That removes the need for a
catalog-to-storage bridge.

This phase therefore stays foundation-only:

- land the new storage engine
- keep it isolated and testable
- prepare direct sync cutover as the next bounded phase

## Stage Graph

1. Durable artifact and schema
   - add the factory plan and phase delivery spec
   - add the new storage module and schema initializer

2. Blob and message ingestion
   - hash raw bytes
   - write immutable blob files once
   - persist parsed metadata and mutable message rows
   - attach remote bindings when ingest is given remote context

3. Validation and gates
   - add focused storage tests
   - run `cargo fmt`
   - run targeted and full Rust tests

## Checkpoint Target

Given raw bytes plus remote context, the storage engine produces:

- a blob at `blobs/<ab>/<cd>/<content_id>.eml`
- a row in `.vivarium/storage.sqlite` `messages`
- parsed metadata in `message_metadata`
- a `remote_bindings` row when remote identity is available

## Gate Plan

- Correctness pass checks duplicate-content handling, blob immutability, and
  message-id stability for remote-bound messages.
- Poker-face requires the new storage schema, ingest path, and tests to exist
  together.
- Commit only the phase docs and phase-scoped implementation.

## Open Questions

- None blocking for this phase.

## Phase Checkpoint

### Delivered Outputs

- Added `src/storage.rs` with `Storage::open`, `Storage::ingest_message`, and
  a catalog-entry import helper for bounded tests and bootstrap tooling.
- Added `.vivarium/storage.sqlite` schema creation for `blobs`, `messages`,
  `remote_bindings`, and `message_metadata`.
- Added blob sharding at `blobs/<ab>/<cd>/<content_id>.eml`.
- Added deterministic `message_id` generation for remote-bound and fallback
  local-only rows.
- Added focused tests for blob dedupe, metadata persistence, fallback
  identifiers, and the direct ingest API.
- Added the repo-local factory plan for the storage rewrite.

### Correctness Pass

- Checked blob immutability behavior: repeated ingest reuses the same blob path
  and does not duplicate blob files.
- Checked duplicate-content behavior: distinct remote-bound occurrences create
  distinct `messages` rows while reusing the same blob.
- Checked foundation shape: the primary ingest API now accepts raw bytes plus
  direct message context rather than depending on catalog rows.
- Checked clean-break alignment: removed the temporary sync-time adapter before
  checkpointing the phase.

### Verification Run

- `cargo fmt`
- `cargo test storage:: --lib`
- `cargo test --lib`

### Poker Face

- Self estimate: 91%.
- Largest deferred requirement: sync, catalog, and read-path cutover have not
  started yet; this phase only establishes the new primary storage engine.
- Verdict: clear to commit as a bounded foundation phase.

### Gate Result

PASS. Phase 00 is complete enough to commit and proceed to Storage Phase 01.

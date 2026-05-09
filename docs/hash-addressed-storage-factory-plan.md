# Vivi Factory Plan: Hash-Addressed Storage Rewrite

## Factory Intake

### Phase Set Source

`docs/hash-addressed-storage-rewrite.md` defines the target architecture:
replace Maildir as Vivi's source of truth with hash-addressed raw blobs plus one
primary `storage.sqlite` database.

The repo is still deeply path-coupled today. Sync writes Maildir files, the
catalog persists `raw_path`, the deterministic index rebuilds from catalog rows
and filesystem reads, and retrieval/mutation commands reopen messages by path.
That means the rewrite needs an explicit phase set rather than one heroic
cutover commit.

### Target Repo

The local Vivarium checkout.

### Delivery Spec Directory

Write one delivery spec per phase under:

`docs/factory/`

Recommended names:

- `docs/factory/storage-phase-00-blob-foundation.md`
- `docs/factory/storage-phase-01-sync-and-catalog-cutover.md`
- `docs/factory/storage-phase-02-read-search-thread-cutover.md`
- `docs/factory/storage-phase-03-storage-db-consolidation.md`
- `docs/factory/storage-phase-04-mutation-and-reconciliation-cutover.md`
- `docs/factory/storage-phase-05-handle-reset-and-docs.md`

### Checkpoint Policy

Each phase must end with:

- a saved phase delivery spec
- phase-scoped implementation only
- correctness notes against the selected phase invariants
- validation commands run, or skipped checks documented
- a poker-face completion estimate
- a local commit

### Commit Policy

Commit after every completed phase. Keep commits phase-scoped. Favor a clean
break. Since full redownload is acceptable, do not preserve Maildir/catalog
compatibility longer than a bounded foundation phase absolutely requires.

### Agent Policy

Use explorer agents for bounded read-only mapping and one bounded worker only
when the write surface is clearly isolated. Factory remains responsible for
integration, verification, and commits.

### Correctness Policy

Preserve these invariants throughout:

- raw RFC 5322 bytes remain the only content source of truth
- immutable content identity is the full SHA-256 of raw bytes
- mutable mailbox state is keyed separately from blob identity
- duplicate exact-byte content can exist as distinct message rows
- blob paths never encode role, flags, or remote state
- read/search/thread/embedding rebuild remains possible from durable storage
- no silent compatibility bridge should survive past the cutover phases

## Phase Set

### Storage Phase 00: Blob Store And Source-Of-Truth Foundation

#### Goal

Introduce the new storage engine and durable schema without pretending the rest
of the repo has already been cut over.

#### Expected Outputs

- new `src/storage.rs` module
- `.vivarium/storage.sqlite` schema for blobs, messages, remote bindings, and
  parsed metadata
- blob sharding under `blobs/ab/cd/<content_id>.eml`
- deterministic synced-message `message_id` generation
- ingest APIs ready for direct sync cutover
- tests covering blob dedupe, metadata persistence, and remote-binding writes

#### Out Of Scope

- removing `catalog.sqlite`
- removing `index.sqlite`
- list/search/thread/retrieve cutover
- mutation cutover

#### Checkpoint Target

The new storage engine exists as a coherent primary target with schema, blob
writing, metadata parsing, and message-row persistence ready for direct sync
cutover in the next phase.

### Storage Phase 01: Sync And Catalog Cutover

#### Goal

Make sync and catalog source their truth from the new storage engine rather than
Maildir scans and path-based catalog rows, including the minimum read surfaces
required to keep Vivi coherent after sync stops writing Maildir.

### Storage Phase 02: Read, Search, And Thread Cutover

#### Goal

Move the remaining indexed and search-driven surfaces fully off compatibility
views and onto native `storage.sqlite` + blob reads, including handle-model
cleanup.

### Storage Phase 03: Storage DB Consolidation

#### Goal

Fold deterministic thread/search metadata into `storage.sqlite` and eliminate
the separate `index.sqlite` core DB.

### Storage Phase 04: Mutation And Reconciliation Cutover

#### Goal

Replace local filesystem moves and filename flag rewrites with message-row and
remote-binding updates.

### Storage Phase 05: Handle, Reset, And Docs Cleanup

#### Goal

Finish the user-facing clean break: opaque `message_id` rows, short-hash
handles, reset/bootstrap semantics, and docs that stop describing Maildir as
the source of truth.

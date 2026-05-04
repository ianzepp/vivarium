# Storage Phase 01: Sync, Catalog, And Basic Read Cutover

## Interpreted Phase Problem

Once sync stops writing Maildir, Vivi cannot keep `list`, `show`, and export on
directory scans without becoming incoherent. The original phase split was too
optimistic about how independent sync and read surfaces were.

This phase therefore has to do three things together:

- make sync write directly to hash-addressed storage
- make `Catalog` read from `storage.sqlite` instead of `catalog.sqlite`
- move the basic `MailStore` read/list helpers onto storage so the CLI still
  works after a fresh re-download

## Normalized Phase Spec

### Goal

Cut sync and catalog over to `storage.sqlite` as the only source of truth, and
move the minimum direct read surfaces needed for a coherent clean break.

### Inputs

- `docs/hash-addressed-storage-rewrite.md`
- `docs/hash-addressed-storage-factory-plan.md`
- Phase 00 storage foundation in `src/storage.rs`
- current sync path in `src/imap/sync.rs` and `src/sync.rs`
- current catalog and store abstractions

### Expected Outputs

- sync downloads raw bytes straight into the blob store and `storage.sqlite`
- remote bindings are written at ingest time instead of attached later through
  catalog reconciliation
- `Catalog` becomes a storage-backed compatibility view rather than a separate
  SQLite database
- `MailStore` list/read/locate and sync dedupe helpers read from storage instead
  of Maildir scans
- `catalog.sqlite` and Maildir scans are no longer on the normal sync/read path
- tests cover sync dedupe against storage, catalog listing from storage, and
  basic read/list behavior without Maildir

### Out Of Scope

- mutation move/flag/delete cutover
- final handle-model switch to short unique prefixes
- search/index schema cleanup
- embedding schema cleanup

## Repo-Aware Baseline

The critical discovery after Phase 00 is that sync and basic read surfaces are
not separable in a clean-break rollout. `MailStore` is the read path for `list`,
`show`, and export, and sync currently relies on `MailStore`-scanned RFC and
size indexes for dedupe.

The smallest coherent cutover is therefore:

1. give `Storage` enough query APIs to back list/read/dedupe
2. move `MailStore` read-only helpers onto those APIs
3. replace sync writes with direct storage ingest
4. replace `Catalog` with a storage-backed compatibility view

Compatibility note for this phase:

- compatibility views are allowed in Rust types and helper methods
- compatibility persistence in `catalog.sqlite` is not allowed

## Stage Graph

1. Storage query surface
   - add lookup APIs for content blobs, message metadata, remote bindings, and
     dedupe queries

2. MailStore read-path cutover
   - move list/read/locate/local-size/RFC-index helpers onto storage

3. Sync cutover
   - ingest remote messages directly into storage
   - remove catalog reconciliation from the sync path

4. Catalog cutover
   - source catalog rows from storage-derived data
   - stop opening or writing `catalog.sqlite` on the normal path

5. Validation and gates
   - focused tests for storage-backed sync/read behavior
   - `cargo fmt`
   - targeted tests
   - full library tests

## Checkpoint Target

After `vivi sync --reset`, Vivi can redownload mail into blobs plus
`storage.sqlite`, and `list`/`show`/export still work without Maildir storage.

## Gate Plan

- Correctness pass checks duplicate handling, sync dedupe semantics, remote
  binding integrity, and whether the normal path still depends on Maildir scans
  or `catalog.sqlite`.
- Poker-face requires direct sync ingest, storage-backed catalog reads, and
  basic read/list continuity together.
- Commit only when the new path is coherent after a fresh reset.

## Open Questions

- Whether `EmailIndex` rebuild needs a small compatibility patch in this phase
  or can stay entirely for Phase 02 depends on how much of it still assumes
  filename-derived handles after catalog cutover.

## Phase Checkpoint

### Delivered Outputs

- Sync now ingests remote messages directly into the blob store plus
  `.vivarium/storage.sqlite` instead of writing Maildir files first.
- Remote bindings are written during ingest, and sync extraction now runs from
  storage-derived catalog entries instead of post-sync Maildir catalog scans.
- `Catalog` is now a storage-backed facade over `storage.sqlite`, with a small
  `catalog_compat` table inside the same DB to preserve transitional
  compatibility fields without using `catalog.sqlite` as the source of truth.
- `MailStore` list/read/locate plus sync dedupe helpers now read from storage
  for synced roles while preserving local file-backed behavior where still
  needed for older tests and outbox-like flows.
- `EmailIndex` rebuild and mutation prep no longer require Maildir-shaped paths
  for blob-backed rows; they fall back to the catalog handle when the path is a
  content-addressed blob path.
- The Phase 01 factory artifact and revised phase boundary are saved in
  `docs/hash-addressed-storage-factory-plan.md` and this file.

### Correctness Pass

- Checked direct-ingest duplicate behavior: sync dedupe still works from storage
  metadata and RFC Message-ID lookups.
- Checked remote-binding integrity: live reset sync created `2454` message rows
  and `2454` remote bindings in `storage.sqlite`.
- Checked clean-break behavior: after `sync --reset`, Vivi rebuilt the local
  cache from blobs plus `storage.sqlite` and did not rely on Maildir files for
  `list`, `show`, or index rebuild.
- Checked index compatibility: a handle-selection bug that treated blob
  filenames as user-facing handles was fixed, and `index status` now reports
  `pending=0` after rebuild.

### Verification Run

- `cargo fmt`
- `cargo test sync:: --lib`
- `cargo test email_index::tests:: --lib`
- `cargo test --lib`
- `cargo run -- --account personal-proton sync --limit 1 --since 7d`
- `cargo run -- --account personal-proton sync --reset`
- `cargo run -- --account personal-proton list inbox --limit 5`
- `cargo run -- --account personal-proton show inbox-2058 --json`
- `cargo run -- --account personal-proton index rebuild`
- `cargo run -- --account personal-proton index status`
- `cargo run -- --account personal-proton search Proton --limit 3`

Live reset evidence after `sync --reset`:

- `blobs/` file count: `2452` during sync, then full sync completion at `2454`
  new messages
- `storage.sqlite` message rows: `2454`
- `storage.sqlite` remote bindings: `2454`
- `vivi list inbox --limit 5` returned real inbox rows from the rebuilt cache
- `vivi show inbox-2058 --json` returned a blob-backed citation under
  `blobs/e5/1c/...`
- `vivi index status` reported `catalog=2454 indexed=2454 pending=0` after the
  rebuild

### Poker Face

- Self estimate: 90%.
- Largest deferred work: search/thread/index still carry compatibility fields
  like `raw_path` and `maildir_subdir`, and the handle model is still
  transitional.
- Verdict: Phase 01 is complete enough to commit; remaining work belongs to the
  next cleanup/cutover phases rather than this foundation slice.

### Gate Result

PASS. Phase 01 is complete enough to commit and proceed to the remaining
search/thread/mutation/embedding cleanup phases.

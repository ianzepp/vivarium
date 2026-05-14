# Storage Phase 03: Fold Deterministic Index State Into `storage.sqlite`

## Interpreted Phase Problem

The hash-addressed rewrite still violates one of its core storage decisions:
Vivi keeps a separate `.vivarium/index.sqlite` for deterministic thread/search
metadata even though `docs/hash-addressed-storage-rewrite.md` says
`storage.sqlite` should be the only core metadata database.

Today that leak shows up in several ways:

- `src/email_index.rs` opens and manages `index.sqlite`
- `vivi index rebuild` repopulates a second DB instead of derived tables inside
  `storage.sqlite`
- search, thread, and embeddings depend on the separate DB surface even though
  their underlying message identity now lives in storage-native rows
- the completion audit for the rewrite fails on the explicit requirement to
  fold deterministic index concerns into `storage.sqlite`

This phase should remove the separate core DB while preserving the already
working search/thread/index/embedding behavior.

## Normalized Phase Spec

### Goal

Move deterministic link/index state into `storage.sqlite` and stop creating or
depending on `.vivarium/index.sqlite`.

### Inputs

- `docs/hash-addressed-storage-rewrite.md`
- `docs/hash-addressed-storage-factory-plan.md`
- completed storage phases 00, 01, and 02
- current `email_index`, `search`, `thread`, `index_runner`, and `embeddings`
  code

### Expected Outputs

- `message_links` and related deterministic metadata live in
  `.vivarium/storage.sqlite`
- `vivi index rebuild` rebuilds deterministic state in `storage.sqlite`
- thread/search/embedding reads no longer require a separate `index.sqlite`
  file
- tests updated to validate the storage-backed deterministic state instead of a
  second DB file

### Out Of Scope

- mutation reconciliation semantics
- short-handle CLI resolution and prefix lookup
- deleting every compatibility helper around `CatalogEntry`
- README/doc cleanup beyond any small wording needed for this phase

## Repo-Aware Baseline

The live repo after Phase 02 has:

- `storage.sqlite` as the source of truth for blobs, message rows, and parsed
  metadata
- `index.sqlite` still present for deterministic thread/search metadata
- real `vivi index rebuild` and `vivi search --json` working against the
  separate index DB
- mutation and README surfaces still using Maildir-era assumptions

That means the next clean checkpoint is DB consolidation, not user-facing docs
or mutation semantics.

## Stage Graph

1. Storage schema extension
   - add deterministic link tables to `storage.sqlite`
   - add storage helpers for rebuild/query paths

2. Email-index facade rewrite
   - preserve `email_index` call sites if useful
   - remove filesystem/index.sqlite lifecycle management

3. Surface cutover
   - thread, search, embeddings, and index commands read through the
     storage-backed deterministic state

4. Verification
   - unit tests for rebuild/search/thread/embeddings
   - live `index rebuild` and search validation on the real mailbox

## Checkpoint Target

The repo no longer creates or depends on `.vivarium/index.sqlite`; deterministic
thread/search metadata is rebuilt and queried from `storage.sqlite` instead.

## Gate Plan

- Correctness pass checks that deterministic links remain rebuildable from blob
  content and that thread/search behavior still works on real data.
- Poker-face should reject the phase if `index.sqlite` still appears as a core
  runtime dependency or if search/thread quietly fall back to stale state.

## Completed Checkpoint

Phase 03 landed as a storage-backed deterministic-state consolidation:

- `email_index` now uses `.vivarium/storage.sqlite` instead of a separate
  `.vivarium/index.sqlite`
- deterministic rows now live in storage-backed `indexed_messages` and
  `message_links` tables
- thread, search, and embeddings still read through the `email_index` facade,
  but that facade is now a projection over `storage.sqlite`
- stale legacy `.vivarium/index.sqlite` files are removed on open so the clean
  break is visible on disk

Validation completed:

- `cargo fmt`
- `cargo test --lib`
- `cargo run -- --account personal-proton index rebuild`
- `cargo run -- --account personal-proton search Proton --limit 3 --json`
- verified that `~/.vivarium/personal-proton/.vivarium/index.sqlite`
  no longer exists after rebuild

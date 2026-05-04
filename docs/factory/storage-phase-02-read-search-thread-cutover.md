# Storage Phase 02: Read, Search, Thread, And Index Shape Cleanup

## Interpreted Phase Problem

Phase 01 made `storage.sqlite` and blobs the live source of truth for sync,
catalog, and basic reads, but several public and derived surfaces still carry
Maildir-era fields:

- `raw_path`
- `maildir_subdir`
- `catalog_handle`
- path-based citation shapes
- embedding chunk keys tied to handle/fingerprint rather than
  `message_id`/`content_id`

The system works, but it is still exposing compatibility data instead of the
native storage model described in `docs/hash-addressed-storage-rewrite.md`.

## Normalized Phase Spec

### Goal

Move the remaining indexed, retrieval, search, thread, and embedding-facing
surfaces onto native `message_id` plus `content_id` semantics and stop treating
blob paths as ordinary user-facing data.

### Inputs

- `docs/hash-addressed-storage-rewrite.md`
- `docs/hash-addressed-storage-factory-plan.md`
- `docs/factory/storage-phase-01-sync-and-catalog-cutover.md`
- current index, search, retrieve, thread, and embeddings modules

### Expected Outputs

- `retrieve` and `thread` citations prefer handle/account/role plus optional
  content prefix instead of Maildir-shaped fields
- `search` result rows stop exposing `maildir_subdir` and stop treating
  `raw_path` as the primary citation field
- `email_index` row shape stops carrying `catalog_handle` and Maildir-specific
  columns as required fields
- embedding chunk identity and storage shift from handle/fingerprint to
  `message_id`/`content_id`
- tests cover blob-backed citations, indexed handle behavior, and embedding
  rebuild semantics after the schema change

### Out Of Scope

- mutation move/flag/delete model cleanup
- final local-role mutation semantics
- deleting all compatibility helper code in one pass if a smaller cleanup step
  keeps the phase coherent

## Repo-Aware Baseline

Live verification after Phase 01 showed:

- `sync --reset` repopulates blobs plus `storage.sqlite`
- `list`, `show`, `index rebuild`, and `search` work on the real mailbox
- compatibility fields still leak through result shapes and DB schemas

That means the next phase is no longer about foundational correctness. It is a
shape cleanup and native-identity cleanup phase.

## Stage Graph

1. Retrieval and citation cleanup
   - revise `retrieve.rs` and `thread.rs` citation JSON
   - keep blob paths internal or explicitly debug-only

2. Search and index row cleanup
   - revise `SearchResult` and index row shapes away from Maildir fields
   - keep enough compatibility only where existing commands still need it

3. Embedding identity cleanup
   - move chunk identity/storage from handle/fingerprint to
     `message_id`/`content_id`
   - keep provider/model-scoped DB split

4. Validation and gates
   - focused tests for search/thread/retrieve/embedding output shapes
   - `cargo fmt`
   - full library tests
   - bounded live checks against the real mailbox as needed

## Checkpoint Target

The public read/search/thread surfaces no longer describe Maildir internals, and
embeddings rebuild from `message_id` plus `content_id` without path dependence.

## Gate Plan

- Correctness pass checks that path-shaped citations are gone from ordinary
  reads, index rows still resolve to the right blobs, and embeddings survive
  role changes without content changes.
- Poker-face requires both API-shape cleanup and real rebuild behavior, not just
  schema edits.
- Commit only when the remaining compatibility fields are intentionally bounded
  and documented.

## Open Questions

- Whether the final short-handle presentation belongs inside this phase or in a
  follow-on handle-model phase depends on how invasive the CLI-output changes
  look once the storage-native result shapes are in place.

## Completed Checkpoint

Phase 02 landed as a storage-native read/search/thread/index cleanup:

- `email_index` rows now key on `message_id` and carry `content_id`,
  `blob_path`, and `local_role`
- index rebuild now reads through storage-backed catalog views so blob paths
  point at immutable stored blobs instead of Maildir compatibility paths
- retrieve, thread, and search citations now expose `handle`, `account`,
  `local_role`, and optional `content_id` instead of Maildir internals
- embedding chunk identity and embedding storage moved from handle/fingerprint
  to `message_id`/`content_id`
- embedding DB schema is versioned and resets cleanly across the storage-native
  shape change

Validation completed:

- `cargo fmt`
- `cargo test --lib`
- `cargo run -- --account personal-proton index rebuild`
- `cargo run -- --account personal-proton search Proton --limit 3 --json`

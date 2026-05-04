# Storage Phase 06: Catalog Compatibility Removal

## Interpreted Phase Problem

The storage rewrite had moved runtime data into `storage.sqlite`, but the old
catalog compatibility table still existed as a bridge for path-shaped fields.
That left a stale `catalog_compat` table in existing caches and kept remote
identity attachment able to fall back to `inbox-UID` filename assumptions.

## Normalized Phase Spec

### Goal

Remove the persisted catalog compatibility layer and make catalog-facing reads a
synthesized view over storage-backed message, blob, metadata, and remote binding
tables.

### Inputs

- `docs/hash-addressed-storage-rewrite.md`
- completed storage phases 00 through 05
- `src/catalog.rs`
- `src/catalog/local.rs`
- `src/catalog/remote.rs`

### Expected Outputs

- `catalog_compat` is no longer created, queried, updated, or deleted
- existing `catalog_compat` tables are dropped during storage schema setup
- catalog entries synthesize blob paths and role-shaped compatibility fields
  from storage rows only
- remote identity attachment no longer resolves by legacy `folder-UID`
  filenames
- tests assert storage handles instead of old Maildir IDs

### Out Of Scope

- Removing the `CatalogEntry` struct entirely
- Reworking local draft/outbox file handling
- Removing Maildir fallback methods used only by legacy tests or local draft
  workflows

## Repo-Aware Baseline

Before this phase, ordinary sync, list, read, thread, search, mutation, and
embedding paths were storage-backed, but catalog compatibility metadata could
still linger inside `storage.sqlite`. The live `personal-proton` cache still had
a `catalog_compat` table even though newer code no longer relied on
`catalog.sqlite`.

## Checkpoint Target

Opening the storage-backed runtime should leave no `catalog_compat` table in
the live account database, and the full library test suite should remain green.

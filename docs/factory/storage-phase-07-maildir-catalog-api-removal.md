# Storage Phase 07: Maildir Catalog API Removal

## Interpreted Phase Problem

After removing the persisted catalog compatibility table, the catalog module
still exposed dead APIs for scanning Maildir folders and updating catalog rows
from filesystem paths. Those functions were no longer used by runtime code, but
their tests kept the old path-oriented catalog model alive.

## Normalized Phase Spec

### Goal

Remove unused Maildir catalog scan/update helpers so catalog behavior is limited
to storage-backed rows and legacy JSON import.

### Inputs

- `docs/hash-addressed-storage-rewrite.md`
- `src/catalog.rs`
- `src/catalog/local.rs`
- catalog and mutation tests

### Expected Outputs

- remove `CatalogUpdateResult`
- remove `update_maildir`
- remove `scan_maildir` and private Maildir scan helpers
- remove unused `handle_for_path` and `update_local_location`
- keep storage-backed catalog tests green

### Out Of Scope

- Removing the `CatalogEntry` compatibility struct
- Rewriting draft/outbox file handling
- Removing low-level `MailStore` helpers still used by draft and test code

## Checkpoint Target

No runtime or test code should call the old Maildir catalog scan/update APIs,
and `cargo test --lib` should pass.

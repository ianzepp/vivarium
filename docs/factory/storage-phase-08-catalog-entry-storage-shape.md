# Storage Phase 08: Catalog Entry Storage Shape

## Interpreted Phase Problem

`CatalogEntry` still carried Maildir-oriented fields even after the runtime
catalog moved to `storage.sqlite`. That kept path-shaped compatibility concepts
in index, extraction, mutation, and test fixtures, and it left a legacy JSON
import path wired into catalog open.

## Normalized Phase Spec

### Goal

Make `CatalogEntry` describe the storage-backed message view directly instead
of preserving the old Maildir catalog JSON shape.

### Inputs

- `docs/hash-addressed-storage-rewrite.md`
- `src/catalog.rs`
- storage, indexing, extraction, search, threading, and mutation consumers
- catalog and storage tests

### Expected Outputs

- remove `raw_path`, `fingerprint`, `folder`, `maildir_subdir`, and
  `is_duplicate` from `CatalogEntry`
- expose `content_id`, `blob_path`, `local_role`, `read_state`, and `starred`
  on catalog entries
- stop importing `.vivarium/catalog.json` during catalog open
- remove the unused legacy `src/catalog/sqlite.rs` module
- update consumers to read storage fields directly

### Out Of Scope

- Removing `CatalogEntry` itself as a transitional API between modules
- Rewriting draft/outbox Maildir file handling
- Removing low-level `MailStore` helpers still used by non-catalog code

## Checkpoint Target

No compiled `CatalogEntry` consumer should depend on path-shaped Maildir fields,
and `cargo test --lib` should pass.

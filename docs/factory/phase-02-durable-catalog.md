# Phase 02: Durable Catalog

## Interpreted Problem

Phase 01 established Proton Bridge as the first-class read-only ingest path.
Phase 02 adds a local catalog that gives every message a stable handle and
stores rebuildable metadata without replacing raw `.eml` files as the source
of truth.

## Phase Spec

### Goal

Add a local catalog that gives every message a stable handle and stores
rebuildable metadata without replacing raw `.eml` files as the source of truth.

### Expected Outputs

1. Local SQLite catalog (`{mail_root}/.vivarium/catalog.db`)
2. Stable message handles (content-fingerprint-based)
3. Raw file path and content fingerprint per message
4. Metadata: account, folder, Maildir subdir, date, from/to/cc/bcc, subject, RFC Message-ID
5. Duplicate tracking across folders
6. Catalog rebuild command (`vivarium catalog rebuild`)
7. Tests for handle stability and rebuild behavior

### Out Of Scope

- Embeddings
- Summaries
- Attachment OCR
- Cross-device sync

### Checkpoint Target

Deleting the catalog and rebuilding it from raw mail produces the same stable
handles for unchanged messages.

## Workstreams

### WS-02-A: SQLite Schema and Library

- Add `sqlite3` dependency to Cargo.toml
- Create `src/catalog.rs` with:
  - `Catalog` struct wrapping a `rusqlite::Connection`
  - Schema: messages table with handle, raw_path, fingerprint, account, folder,
    subdir, date, from, to, cc, bcc, subject, rfc_message_id, is_duplicate
  - `open()` / `create()` for catalog initialization
  - `insert()` / `upsert()` for adding messages
  - `list_messages()` for querying
  - `rebuild()` for full rebuild from raw files
  - `handle_for_path()` for handle lookup
- Keep catalog file at `{mail_root}/.vivarium/catalog.db`

### WS-02-B: Stable Handles

- Handle = first 16 chars of SHA-256(content of raw .eml file)
- Content fingerprint = SHA-256 of raw bytes
- Handle is stable across syncs for unchanged messages
- On ingest, check catalog: if fingerprint matches existing handle, link it
- If handle already exists but points to a different path, mark as duplicate

### WS-02-C: Metadata Extraction

- Extract from/to/cc/bcc/subject/date/rfc_message_id from each .eml during catalog scan
- Use existing `mail_parser` for parsing
- Store parsed values in catalog columns
- Invalid parses recorded but don't block catalog

### WS-02-D: Catalog Rebuild Command

- `vivarium catalog rebuild` scans all folders for the account
- Rebuilds the catalog from raw files
- Updates handles, fingerprints, metadata
- Prints summary: N scanned, N new, N updated, N duplicates

### WS-02-E: Tests

- Handle stability: ingest → rebuild → same handles
- Rebuild from empty catalog → same metadata as original scan
- Duplicate detection across folders

## Dependencies Added

- `sqlite3` (build system) via `pkg-config` for `rusqlite`
- `rusqlite` crate (version 7 or compatible)

## Verification Commands

```
cargo check
cargo test
```

## Gate Plan

| Gate | Trigger | Pass Criteria | Fail Action |
|------|---------|--------------|-------------|
| Build + Tests | After WS-02-A through WS-02-E | `cargo check` clean, `cargo test` all green | Fix compilation or test failures |
| Checkpoint | After all workstreams | Rebuild from empty catalog produces stable handles | Revise handle generation |

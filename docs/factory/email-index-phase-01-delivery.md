# Email Index Phase 01 Delivery

## Phase Name

Deterministic index and indexed thread lookup.

## Input

- `docs/email-embeddings-plan.md`
- Current slow `vivi thread <handle> --json` behavior caused by broad Maildir
  scans in `src/thread.rs`

## Problem

Thread display currently parses every message in `INBOX`, `Archive`, `Sent`,
and `Drafts` before returning output. This is unusable on the real mailbox and
also leaves no deterministic metadata index for future semantic search.

## Scope

Implement the first deterministic index slice:

- account-local `.vivarium/index.sqlite`
- `messages` table with catalog-derived metadata
- `message_links` table with normalized `Message-ID`, `In-Reply-To`, and
  `References`
- idempotent rebuild/upsert from existing catalog entries
- explicit `vivi index rebuild`, `vivi index status`, and `vivi index pending`
  commands
- indexed `vivi thread <handle> --json`

## Out Of Scope

- embedding DBs
- chunking
- semantic search
- hybrid search
- `sync --index`
- FTS5 lexical index
- storing message body text, extracted text, chunk text, or snippets in SQLite

## Acceptance Criteria

- `index.sqlite` is created under the selected account mail root.
- Index rebuild is derived from `.vivarium/catalog.json` and raw `.eml` files.
- `message_links` records `message_id`, `in_reply_to`, and `reference` values.
- `vivi index rebuild` emits progress for large mailboxes.
- `thread` can find a sent reply by RFC `References` without broad Maildir
  folder scans.
- `thread` does not hide a large first index build when the index is empty.
- Existing thread JSON shape remains usable.
- No message body text or chunk text is written to SQLite.

## Checkpoint

Run:

```sh
cargo test
cargo build
target/debug/vivi index rebuild --account personal-proton
target/debug/vivi thread inbox-2048 --json
```

The live `thread` command should return promptly after the account index has
been built explicitly.

## Gate

PASS if tests and build succeed and `thread` is backed by indexed metadata
rather than folder-wide parsing. NEEDS REVIEW if `thread` still depends on
broad folder scans or index creation requires persisting message text.

# Hash-Addressed Storage Rewrite

## Problem

Vivi currently mixes two different identity models:

- content identity through the catalog `handle` and `fingerprint`
- storage identity through Maildir paths, folder names, `new`/`cur`, and
  filename flags

That split leaks through the whole stack:

- the catalog stores `raw_path`, `folder`, and `maildir_subdir`
- the deterministic index stores `raw_path`, `folder`, and `maildir_subdir`
- retrieval, search, thread display, and embeddings reopen the raw message by
  path
- local mutations are implemented as filesystem moves and filename rewrites
- remote identity attachment is separate from local storage identity

For a single-consumer system where Vivi is the only mail entry point, Maildir is
carrying more conceptual cost than value.

## Goals

- Replace Maildir with a Vivi-native storage model.
- Use the full SHA-256 of raw RFC 5322 bytes as the durable blob identifier.
- Keep raw message bytes as the only content source of truth.
- Separate immutable message content from mutable mailbox placement and flags.
- Remove `raw_path` and Maildir-specific fields from the catalog and index data
  models.
- Keep remote identity, search, thread lookup, and embeddings rebuildable from
  the new source of truth.
- Treat this as a clean break with no compatibility layer for old Maildir
  layouts.

## Non-Goals

- Preserving interoperability with other mail tools.
- In-place migration of the current Maildir tree.
- Continuing to support `inbox-2050`-style handles.
- Keeping Maildir filename flags or `new`/`cur` semantics.

## Recommendation

Do the rewrite, but do not use the full content hash as the only mutable message
identifier.

Use two identities:

- `content_id`: full 64-hex SHA-256 over the exact raw message bytes
- `message_id`: Vivi-local row identity for one account-local mailbox
  occurrence of that content

The hash is the right identity for the immutable blob. It is not sufficient by
itself for mutable mailbox state because exact-byte duplicates can exist.

## Decisions

The following decisions are now fixed for this rewrite:

- the default user-facing handle is a Git-style short hash derived from
  `message_id`
- `search` and `list` show duplicate message rows when both rows match
- the IMAP server is authoritative for deletion; if a remotely bound message is
  gone there, it should be removed here
- drafts get a `message_id` immediately, before any remote binding exists
- blob paths remain internal and are not part of the normal `vivi` surface
- use one primary `storage.sqlite` database for source-of-truth metadata
- keep embeddings in a separate provider/model-scoped database
- design the blob layout for Git-friendly backup with stable file paths and low
  churn

## Why Hash-Only Is Not Enough

A full content hash cleanly identifies one byte-exact message blob. It does not
cleanly identify one mailbox occurrence.

Cases that break a hash-only mutable model:

- the same exact message bytes can appear in two different accounts
- the same exact bytes can appear twice in one account due to provider or import
  behavior
- the same logical message may need multiple placements during reconciliation or
  transient provider states
- one blob can move across local roles over time while keeping the same content

If the system keys mutable state only by content hash, it will collapse true
duplicates and make remote identity updates ambiguous.

## Proposed Source Of Truth

Use a hash-addressed blob store plus SQLite metadata.

Durable source of truth:

- raw RFC 5322 bytes in a sharded blob tree
- account-local message rows in SQLite
- remote identity rows in SQLite

Derived state:

- thread link index
- extracted text cache if one exists later
- provider/model-scoped embeddings

## Filesystem Layout

Store raw message bytes once per unique content hash:

```text
<mail_root>/
├── blobs/
│   ├── ab/
│   │   ├── cd/
│   │   │   └── abcdef...<64 hex>....eml
├── outbox/
└── .vivarium/
    ├── storage.sqlite
    └── embeddings/
```

Rules:

- `blobs/` is immutable after write.
- the blob filename is the full SHA-256 plus `.eml`
- shard by the first 4 hex chars to avoid large flat directories
- mailbox role is not encoded in the blob path
- read/unread/starred is not encoded in the blob filename

### Git-Friendly Backup Rules

The blob tree should optimize for ordinary Git backup behavior:

- one message blob per file
- stable path derived only from `content_id`
- no renames when local role, flags, or remote state change
- no metadata sidecars next to each blob
- all mutable state kept in SQLite, not scattered across the tree

Why this is Git-friendly:

- a message entering Trash does not rename or rewrite the blob
- duplicate content never creates duplicate blob files
- historical backup churn is mostly append-only as new blobs appear
- content-addressed paths make dedupe and integrity checks obvious

The cost is that SQLite changes are not line-diff-friendly in Git. That is
acceptable because the valuable backup surface for email is the immutable blob
tree, not the mutable mailbox-state database.

## Primary Data Model

Use one primary SQLite database:

```text
<mail_root>/.vivarium/storage.sqlite
```

This replaces the current conceptual split between Maildir layout and
`catalog.sqlite`.

### `blobs`

Immutable content rows.

```sql
CREATE TABLE blobs (
  content_id TEXT PRIMARY KEY,          -- full SHA-256 over raw bytes
  blob_relpath TEXT NOT NULL UNIQUE,    -- blobs/ab/cd/<content_id>.eml
  byte_size INTEGER NOT NULL,
  rfc_message_id TEXT,
  content_sha256 TEXT NOT NULL,         -- same as content_id for clarity
  parsed_at TEXT NOT NULL
);
```

`content_id` and `content_sha256` are the same value. Keeping both is optional.
If the duplication feels noisy, keep only `content_id`.

### `messages`

Mutable account-local mailbox state.

```sql
CREATE TABLE messages (
  message_id TEXT PRIMARY KEY,          -- Vivi-local opaque id
  account TEXT NOT NULL,
  content_id TEXT NOT NULL REFERENCES blobs(content_id) ON DELETE RESTRICT,
  local_role TEXT NOT NULL,             -- inbox, archive, trash, sent, drafts
  read_state INTEGER NOT NULL DEFAULT 0,
  starred INTEGER NOT NULL DEFAULT 0,
  draft_state TEXT,                     -- local, remote, sent, failed
  discovered_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  deleted_at TEXT
);

CREATE INDEX messages_account_role_idx
  ON messages(account, local_role, updated_at);

CREATE INDEX messages_account_content_idx
  ON messages(account, content_id);
```

### `remote_bindings`

Current remote write target for a message row.

```sql
CREATE TABLE remote_bindings (
  message_id TEXT PRIMARY KEY REFERENCES messages(message_id) ON DELETE CASCADE,
  account TEXT NOT NULL,
  provider TEXT NOT NULL,
  remote_mailbox TEXT NOT NULL,
  remote_uid INTEGER NOT NULL,
  remote_uidvalidity INTEGER NOT NULL,
  last_verified_at TEXT NOT NULL,
  stale INTEGER NOT NULL DEFAULT 0,
  UNIQUE (account, remote_mailbox, remote_uidvalidity, remote_uid)
);
```

### `message_metadata`

Parsed header fields for display and filtering.

```sql
CREATE TABLE message_metadata (
  content_id TEXT PRIMARY KEY REFERENCES blobs(content_id) ON DELETE CASCADE,
  date TEXT NOT NULL,
  from_addr TEXT NOT NULL,
  to_addr TEXT NOT NULL,
  cc_addr TEXT NOT NULL,
  bcc_addr TEXT NOT NULL,
  subject TEXT NOT NULL,
  normalized_message_id TEXT
);
```

This is deterministic derived metadata, but keeping it in `storage.sqlite`
avoids reparsing for every list or show command.

## Handle Model

Stop using folder-and-UID handles like `inbox-2050`.

Use:

- `content_id` as the full immutable blob identity
- `message_id` as the stable local mutable identity
- Git-style short hashes of `message_id` as the normal user-facing handle
- content-hash prefixes as an optional citation/debug surface

Example:

- blob: `9a47...` full 64 hex chars
- local row: `msg_01jv...`
- default short handle: `4f8c2d1`
- optional blob citation: `9a47d1c2`

Resolution rule:

- mutation-targeting commands should resolve to `message_id`
- the displayed handle should be the shortest unique prefix of `message_id`
- citation-oriented commands can additionally show a short unique prefix of
  `content_id`
- prefix lookup must error on ambiguity and require a longer prefix

## Sync And Download Flow

### New message download

1. Fetch remote metadata.
2. Download raw bytes.
3. Compute full SHA-256 as `content_id`.
4. Write the blob once if absent.
5. Parse headers and upsert `blobs` + `message_metadata`.
6. Create or update one `messages` row for the account-local role.
7. Attach `remote_bindings`.

### Duplicate content

If the blob already exists:

1. skip rewriting raw bytes
2. reuse `content_id`
3. create or update the message row and remote binding only

Duplicate rows remain distinct at the mailbox-state layer. `list`, `search`,
and counts should show both rows if both rows match.

### Remote move/archive/delete/flag

Do not move files.

Update:

- `messages.local_role`
- `messages.read_state`
- `messages.starred`
- `remote_bindings.remote_mailbox`
- `remote_bindings.remote_uid`
- `remote_bindings.remote_uidvalidity`

This is the main operational simplification over Maildir.

### Remote Deletion Rule

The IMAP server is authoritative for remotely bound messages.

If reconciliation proves that a remotely bound message no longer exists on the
server and no replacement binding is found, Vivi should remove the local
`message` row as well. Blob retention can be a separate garbage-collection
policy, but the mailbox-state row should not pretend the message still exists.

## Retrieval And Listing Consequences

`vivi show`, `thread`, `search`, and export should resolve:

1. `message_id` or content prefix
2. `content_id`
3. blob path from `blobs.blob_relpath`
4. raw bytes from the blob store

`vivi list` should read from `messages` joined with `message_metadata`, not by
directory scan.

This means:

- no more `message_id_from_path`
- no more folder scans for ordinary listing
- no more path rewrites during archive/trash moves

Blob paths should stay internal. If Vivi exposes them at all, it should do so
through one explicit debugging or backup-oriented command rather than surfacing
them in ordinary reads, search results, or mutation flows.

## Storage DB Shape

Use `storage.sqlite` as the only core metadata database. Do not keep a separate
deterministic `index.sqlite`.

The current deterministic index mirrors too much path-oriented state:

- `raw_path`
- `folder`
- `maildir_subdir`
- `handle` derived from filename/path assumptions

That state should be folded back into `storage.sqlite` tables keyed by
`message_id` and `content_id`.

### Search/Thread Tables

If extra derived tables are needed for thread traversal or faster lookup, keep
them in `storage.sqlite` rather than introducing a second core DB.

Example:

```sql
CREATE TABLE message_links (
  account TEXT NOT NULL,
  message_id TEXT NOT NULL,
  link_kind TEXT NOT NULL,
  normalized_message_id TEXT NOT NULL,
  PRIMARY KEY (account, message_id, link_kind, normalized_message_id),
  FOREIGN KEY (message_id) REFERENCES messages(message_id) ON DELETE CASCADE
);
```

Important changes:

- drop `raw_path`
- drop `maildir_subdir`
- replace `handle`/`catalog_handle` split with `message_id` plus `content_id`
- keep `content_id` available for blob lookup and stale detection
- keep all core mutable and deterministic metadata in one DB

## Embeddings Consequences

The embedding model does not need a conceptual rewrite, but its identifiers
should change.

Today embeddings are keyed by:

- `handle`
- `fingerprint`
- `chunk_id`

and they reopen the raw message by `raw_path`.

That should become:

- `message_id`
- `content_id`
- `chunk_id`

### What does not need to change

- chunking policy
- embedding provider contract
- provider/model-scoped DB split
- rebuild semantics
- "no stored chunk text" policy

### What should change

`chunk_id` should be derived from:

- `content_id`
- extractor version
- chunker version
- chunk kind
- chunk ordinal
- text hash

That means embeddings naturally survive any mailbox-role move because the blob
did not change. `content_id` is the primary content identity; `message_id` is
there for account-scoped joins and result targeting.

### Recommended embedding schema

```sql
CREATE TABLE chunks (
  chunk_id TEXT PRIMARY KEY,
  account TEXT NOT NULL,
  message_id TEXT NOT NULL,
  content_id TEXT NOT NULL,
  extractor_version TEXT NOT NULL,
  chunker_version TEXT NOT NULL,
  chunk_kind TEXT NOT NULL,
  chunk_ordinal INTEGER NOT NULL,
  text_hash TEXT NOT NULL,
  token_count INTEGER NOT NULL,
  indexed_at TEXT NOT NULL
);

CREATE INDEX chunks_account_content_idx
  ON chunks(account, content_id);

CREATE INDEX chunks_account_message_idx
  ON chunks(account, message_id);
```

The embedding DB should no longer store or depend on `raw_path`.

### Net effect on embeddings

The rewrite improves embedding stability:

- archive/trash moves no longer invalidate embedding references
- path churn no longer causes stale checks
- one blob can be embedded once and reused across message-row updates

If you want to dedupe embeddings across accounts, `content_id` makes that
possible later. That is optional and should not be part of the first rewrite.

## Search Consequences

Keyword and semantic search result rows should cite:

- `message_id`
- default short handle derived from `message_id`
- optional `content_id`
- account
- local role

If raw blob path is still useful for local citation workflows, it can be exposed
as a derived `blob_path`, but it should not be the primary identifier.

## Remote Identity Consequences

This rewrite does not remove the need for remote identity. It makes the
relationship cleaner.

Today the remote write target is attached indirectly to a catalog row that is
also tied to a mutable filesystem path.

After the rewrite:

- remote identity attaches to `message_id`
- raw content lives independently at `content_id`
- a missing remote identity means "message row has no remote binding", not
  "path backed catalog row is incomplete"

That is a better failure mode and a clearer diagnosis surface.

## Clean-Break Migration

Because this is a clean break and re-download is acceptable, migration should be
simple and destructive.

### Recommended cutover

1. ship the new storage engine behind the normal code paths
2. bump storage schema versions
3. refuse to open old Maildir-backed state as if it were current
4. require `vivi sync --reset` or equivalent fresh bootstrap
5. rebuild index and embeddings from the new blob store

Do not write a compatibility bridge that keeps both Maildir and blob storage
alive. That adds cost without value in this repo.

## Required Follow-On Work

- replace `MailStore` with a blob-store plus placement-store abstraction
- replace path-based lookup with `content_id` and `message_id` resolution
- replace folder scans in `list` with SQLite queries
- rewrite mutation reconciliation to update message rows instead of moving
  files
- replace catalog schema and fold index concerns into `storage.sqlite`
- rewrite thread/search/retrieve citation surfaces away from `raw_path`
- update embedding chunk identity to use `content_id`
- rewrite tests that assume Maildir paths and filename flags
- update README and operational docs to stop describing Maildir as the source of
  truth

## Risks

- exact-byte duplicates will be mishandled if the design uses only `content_id`
  and no separate `message_id`
- blob garbage collection needs an explicit policy once IMAP-authoritative row
  deletion is in place
- provider aggregate views like Gmail or Proton `All Mail` still need explicit
  reconciliation rules
- search and thread tests currently assume path-oriented citations and will need
  a deliberate replacement citation shape

## Recommendation

This rewrite makes architectural sense for Vivi. The current code already wants
content-derived identity, but the Maildir path model keeps pulling the system
back toward location-based state.

The clean version is:

- full SHA-256 blob identity for immutable content
- separate local `message_id` for mutable mailbox state
- SQLite as the mailbox state authority
- index and embeddings rebuilt on top of blob identity, not path identity

That is a better fit for a single-consumer, agent-first mail archive than
Maildir.

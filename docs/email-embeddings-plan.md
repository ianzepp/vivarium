# Email Index And Embeddings Plan

## Problem

Vivi can sync, preserve, catalog, extract, search, mutate, draft, send, and show
local email, but retrieval still depends on live scans in too many places.
Keyword search and thread lookup should not need to parse large Maildir folders
for every command, and semantic retrieval is weak when the user remembers
meaning rather than exact wording.

The implementation needs two separate derived stores:

1. a deterministic metadata index for exact lookup, thread display, stale-state
   detection, and search candidates
2. provider/model-scoped embedding databases for semantic retrieval

Neither store is the source of truth. Raw RFC 5322 `.eml` files remain the only
durable message-content store.

## Goals

- Add a deterministic account-local index for message metadata and thread
  relationships.
- Make `vivi thread` an indexed lookup instead of a broad folder scan.
- Add local-only semantic retrieval over synced email.
- Keep the deterministic index and embedding stores in separate SQLite
  databases.
- Store no message body text or chunk text in either database.
- Embed new or changed messages incrementally.
- Reuse embeddings when raw message content, chunker identity, and model
  identity are unchanged.
- Preserve citations back to Vivi handles, folders, raw paths, and message
  metadata.
- Keep lexical search as the deterministic baseline.
- Make semantic search additive through explicit flags or commands.

## Non-Goals

- Replacing the raw `.eml` archive.
- Replacing `.vivarium/catalog.json` as the current catalog source.
- Replacing the current lexical search path before the index is proven.
- Cloud embedding APIs as a default backend.
- Storing extracted body text in SQLite.
- Storing chunk text in SQLite.
- Summarizing or rewriting email content before embedding as the only index.
- Blocking normal sync on long embedding jobs until the indexer is proven.
- Embedding attachments in the first implementation.

## Source Of Truth

The durable source remains:

- raw RFC 5322 `.eml` bytes in Maildir folders
- catalog entries under `.vivarium/catalog.json`
- extracted text as rebuildable transient output from raw messages
- remote identity metadata for sync and mutation safety

The index and embedding databases are derived state. They must be safe to delete
and rebuild from raw messages plus catalog entries.

## Storage Layout

Use one deterministic index database per account mail root:

```text
<mail_root>/.vivarium/index.sqlite
```

Use separate embedding databases per provider/model:

```text
<mail_root>/.vivarium/embeddings/<provider>-<model>.sqlite
```

Rules:

- `index.sqlite` stores deterministic metadata and relationships only.
- embedding DBs store chunk manifests and vectors only.
- no DB stores message body text, extracted body text, or chunk text.
- embedding DBs can be deleted or rebuilt without breaking exact lookup,
  threading, lexical search, sync, or mutation commands.

## Deterministic Index Schema

`index.sqlite` should start small and boring.

```sql
CREATE TABLE index_metadata (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE messages (
  account TEXT NOT NULL,
  handle TEXT PRIMARY KEY,
  fingerprint TEXT NOT NULL,
  raw_path TEXT NOT NULL,
  folder TEXT NOT NULL,
  maildir_subdir TEXT NOT NULL,
  date TEXT NOT NULL,
  from_addr TEXT NOT NULL,
  to_addr TEXT NOT NULL,
  cc_addr TEXT NOT NULL,
  bcc_addr TEXT NOT NULL,
  subject TEXT NOT NULL,
  rfc_message_id TEXT,
  remote_mailbox TEXT,
  remote_uid INTEGER,
  remote_uidvalidity INTEGER,
  indexed_at TEXT NOT NULL
);

CREATE TABLE message_links (
  account TEXT NOT NULL,
  handle TEXT NOT NULL,
  link_kind TEXT NOT NULL,
  rfc_message_id TEXT NOT NULL,
  PRIMARY KEY (account, handle, link_kind, rfc_message_id),
  FOREIGN KEY (handle) REFERENCES messages(handle) ON DELETE CASCADE
);

CREATE INDEX messages_account_folder_idx ON messages(account, folder, date);
CREATE INDEX messages_rfc_message_id_idx ON messages(account, rfc_message_id);
CREATE INDEX message_links_rfc_idx ON message_links(account, rfc_message_id);
```

`message_links.link_kind` values:

- `message_id`
- `in_reply_to`
- `reference`

The index may later add FTS5 tables for lexical search, but the first
implementation should not require SQLite full-text extensions.

## Embedding Schema

Each provider/model DB should be self-contained for that model while referencing
Vivi handles from the deterministic index.

```sql
CREATE TABLE embedding_metadata (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE chunks (
  chunk_id TEXT PRIMARY KEY,
  account TEXT NOT NULL,
  handle TEXT NOT NULL,
  fingerprint TEXT NOT NULL,
  extractor_version TEXT NOT NULL,
  chunker_version TEXT NOT NULL,
  chunk_kind TEXT NOT NULL,
  chunk_ordinal INTEGER NOT NULL,
  text_hash TEXT NOT NULL,
  token_count INTEGER NOT NULL,
  indexed_at TEXT NOT NULL
);

CREATE TABLE embeddings (
  chunk_id TEXT PRIMARY KEY REFERENCES chunks(chunk_id) ON DELETE CASCADE,
  provider TEXT NOT NULL,
  model TEXT NOT NULL,
  dimensions INTEGER NOT NULL,
  vector BLOB NOT NULL,
  indexed_at TEXT NOT NULL
);

CREATE INDEX chunks_handle_idx ON chunks(account, handle);
CREATE INDEX chunks_fingerprint_idx ON chunks(account, fingerprint);
```

Vector storage can start as a raw little-endian `f32` blob. If a native vector
SQLite extension is introduced later, keep migration explicit and scoped to the
embedding DBs.

## No Stored Text Rule

Text is transient:

1. read raw `.eml`
2. parse/extract text in memory
3. chunk in memory
4. hash each chunk
5. call the embedding provider
6. store chunk metadata and vectors
7. discard extracted text and chunk text

Snippet display must be lazy:

1. semantic search returns `handle`, `chunk_id`, `chunk_ordinal`, and score
2. Vivi resolves the handle through `index.sqlite` or the catalog
3. Vivi reads the raw `.eml`
4. Vivi re-runs the recorded extractor/chunker version
5. Vivi uses `chunk_ordinal` and `text_hash` to regenerate the display snippet

If regenerated text does not match the stored `text_hash`, the result should be
marked stale and excluded or returned with an explicit stale warning.

## Chunking Policy

Use a hybrid chunking policy.

Every embedded email should get at least one message-level chunk:

```text
Subject: ...
From: ...
To: ...
Cc: ...
Date: ...

<first useful body text>
```

Then add body chunks when needed:

- Short normal emails: one body chunk is enough.
- Long newsletters, legal mail, quote-heavy replies, and HTML dumps: split into
  multiple body chunks.
- Oversized lines must be split defensively before embedding.
- Quoted prior messages should either be downweighted or split so they do not
  dominate the current message's semantic representation.

Initial practical rule:

- message-level chunk: metadata plus first 2-4 KB of extracted text
- body chunks: approximately 1,000-1,500 words or provider-safe byte windows
- overlap: small overlap, around 100-200 words, only for long body chunks

The chunker should operate on extracted text, not raw MIME. The extracted text
is never written to the index.

## Stable IDs And Reuse

Chunk IDs should be deterministic across runs.

Suggested identity fields:

```text
account
handle
raw fingerprint
extractor version
chunker version
chunk ordinal
chunk kind
embedding provider
embedding model
embedding dimensions
```

Suggested chunk ID input:

```text
account + handle + fingerprint + extractor_version + chunker_version + kind + ordinal
```

Embedding reuse rule:

- If raw fingerprint, extractor version, chunker version, provider, model, and
  dimensions match, reuse the existing embedding.
- If any of those change, re-embed only affected chunks.
- If the deterministic index says a handle moved folders but its fingerprint is
  unchanged, update index metadata without re-embedding.

## Deterministic Indexing Flow

For each catalog entry:

1. Confirm raw path exists.
2. Read raw bytes and verify or compute fingerprint.
3. Parse headers once.
4. Upsert `messages`.
5. Replace `message_links` for that handle with normalized `Message-ID`,
   `In-Reply-To`, and `References` values.
6. Remove stale rows for handles no longer present in the catalog.
7. Emit progress to stderr.
8. Return a concise count summary on stdout.

Example summary:

```text
indexed personal-proton: scanned=42 updated=3 reused=39 stale=0 errors=0
```

## Embedding Flow

For each indexed message selected for embedding:

1. Resolve the handle through `index.sqlite`.
2. Read raw `.eml` from the indexed raw path.
3. Verify the raw fingerprint still matches.
4. Extract body text in memory.
5. Build deterministic chunks in memory.
6. Check chunk metadata and embedding rows for reuse.
7. Batch pending chunk text for the local embedding provider.
8. Upsert chunk metadata and embeddings transactionally.
9. Discard all text buffers.
10. Return a concise count summary on stdout.

Example summary:

```text
embedded personal-proton: scanned=42 reused=39 embedded=3 stale=0 errors=0
```

## Thread Query Flow

`vivi thread <handle> --json` should use `index.sqlite`.

1. Resolve seed handle in `messages`.
2. Load the seed `rfc_message_id` and linked `in_reply_to`/`reference` IDs.
3. Find direct replies with `message_links.rfc_message_id = seed.rfc_message_id`.
4. Walk parent references for the seed when available.
5. Fetch ordered message metadata from `messages`.
6. Read raw `.eml` only for final JSON body/snippet rendering.

This turns thread display into indexed metadata lookup plus bounded final reads,
not a scan of every message in `INBOX`, `Archive`, `Sent`, and `Drafts`.

## Embedding Provider

Default to local embeddings.

Preferred first backend:

```text
Ollama /api/embed
```

This keeps private email content local by default.

Configuration should name both provider and model explicitly:

```toml
[defaults]
embedding_provider = "ollama"
embedding_model = "cassio-embedding"
embedding_endpoint = "http://127.0.0.1:11434/api/embed"
```

Do not hide the real model identity in persisted metadata. A friendly alias can
exist in config, but the resolved provider/model identity must be stored in the
embedding DB metadata.

## Commands

Add deterministic indexing before embedding:

```sh
vivi index rebuild
vivi index pending
vivi index status
vivi index rebuild --account personal-proton
```

Add embedding commands after deterministic indexing exists:

```sh
vivi index embeddings
vivi index embeddings --pending
vivi index embeddings --account personal-proton
vivi index embeddings --rebuild
```

Later, add sync integration:

```sh
vivi sync --index
vivi sync --embed
```

Rules:

- `sync --index` may run after sync persistence succeeds.
- `sync --embed` should require or imply deterministic indexing first.
- `sync --embed` should not make successful mail sync depend on embedding
  service availability unless explicitly requested.

Search should stay additive:

```sh
vivi search "query" --semantic
vivi search "query" --hybrid
```

Rules:

- default `vivi search` remains lexical
- `--semantic` uses embedding similarity and returns chunk citations
- `--hybrid` combines lexical and semantic signals
- all results include handle, account, folder, raw path, chunk ID, and snippet
- snippets are regenerated from raw `.eml`, not loaded from the DB

## Semantic Query Flow

For `vivi search --semantic "query"`:

1. Embed query locally with the same provider/model as the selected embedding
   DB.
2. Load candidate vectors from the embedding DB.
3. Compute cosine similarity in-process.
4. Join handles to `index.sqlite` metadata.
5. Return top-k chunks with citations.
6. Regenerate snippets lazily from raw `.eml`.
7. Optionally collapse multiple chunks from the same handle.

For `--hybrid`:

1. Run lexical search for deterministic exact-match candidates.
2. Run semantic search for meaning-based candidates.
3. Merge by handle/chunk.
4. Apply simple scoring weights.
5. Return citations with both lexical and semantic scores when available.

## Concurrency

Use the same future account lock described in
`docs/sync-state-and-reconciliation-plan.md` for local index writes:

- catalog reads can be concurrent
- SQLite writes should be serialized by the database and account lock
- embedding provider calls can happen outside the lock
- final upsert of chunk/embedding rows should re-check fingerprint before commit

This matters because another Vivi process can sync, mutate, or reconcile the
same account while indexing is running.

## Privacy And Safety

- Default embedding backend must be local.
- Cloud embedding providers require explicit config and command confirmation.
- Never include secrets, passwords, OAuth tokens, or local config contents in
  embedding text.
- Treat attachment embedding as a later opt-in feature.
- Do not persist message body text, extracted body text, chunk text, or snippets
  in SQLite.
- Treat vectors as sensitive derived data even though they are not readable
  message text.
- Provider/model embedding DBs can have separate backup, deletion, and rebuild
  policy from the deterministic index.

## Tests

Deterministic index tests:

- index DB opens under `.vivarium/index.sqlite`
- catalog entry upserts a `messages` row
- `Message-ID`, `In-Reply-To`, and `References` create normalized links
- changed folder updates message metadata without changing handle identity
- changed raw fingerprint refreshes indexed metadata
- removed catalog entry removes stale index rows
- thread lookup finds a sent reply by `References`
- thread lookup avoids broad Maildir folder scans

Chunker tests:

- chunker creates one message-level chunk for short mail
- chunker splits long body text deterministically
- chunk IDs are stable across runs
- oversized single lines split below provider limit
- no chunk text is written to SQLite

Embedding tests:

- embedding DB path is provider/model scoped
- changed raw fingerprint invalidates reuse
- changed embedding model invalidates reuse
- pending index embeds only missing chunks
- rebuild clears only the selected provider/model embedding DB
- failed provider call leaves existing embeddings intact
- transactional write does not leave orphan embeddings
- deleted/moved local message is handled without panicking
- query similarity returns expected nearest chunk from small fixtures

CLI tests:

- parser accepts `vivi index rebuild`
- parser accepts `vivi index pending`
- parser accepts `vivi index embeddings`
- parser accepts `--pending`, `--rebuild`, and account scoping
- parser accepts `vivi search --semantic`
- parser accepts `vivi search --hybrid`

Mock provider tests:

- local `/api/embed` request shape is correct
- provider dimension mismatch is rejected
- provider errors are reported without corrupting index state

## Stop Conditions

Pause implementation if:

- extracted text is not stable enough to use as embedding input
- local embedding provider cannot expose dimensions/model identity reliably
- initial indexing blocks normal sync for too long
- SQLite vector storage becomes too large without a retention/rebuild policy
- search results cannot preserve citations back to raw messages and chunks
- snippet regeneration from raw `.eml` cannot reliably match chunk metadata

## Suggested Implementation Order

1. Add `index.sqlite` schema, migration metadata, and index path helpers.
2. Add deterministic index upsert from catalog entries.
3. Add `message_links` extraction and tests.
4. Rewrite `vivi thread` to use `index.sqlite`.
5. Add `vivi index rebuild`, `pending`, and `status`.
6. Add chunking module that returns transient chunk text plus persisted chunk
   metadata.
7. Add provider trait plus Ollama `/api/embed` backend.
8. Add provider/model-scoped embedding DB schema.
9. Add `vivi index embeddings --pending`.
10. Add semantic query over the selected embedding DB.
11. Add `vivi search --semantic`.
12. Add hybrid merge after semantic search is useful on real mail.
13. Add optional `sync --index`.
14. Add optional `sync --embed` once indexing is reliable.

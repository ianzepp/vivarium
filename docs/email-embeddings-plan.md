# Email Embeddings Plan

## Problem

Vivi can sync, preserve, catalog, extract, search, mutate, draft, and send local
email, but retrieval is still mostly lexical. Keyword search is useful for exact
terms, handles, senders, and citations, but it is weak when the user remembers
meaning rather than wording.

Email embedding should be a derived local index layered on top of raw `.eml`,
Maildir storage, catalog entries, and extracted text. It must not become the
source of truth, and it must not send private email content to a cloud embedding
provider by default.

## Goals

- Add local-only semantic retrieval over synced email.
- Embed new or changed messages incrementally.
- Reuse embeddings when raw message content and model identity are unchanged.
- Support short one-chunk messages and long multi-chunk messages.
- Preserve citations back to Vivi handles, folders, raw paths, and message
  metadata.
- Keep lexical search as the deterministic baseline.
- Make semantic search additive through explicit flags or commands.

## Non-Goals

- Replacing the raw `.eml` archive.
- Replacing the current lexical search path.
- Cloud embedding APIs as a default backend.
- Summarizing or rewriting email content before embedding as the only index.
- Blocking normal sync on long embedding jobs until the indexer is proven.
- Embedding attachments in the first implementation.

## Source Of Truth

The durable source remains:

- raw RFC 5322 `.eml` bytes in Maildir folders
- catalog entries under `.vivarium/catalog.json`
- extracted text as rebuildable derived state
- remote identity metadata for sync and mutation safety

Embeddings are derived state and must be rebuildable from those sources.

## Chunking Policy

Use a hybrid chunking policy.

Every indexed email should get at least one message-level chunk:

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

The chunker should operate on extracted text, not raw MIME.

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

## Storage

Store indexes under the account mail root:

```text
<mail_root>/.vivarium/embeddings/<provider>-<model>.sqlite
```

Use provider/model-scoped SQLite databases so model migrations do not corrupt or
confuse existing indexes.

Proposed tables:

```sql
CREATE TABLE index_metadata (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE chunks (
  chunk_id TEXT PRIMARY KEY,
  account TEXT NOT NULL,
  handle TEXT NOT NULL,
  folder TEXT NOT NULL,
  maildir_subdir TEXT NOT NULL,
  raw_path TEXT NOT NULL,
  rfc_message_id TEXT,
  fingerprint TEXT NOT NULL,
  chunk_kind TEXT NOT NULL,
  chunk_ordinal INTEGER NOT NULL,
  extractor_version TEXT NOT NULL,
  chunker_version TEXT NOT NULL,
  text_hash TEXT NOT NULL,
  text TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE embeddings (
  chunk_id TEXT PRIMARY KEY REFERENCES chunks(chunk_id) ON DELETE CASCADE,
  provider TEXT NOT NULL,
  model TEXT NOT NULL,
  dimensions INTEGER NOT NULL,
  vector BLOB NOT NULL,
  created_at TEXT NOT NULL
);

CREATE INDEX chunks_handle_idx ON chunks(account, handle);
CREATE INDEX chunks_fingerprint_idx ON chunks(fingerprint);
```

Vector storage can start as a raw little-endian `f32` blob. If a native vector
SQLite extension is introduced later, keep migration explicit.

## Embedding Provider

Default to local embeddings.

Preferred first backend:

```text
Ollama /api/embed
```

This matches the already-proven local semantic-index pattern in nearby tooling
and keeps private email content local by default.

Configuration should name both provider and model explicitly:

```toml
[defaults]
embedding_provider = "ollama"
embedding_model = "cassio-embedding"
embedding_endpoint = "http://127.0.0.1:11434/api/embed"
```

Do not hide the real model identity in persisted metadata. A friendly alias can
exist in config, but the resolved provider/model identity must be stored in the
index metadata.

## Commands

Add a separate indexing command before adding sync-integrated embedding:

```sh
vivi index embeddings
vivi index embeddings --pending
vivi index embeddings --account personal-proton
vivi index embeddings --rebuild
```

Later, add sync integration:

```sh
vivi sync --embed
```

`sync --embed` should enqueue or run embedding for newly cataloged/extracted
messages only after sync persistence succeeds. It should not make successful
mail sync depend on embedding service availability unless explicitly requested.

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

## Indexing Flow

For each catalog entry:

1. Confirm raw path exists.
2. Read raw bytes and verify or compute fingerprint.
3. Load extracted body text, or extract on demand if missing.
4. Build deterministic chunks.
5. Check existing chunk and embedding rows for reuse.
6. Batch pending chunk texts for the local embedding provider.
7. Store chunks and embeddings transactionally.
8. Emit progress to stderr.
9. Return a concise count summary on stdout.

Example summary:

```text
indexed personal-proton: scanned=42 reused=39 embedded=3 skipped=0 errors=0
```

## Query Flow

For `vivi search --semantic "query"`:

1. Embed query locally with the same provider/model as the selected index.
2. Load candidate vectors from SQLite.
3. Compute cosine similarity in-process.
4. Return top-k chunks with citations.
5. Optionally collapse multiple chunks from the same handle.

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

- Default backend must be local.
- Cloud embedding providers require explicit config and command confirmation.
- Never include secrets, passwords, OAuth tokens, or local config contents in
  embedding text.
- Treat attachment embedding as a later opt-in feature.
- Keep raw text in the local SQLite index only if the user accepts that index as
  sensitive local data; otherwise store snippets and hashes while embedding from
  transient text.

Initial recommendation: store chunk text locally. It makes index debugging,
reranking, and citation snippets much simpler, and the index lives under the
same private account mail root.

## Tests

Unit tests:

- chunker creates one message-level chunk for short mail
- chunker splits long body text deterministically
- chunk IDs are stable across runs
- changed raw fingerprint invalidates reuse
- changed embedding model invalidates reuse
- oversized single lines split below provider limit
- query similarity returns expected nearest chunk from small fixtures

Indexer tests:

- pending index embeds only missing chunks
- rebuild clears provider/model index safely
- failed provider call leaves existing embeddings intact
- transactional write does not leave orphan embeddings
- deleted/moved local message is handled without panicking

CLI tests:

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

## Suggested Implementation Order

1. Add an embedding planning/delivery artifact for the first implementation
   phase.
2. Add chunking module and tests.
3. Add provider trait plus Ollama `/api/embed` backend.
4. Add SQLite schema and provider/model-scoped index path.
5. Add `vivi index embeddings --pending`.
6. Add semantic query over the local SQLite index.
7. Add `vivi search --semantic`.
8. Add hybrid merge after semantic search is useful on real mail.
9. Add optional `sync --embed` once indexing is reliable.

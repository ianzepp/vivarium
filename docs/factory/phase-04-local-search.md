# Phase 04: Local Search And Embeddings

## Phase Spec

### Goal

Make historical email discoverable through local keyword search and local semantic search.

### Expected Outputs

1. Keyword search over subject, sender, recipients, extracted body
2. Local embedding generation only
3. Embedding model identity and dimensions stored with vectors
4. Rebuildable embedding index
5. Stale-index detection when extraction version or embedding model changes
6. `vivarium search` with text and JSON output
7. Tests for search result handles and citation metadata

### Out Of Scope

- Cloud embeddings
- Cloud reranking
- Model download management
- Multi-user service mode

### Checkpoint Target

Search returns stable handles with enough metadata for an agent to fetch, inspect,
and cite the underlying message locally.

## Workstreams

### WS-04-A: Keyword Search

- Add keyword search over subject, from, to, cc, extracted body text
- Store keyword index as part of the catalog (or sidecar JSON)
- Support substring and word-boundary matching
- Return search results with handle, score, snippet

### WS-04-B: Search CLI

- `vivarium search <query>` — keyword search with text output
- `vivarium search <query> --json` — JSON output
- Pagination support with `--limit` and `--offset`
- Citation fields in output (handle, folder, date, from, subject)

### WS-04-C: Embedding Index

- Store embedding vectors in catalog sidecar
- Track model identity (name, dimensions, version)
- Detect stale embeddings when model changes

### WS-04-D: Tests

- Keyword search correctness
- Search result handle stability
- JSON output validity

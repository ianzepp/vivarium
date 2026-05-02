# Email Index Phase 03 Delivery

## Phase Name

Semantic and hybrid email search.

## Input

- `docs/email-embeddings-plan.md`
- Phase 1 deterministic index commit `62b4d6e`
- Phase 2 embedding indexer commit `70b0f03`

## Problem

Vivi can now build deterministic indexes and provider/model-scoped embedding
databases, but the user-facing search command still only supports lexical
matching. Semantic retrieval needs to query the embedding DB without persisting
message text in SQLite.

## Scope

- Add `vivi search --semantic`.
- Add `vivi search --hybrid`.
- Embed the query through the selected local provider/model.
- Load vectors from the selected embedding DB and compute cosine similarity
  in-process.
- Join matches to `index.sqlite` metadata.
- Regenerate snippets lazily from raw `.eml` and chunk metadata.
- Keep default `vivi search` lexical behavior unchanged.

## Out Of Scope

- Faster approximate nearest-neighbor indexes.
- Full FTS5 lexical replacement.
- Cross-model vector comparison.
- Sync-integrated embedding.
- Storing snippets or chunk text in SQLite.

## Acceptance Criteria

- `vivi search --semantic` is accepted by the CLI.
- `vivi search --hybrid` is accepted by the CLI.
- Semantic JSON results include handle, account, folder, raw path, chunk ID,
  score, snippet, and citation.
- Snippets are regenerated from raw `.eml`.
- Hybrid search keeps lexical matches and semantic matches additive.
- Default lexical search output remains compatible.

## Checkpoint

Run:

```sh
cargo test
cargo build
cargo clippy --all-targets -- -D warnings
target/debug/vivi search --help
target/debug/vivi search "compose test" --semantic --limit 3 --json
target/debug/vivi search "compose test" --hybrid --limit 3 --json
```

The live semantic checks require the Phase 2 embedding smoke or a larger
embedding index to exist locally.

## Gate

PASS if semantic and hybrid search work from the embedding DB, default lexical
search remains unchanged, and no SQLite text persistence is introduced.

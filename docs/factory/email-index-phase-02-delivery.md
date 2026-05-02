# Email Index Phase 02 Delivery

## Phase Name

Chunk manifests, embedding DBs, and explicit embedding command.

## Input

- `docs/email-embeddings-plan.md`
- Phase 1 deterministic index commit `62b4d6e`

## Problem

Vivi now has deterministic metadata indexing for exact lookup and threads, but
it still has no semantic embedding pipeline. The next slice should add the
derived embedding store without mixing it into `index.sqlite` and without
persisting message text.

## Scope

- Add transient email chunking from raw `.eml` plus extracted text.
- Add provider/model-scoped SQLite DBs under
  `.vivarium/embeddings/<provider>-<model>.sqlite`.
- Store chunk manifests and vectors only.
- Add an Ollama `/api/embed` provider.
- Add `vivi index embeddings` with `--pending`, `--rebuild`, account scoping
  through global `--account`, and a bounded `--limit` for smoke runs.
- Reuse existing embeddings when chunk identity and vector row already exist.

## Out Of Scope

- `vivi search --semantic`
- hybrid search
- sync-integrated embedding
- cloud embedding providers
- attachment embedding
- persisting body text, chunk text, snippets, summaries, or extracted text

## Acceptance Criteria

- Embedding DB path is provider/model scoped.
- The embedding DB schema has no body-text or chunk-text columns.
- Chunk IDs are deterministic.
- Oversized body text is split before provider calls.
- Existing chunk/vector rows are reused.
- Provider dimension mismatch is rejected.
- Failed provider calls do not corrupt existing rows.
- `vivi index embeddings --pending --limit N` is accepted by the CLI.

## Checkpoint

Run:

```sh
cargo test
cargo build
cargo clippy --all-targets -- -D warnings
target/debug/vivi index embeddings --help
```

Live provider smoke is optional for this phase because Ollama/model availability
is host state. If a local embedding model is available, run a small bounded
smoke:

```sh
target/debug/vivi index embeddings --pending --limit 2 --account personal-proton
```

## Gate

PASS if chunking and embedding storage are implemented with tests, no text is
persisted in SQLite, and embedding command execution is bounded and explicit.

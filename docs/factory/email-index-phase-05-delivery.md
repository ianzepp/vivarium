# Email Index Phase 05 Delivery

## Phase Name

Embedding hardening and failure safety.

## Input

- `docs/email-embeddings-plan.md`
- Phase 2 embedding indexer commit `70b0f03`
- Phase 3 semantic search commit `0a4e9fb`
- Phase 4 sync integration commit `0aac21f`

## Problem

The embedding path exists, but the plan calls out provider failure, rebuild
scoping, model scoping, stale raw messages, and deleted local files as safety
requirements. The rebuild path must not destroy usable vectors before a local
embedding provider has successfully returned replacement vectors.

## Scope

- Preserve existing embeddings if provider calls fail during rebuild.
- Keep rebuild pruning scoped to the selected provider/model DB and account.
- Add direct provider request-shape coverage for the Ollama `/api/embed`
  backend.
- Add tests for stale fingerprints, model-scoped DBs, provider failure,
  rebuild scope, and deleted local files.

## Out Of Scope

- Background embedding queues.
- Cloud embedding providers.
- Account locks or sync-state cursors.
- Native vector SQLite extensions.
- Attachment embeddings.

## Acceptance Criteria

- Failed rebuild provider calls leave existing embeddings intact.
- Rebuild pruning runs only after a complete successful unbounded rebuild.
- Different embedding models use separate DB files and do not invalidate each
  other.
- Deleted local messages and stale raw fingerprints count as errors or stale
  without panicking.
- Ollama provider tests assert the POST body uses `model` and `input`.

## Checkpoint

Run:

```sh
cargo test
cargo build
cargo clippy --all-targets -- -D warnings
git diff --check
```

## Gate

PASS if the hardening tests fail against unsafe rebuild behavior and pass with
the corrected implementation, while the prior CLI and search/index behavior
remains green.

# Email Index Phase 04 Delivery

## Phase Name

Optional sync integration for deterministic indexing and embeddings.

## Input

- `docs/email-embeddings-plan.md`
- Phase 1 deterministic index commit `62b4d6e`
- Phase 2 embedding indexer commit `70b0f03`
- Phase 3 semantic search commit `0a4e9fb`

## Problem

Index and embedding commands are explicit, but normal sync does not yet offer a
way to run the derived indexing work after message persistence. Users should be
able to opt in without making baseline sync depend on local embedding service
availability.

## Scope

- Add `vivi sync --index`.
- Add `vivi sync --embed`.
- `--index` runs deterministic index rebuild after sync persistence succeeds.
- `--embed` implies deterministic indexing first, then provider/model-scoped
  embedding with default local Ollama settings.
- Baseline `vivi sync` remains unchanged when neither flag is present.

## Out Of Scope

- `sync --new`
- sync-state cursors
- reconciliation
- background embedding queues
- provider/model flags on `sync`

## Acceptance Criteria

- CLI accepts `vivi sync --index`.
- CLI accepts `vivi sync --embed`.
- `--embed` runs deterministic indexing before embeddings.
- Baseline sync does not require Ollama.
- Post-sync indexing happens only after sync succeeds.

## Checkpoint

Run:

```sh
cargo test
cargo build
cargo clippy --all-targets -- -D warnings
target/debug/vivi sync --help
```

Live sync checks are optional because they hit Proton Bridge. A safe local smoke
can use:

```sh
target/debug/vivi sync --limit 0 --index --account personal-proton
```

## Gate

PASS if sync integration is opt-in, ordered after sync persistence, and tests
cover the new CLI surface.

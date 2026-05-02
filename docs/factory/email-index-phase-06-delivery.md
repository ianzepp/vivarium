# Email Index Phase 06 Delivery

## Phase Name

Indexed lexical search cutover.

## Input

- `docs/email-embeddings-plan.md`
- Phase 1 deterministic index commit `62b4d6e`
- Phase 3 semantic search commit `0a4e9fb`
- Phase 5 embedding hardening commit `be31de0`

## Problem

Default lexical search still enumerates fixed Maildir folders directly. That
keeps search coupled to folder layout and misses the plan goal that exact
retrieval should use the deterministic index as the candidate source.

## Scope

- Change `vivi search` lexical mode to load candidates from `index.sqlite`.
- Use indexed raw paths and metadata for citations.
- Score indexed metadata first, then read raw `.eml` only for matched snippets
  and fingerprint validation.
- Skip stale indexed rows when the raw file is missing or its fingerprint no
  longer matches.
- Add tests proving unindexed Maildir files are not searched.

## Out Of Scope

- SQLite FTS5.
- Full-body lexical search without an FTS/text index.
- Search result ranking changes beyond preserving the existing score function.
- Automatic index rebuild on search.
- Semantic or hybrid scoring changes.

## Acceptance Criteria

- Default lexical search no longer calls fixed-folder Maildir enumeration.
- Default lexical search does not sweep every raw `.eml` body.
- Results preserve handle, account, folder, subdir, date, sender, subject, raw
  path, and snippet citations from indexed metadata.
- Missing or changed raw files do not panic.
- Tests cover indexed search and the unindexed-file boundary.

## Checkpoint

Run:

```sh
cargo test
cargo build
cargo clippy --all-targets -- -D warnings
target/debug/vivi search "compose test" --limit 3 --json --account personal-proton
git diff --check
```

## Gate

PASS if lexical search uses deterministic index candidates, current tests pass,
and live search returns promptly from the existing local index.

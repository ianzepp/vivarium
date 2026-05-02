# Phase 05: Agent Interface

## Interpreted Problem

Phase 04 made local email discoverable through `vivi search`. Phase 05 turns the
read/search archive into a predictable agent-facing CLI surface: JSON retrieval,
thread context, bounded search, raw/text export, and explicit citation metadata
that points back to original local `.eml` files.

Some Phase 05-adjacent pieces already exist: `show --json`, `search --json`, and
raw `export`. This phase completes the contract rather than replacing those
surfaces.

## Phase Spec

### Goal

Expose the archive as a predictable local tool surface for agents and local
LLMs.

### Inputs

- catalog
- extraction artifacts
- search index
- current local Maildir store
- existing `show`, `search`, and `export` commands

### Expected Outputs

1. `vivi show <handle> --json`
2. `vivi thread <handle> --json`
3. `vivi search <query> --json`
4. `vivi export <handle>` for raw `.eml` export
5. `vivi export <handle> --text` for normalized local text export
6. explicit citation fields in every JSON response
7. bounded result sizes and pagination
8. clear errors for missing, stale, or corrupted artifacts

### Out Of Scope

- MCP server
- daemon/API server
- cloud model integration
- sending mail
- upstream IMAP mutation
- incremental catalog/extraction/search updates

### Checkpoint Target

A local agent can search, retrieve, thread, and cite email using CLI JSON without
needing direct unrestricted filesystem traversal.

## Workstreams

### WS-05-A: Message Location And Citation Model

- Add a store-level lookup that resolves a handle to folder, Maildir subdir, and
  raw path.
- Add a shared JSON citation shape:
  - handle
  - account
  - folder
  - maildir subdir
  - raw path
  - source type
- Use citation fields in `show --json`, `thread --json`, and `search --json`.

### WS-05-B: JSON Retrieval

- Keep `vivi show <handle> --json`.
- Include the extracted body, headers, normalized RFC Message-ID, and citation.
- Return clear errors for missing handles and parse failures.
- Support multiple handles in one call.

### WS-05-C: Thread Retrieval

- Add `vivi thread <handle> --json`.
- Build thread context locally by scanning account Maildir folders.
- Match the seed message, its `Message-ID`, `In-Reply-To`, and `References`
  headers.
- Return a bounded chronological message list with citations.
- For now, omit non-JSON text thread output unless needed later.

### WS-05-D: Export Contract

- Preserve `vivi export <handle>` as raw RFC 5322 output.
- Add `vivi export <handle> --text` for normalized local text output.
- Keep export local-only and handle-scoped.

### WS-05-E: Tests And Docs

- Add focused unit tests for citation shape, thread matching, and text export.
- Update README command examples and supported/not-yet-supported lists.
- Run `cargo test` and live CLI probes against the local cache where safe.

## Verification Commands

```sh
cargo test
cargo run --quiet -- show <known-handle> --json
cargo run --quiet -- thread <known-handle> --json
cargo run --quiet -- search <query> --json --limit 1
cargo run --quiet -- export <known-handle> --text
```

Use actual local handles for live probes and record any skipped probe if the
local cache has no suitable messages.

## Gate Plan

| Gate | Trigger | Pass Criteria | Fail Action |
|------|---------|---------------|-------------|
| Build + Tests | After implementation | `cargo test` passes | Fix compilation or test failures |
| CLI Contract | After build/tests | show/thread/search JSON include citations; export raw/text works | Repair command behavior |
| Checkpoint | Before commit | Agent can search, retrieve, thread, and cite without broad filesystem traversal | Rework Phase 05 surface |

## Checkpoint Result

Phase 05 completed.

Implemented:

- `vivi thread <handle> --json --limit <n>`
- `vivi export <handle> --text`
- citation metadata for `show --json`
- citation metadata for `search --json`
- bounded thread output
- message location lookup for folder, Maildir subdir, and raw path
- README command surface update

Validation:

```sh
cargo test
cargo run --quiet -- --help
cargo run --quiet -- thread --help
cargo run --quiet -- export --help
cargo run --quiet -- list -n 1
cargo run --quiet -- show inbox-2040 --json
cargo run --quiet -- thread inbox-2040 --json --limit 5
cargo run --quiet -- search "AI" --json --limit 1
cargo run --quiet -- export inbox-2040 --text | wc -c
```

Live cache probe used handle `inbox-2040`. The JSON commands included citation
objects with account, folder, Maildir subdir, raw path, and source type. Text
export returned 384 bytes for that handle.

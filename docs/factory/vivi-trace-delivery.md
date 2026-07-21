# Delivery Spec: `vivi trace` Command

## Interpreted Unit

Add a top-level `vivi trace <handle>` command to Vivarium that reconstructs the
communication tree around any local mailspace handle. The command works with the
existing storage model and follows captured reply links, lifecycle events, and
inferred body-citation links, collapsing multi-copy messages into one logical
node.

## Normalized Spec

### Functional Requirements

- Accept any resolvable mailspace handle (task, want, need, mail, memo).
- Build a directed graph of related messages with the seed as root.
- Edge sources:
  - `captured`: from `mailspace_links` with `source = 'captured'` (reply headers).
  - `event`: from `mailspace_events` (`tasked` → want→task, `moved` → lifecycle).
  - `inferred`: from body handle citations or subject matching (reuse existing
    thread inference).
  - `copy`: between rows sharing the same `content_id` (sender copy / recipient
    copy).
- Collapse `copy` edges so the tree node is the `content_id`; display the set of
  `(account, role)` copies.
- Support `--json` for structured output and `--max-depth` / `--limit` bounds.
- Bound graph traversal so large mailspaces do not explode.
- Preserve error handling in `VivariumError` with `thiserror`.

### Constraints

- No storage schema migration.
- No new external dependencies.
- No change to existing CLI behavior.
- Local-mailspace only.
- File/function size ceilings apply.

### Non-goals

- Explicit trace headers (`X-Vivi-Trace-Root`, etc.).
- External account tracing.
- Semantic goal ontology.
- Release/version bump.

## Repo-Aware Baseline

- `src/cli.rs`: top-level `Command` enum. New `TraceCommand` variant here.
- `src/local_mailspace_command.rs`: dispatch logic. New `handle_trace_command`
  function.
- `src/mailspace/thread.rs`: existing `Mailspace::thread`, `add_inferred_links`,
  `connected_content_ids`.
- `src/mailspace/lifecycle.rs`: `task_from_source` records `tasked` events with
  `active_tasks=<handle>`.
- `src/mailspace/dump.rs`: pattern for loading events and links by message.
- `src/storage/handles.rs`: `resolve_message_token` and `display_handle`.
- `src/storage.rs`: `Storage` message/event/link queries.
- `tests/local_mailspace_cli.rs`: extend with trace tests.
- `test-data/faberlang-vivi/mail.sqlite`: fixture for manual/automated trace
  verification.
- `README.md`: CLI reference section.

## Stage Graph

```text
[Delivery Spec] → [Graph Builder] → [CLI + Renderer] → [Tests] → [Docs] → [Verify + Commit]
```

| Stage | Inputs | Outputs | Dependencies | Verification |
| --- | --- | --- | --- | --- |
| 1. Graph Builder | goal doc, thread.rs, storage APIs | `src/mailspace/trace.rs` with `TraceGraph`, `TraceNode`, `TraceEdge` | none | unit tests for graph builder |
| 2. CLI + Renderer | `TraceCommand` in cli.rs, dispatch | `vivi trace` text and JSON output | stage 1 | manual CLI smoke |
| 3. Tests | Faberlang fixture, local mailspace | extended CLI tests | stages 1-2 | `cargo test --test local_mailspace_cli` |
| 4. Docs | README | new command section | stage 2 | visual inspection |
| 5. Verify + Commit | all changes | passing tests, committed work | all | `cargo fmt`, `cargo test --test hygiene`, `cargo test` |

## Implementation Work

1. **Graph Builder** (`src/mailspace/trace.rs`)
   - Define `TraceNode` (content_id, messages grouped by account/role, metadata,
     edges).
   - Define `TraceEdge` (target_content_id, source kind, direction).
   - Implement `Mailspace::trace(handle, max_depth, limit) -> TraceGraph`.
   - Load seed, all links, all events, and build adjacency list.
   - Add inferred edges by scanning body handle citations.
   - Add event edges from `tasked` notes and `moved` events where relevant.
   - BFS/DFS with depth/limit bounds.

2. **CLI + Renderer**
   - Add `TraceCommand` to `src/cli.rs` with `handle`, `--json`, `--max-depth`,
     `--limit`, `--project`.
   - Add `handle_trace_command` in `src/local_mailspace_command.rs`.
   - Implement `print_trace` / `print_trace_json` in `src/mailspace/trace.rs`.
   - Text output: indented tree with handle, kind, account/role, subject, and
     edge source.

3. **Tests**
   - Add `tests/local_mailspace_cli.rs` cases that invoke `vivi trace` on the
     Vivarium `.vivi` mailspace and assert the expected handle and reply appear.
   - Add a fixture-based test (or a manual verification script) that traces the
     Faberlang `d5e0bcf` → `096bc357` / `fc593d5` chain.

4. **Docs**
   - Add `vivi trace` entry to README CLI reference with example.

## Checkpoints And Gates

- **Gate 1**: graph builder compiles and unit tests pass.
- **Gate 2**: `vivi trace` command resolves a handle and prints expected output.
- **Gate 3**: `cargo test --test local_mailspace_cli` passes with new trace tests.
- **Gate 4**: `cargo fmt --check`, `cargo test --test hygiene`, and `cargo test`
  pass.
- **Gate 5**: README updated and review findings addressed.

### Batching / Split Decision

This is a single-phase delivery. All stages are tightly coupled (the CLI needs
the graph builder, tests need the CLI), so they should not be split into separate
factory phases. The deferred explicit-header enhancement is captured as future
work.

## Validation

- `cargo fmt --check`
- `cargo test --test hygiene`
- `cargo test --test local_mailspace_cli`
- `cargo test`
- Manual: `vivi trace d9c2ae04` in Vivarium `.vivi`.
- Manual: `vivi trace d5e0bcf` against Faberlang fixture reaches expected nodes.

## Open Questions

- `vivi trace` vs `vivi mailspace trace`: default to `vivi trace` unless dispatch
  wiring strongly favors the subcommand. Decision is non-blocking.
- Default `--max-depth`: 5. Default `--limit`: 100.

## Companion Skill Plan

- `factory` for execution supervision.
- `correctness` for traversal and edge-classification audit.
- `cleanliness` for structural review of new `src/mailspace/trace.rs`.
- `polish` for final file-level cleanup.

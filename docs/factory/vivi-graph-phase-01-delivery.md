# Delivery Spec: Work Graph Phase 1 — Foundation and Atomic Mermaid Import

## Interpreted Unit

Add project-local work-graph storage, a narrow Mermaid flowchart parser, and
CLI commands to import a graph atomically and show it. On first import, Vivi
assigns immutable handles, stores normalized nodes and edges, preserves Mermaid
source as revision evidence, and reports the initial ready frontier (root
nodes). Invalid input must leave the database unchanged.

Vision source: `docs/factory/vivi-graph-goal.md` Phase 1.

## Normalized Spec

### Functional Requirements

1. Schema tables in project-local `mail.sqlite` for graphs, revisions, nodes,
   edges, and graph events.
2. Parse a narrow Mermaid profile: `flowchart`/`graph` + direction, quoted
   node labels, `-->` edges (including chains), `subgraph` as display group
   only, comments ignored.
3. Reject unsupported diagram kinds, missing edge endpoints, duplicate source
   IDs, and cycles before any write.
4. `vivi graph import --code <code> --file <path> [--check] [--json]`:
   - `--check`: parse/validate/diff only; no writes.
   - import: atomic transaction; fail if graph code exists with different
     content; idempotent when content hash matches.
5. `vivi graph show <code|handle> [--json]`: topology, states, ready set,
   blocked-by for open nodes with unsatisfied prereqs.
6. Stable JSON contracts for import report and show.
7. Node handles are stable for a given graph + Mermaid source ID.

### Constraints

- Extend existing `mail.sqlite`; no second database.
- No new crate dependencies for the parser.
- Standalone `X-Vivi-Depends-On` task behavior unchanged.
- File ≤ 1000 lines; functions ≤ 60 lines.
- Errors via `VivariumError`; clap derive; no panics in production paths.

### Non-goals (this phase)

- `graph apply`, `node add`, `edge add`
- Node activate/complete and task-attempt binding
- `board --graph`, readiness events for watch
- Mermaid export with state styling
- Fleet companion adapter
- Conditional edge guards

## Repo-Aware Baseline

| Surface | Role |
| --- | --- |
| `src/storage/schema.rs` | Schema version + DDL |
| `src/storage.rs` + `src/storage/graph.rs` | Storage APIs |
| `src/mailspace/mermaid.rs` | Mermaid profile parser |
| `src/mailspace/graph.rs` | Import, show, ready derivation, render |
| `src/cli.rs` + `src/cli/mailspace_command.rs` | `Graph` subcommands |
| `src/local_mailspace_command.rs` | Dispatch |
| `src/main.rs` | Unreachable arm for mailspace commands |
| `tests/local_mailspace_cli.rs` | CLI integration |
| Parser unit tests | Inline in mermaid/graph modules |

## Stage Graph

```text
[Schema+Storage] → [Parser] → [Domain import/show] → [CLI] → [Tests] → [Verify]
```

| Stage | Outputs | Verification |
| --- | --- | --- |
| 1. Schema + storage | tables, CRUD, transactions | storage unit tests |
| 2. Parser | AST + validation + cycle check | parser unit tests |
| 3. Domain | import/check/show + ready | domain unit tests |
| 4. CLI | `graph import`, `graph show` | CLI smoke |
| 5. Tests | fixtures: fan-out, cycle, idempotent | integration |
| 6. Verify | hygiene + full suite | gate |

## Implementation Work

1. Bump storage schema version; add `work_graphs`, `work_graph_revisions`,
   `work_graph_nodes`, `work_graph_edges`, `work_graph_events`.
2. Storage methods: get-by-code, insert graph+revision+nodes+edges+events in
   one transaction, load graph view.
3. Mermaid parser producing nodes (source_id, label, subgraph), edges
   (from, to, optional label), direction.
4. Validate acyclicity via DFS; compute ready roots (open nodes with no
   unfinished prerequisites; on first import all open → roots).
5. Import path: parse → validate → if exists check hash → commit or no-op.
6. CLI + text/JSON renderers.
7. Tests covering success, `--check`, cycle, missing endpoint, duplicate ID,
   idempotent re-import, rollback (failed import leaves empty).

## Checkpoints And Gates

- Gate 1: schema opens and migrations apply on fresh and v1 DBs.
- Gate 2: parser accepts profile fixture and rejects cycle/unsupported.
- Gate 3: import JSON returns handle, revision, counts, roots, ready.
- Gate 4: `cargo fmt --check`, `cargo test --test hygiene`,
  `cargo test --test local_mailspace_cli`, `cargo test`.

### Batching / Split Decision

Single factory phase. Schema, parser, domain, and CLI are one checkpoint:
import is not useful without show, and show is not useful without storage.

## Validation

```bash
cargo fmt --check
cargo test --test hygiene
cargo test mermaid
cargo test graph
cargo test --test local_mailspace_cli graph
cargo test
```

Fixtures:

- Fan-out: `verify --> accept --> u2` and `accept --> u3`
- Cycle: `a --> b --> a` must fail with zero writes
- Idempotent: import twice same file → same handles, revision stays 1

## Companion Skill Plan

- correctness: transaction rollback, readiness derivation
- cleanliness: keep parser / storage / domain / CLI boundaries
- polish: phase-modified files before commit

## Open Questions

None blocking. Defaults from the goal doc:

- `import` creates; existing different content errors (apply is Phase 2)
- source IDs unique within graph; labels non-unique display
- subgraph = display metadata only

## Release Decision

Defer. Phase 1 alone is not the full minor-release surface (Phases 2–3 still
pending per goal release posture).

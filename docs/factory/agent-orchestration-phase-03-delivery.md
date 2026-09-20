# Agent Orchestration Phase 03: Lowering As Expansion

## Interpreted Phase Problem

When a need is lowered into recorded tasks, the graph has no record of the
composition: the need's node and its unit tasks are unrelated nodes, and
nothing derives the need's completion from its units landing. Phase 03 makes
lowering a graph fact and the need's completion the join of its units.

## Normalized Phase Spec

### Goal

`vivi need bind <need-handle> <task-handle>...` records unit composition on
the backlog graph; completing the last bound unit auto-completes the need
(and unlocks its dependents).

### Functional Requirements

- Binding sets the unit node's `subgraph` to the parent need's source id
  (join marker; no edges invented — composition is not a dependency).
- Bind requires: backlog graph exists, parent node exists and is open, each
  unit node exists (task sent first) in any state. One `unit_bound` graph
  event per unit.
- Join rule: when every node whose `subgraph` names an open parent is done,
  the parent auto-completes (normal completion semantics: events, unlock of
  dependents). Checked after every backlog completion and after bind (retro
  binding of already-done units completes the need immediately).
- Recursive: an auto-completed parent that is itself a bound unit cascades.

### Constraints

- Backlog-graph scoped; `graph apply`/import surfaces unchanged.
- No schema change (`subgraph` column exists). `src/local_mailspace_command.rs`
  stays under ceiling (handler lives in `local_work_command.rs`).
- Standard gates green; hygiene totals ratchet to measured.

### Out Of Scope

- `vivi step`, provider, docs (later phases).
- Bind for wants (a promoted want is a need by then).
- Mixed graph/import topology composition.

## Repo-Aware Baseline

- Phase 1–2 surface: `src/mailspace/backlog.rs`, `src/storage/backlog_graph.rs`.
- `work_graph_nodes.subgraph` column exists (set at import, unused by
  backlog minting).
- Need CLI: `NeedCommand` in `src/cli/mailspace_command/work_command.rs`;
  handlers in `src/local_work_command.rs`.

## Stage Graph

1. Storage: `bind_backlog_units` (tx: subgraph updates + events) and
   `work_graph_nodes_by_subgraph` query.
2. Domain: `backlog_bind_units`, `backlog_complete_if_join_complete`, join
   check at the tail of `backlog_complete_item`.
3. CLI: `NeedCommand::Bind`, handler, parse test.
4. Tests: join completion incl. cascade and retro bind; rejection paths;
   dependent unlock only after full join; integration via `board --graph`.

## Validation

- `cargo test --lib backlog`; integration `need bind` flow; full gates.

## Decisions

- Composition via `subgraph` marker, not edges: an edge would make a unit
  indistinguishable from an ordinary dependency, and the join rule would
  fire on mere dependents.
- Units must already be sent (nodes exist); bind does not mint units. Keeps
  send as the single minting path.

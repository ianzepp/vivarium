# Delivery Spec: Work Graph Phase 3 — Board, Binding, Events

## Unit

Expose graph frontier on `vivi board --graph`, bind task attempts to nodes,
activate ready nodes, emit durable ready events on complete, and let watch
surface `node_ready` graph events.

## Requirements

1. Schema: `work_graph_attempts` (node ↔ task attempt).
2. `vivi graph activate <graph>:<id> --task <handle>` — refuse if not ready/open;
   bind attempt; set node `active`.
3. Complete recalculates readiness and emits `node_ready` events for newly ready
   successors (no agent spawn).
4. `vivi board --graph [--json]` adds `graphs[]` with node code, state,
   readiness, blocked-by handles, successors; keeps existing board fields.
5. `mailspace watch` accepts kind `graph` and event `node_ready`.

## Non-goals

Fleet prepare/claim (Phase 4). Auto-complete on task done.

## Validation

`cargo fmt --check`, hygiene, local_mailspace_cli graph/board tests, `cargo test`.

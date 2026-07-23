# Delivery Spec: Work Graph Phase 2 — Lifecycle, Apply, Export

## Interpreted Unit

Extend Phase 1 graphs with atomic apply revisions, small node/edge appends,
explicit node completion, mutation freezes for active/done history, and Mermaid
export with optional state styling. Ready/blocked remain derived.

Vision source: `docs/factory/vivi-graph-goal.md` Phase 2.

## Normalized Spec

### Functional Requirements

1. `vivi graph apply <graph> --file <path> [--check] [--json]`
   - Reconcile by Mermaid source ID.
   - Add open nodes/edges; update labels on open unassigned nodes.
   - Reject changing prerequisites of active nodes.
   - Reject mutating done nodes or their incoming edges.
   - Allow new successors on active/done nodes.
   - Identical content hash → idempotent (no new revision).
2. `vivi graph node add --graph <code> --id <source-id> [--label <text>]`
3. `vivi graph edge add --graph <code> --from <id> --to <id> [--label <text>]`
4. `vivi graph complete <graph>:<source-id> [--note <text>]`
   - Marks node `done`; recalculates ready frontier.
5. `vivi graph show --mermaid [--include-state]` (and/or `graph export`)
   - Normalized topology; optional class/state styling for LLM readability.
6. Mutation rules from the goal doc enforced in one transaction path.

### Non-goals

- Task-attempt binding / activate-with-task (Phase 3)
- board --graph (Phase 3)
- Fleet adapter (Phase 4)
- Conditional edge guards

## Stage Graph

```text
[Apply domain] → [Complete + append] → [Mermaid export] → [CLI] → [Tests]
```

## Checkpoints

- Apply additive fan-out succeeds; prereq rewrite on done fails.
- Complete unlocks successors into ready.
- Export re-imports without semantic change (topology).
- `cargo fmt --check`, hygiene, local_mailspace_cli, `cargo test`.

## Release

Still defer minor release until Phase 3 per goal posture.

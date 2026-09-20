# Agent Orchestration Phase 05: Step Apply And Decision Records

## Interpreted Phase Problem

Graph transitions happen (nodes complete, dependents unlock) but leave no
decision record: a later reader reconstructing why a node unlocked sees state
changes and events, not who decided the transition and through which path.
And the host has no single-command settle-side contract.

## Normalized Phase Spec

### Goal

Every node completion writes a `step_decision` graph event atomically with
the transition, carrying the deciding path (`via=lifecycle`,
`via=graph-complete`, `via=step-apply`). `vivi step --apply <handle>`
adjudicates one settled item, completes its node if still completable, and
emits the manifest with the decision listed.

### Functional Requirements

- `Storage::complete_work_graph_node` takes a `decision` note and writes the
  `step_decision` event inside the same transaction as the state change and
  `node_ready` events (atomic by construction).
- `vivi step --apply <handle>`: item must resolve; a not-yet-settled item
  (not in `done`) becomes a `not_settled` exception — apply never settles
  work itself. A settled item completes its backlog node via
  `backlog_complete_item_via(..., "via=step-apply")`, which returns whether a
  transition occurred; only real transitions append to `manifest.decisions`.
  Re-running apply on an already-done node is a clean no-op.
- Apply vocabulary stays bounded: complete (when already settled) or hold.
  No path exists to settling items, admission, verdicts, or novel topology.

### Constraints

- No schema change (decision = graph event). Read paths untouched.
- Standard gates green; hygiene ratchet to measured.

### Out Of Scope

- Judgment provider screens (deferred — see goal amendment).
- Fleet-side consumption of the manifest (host territory).

## Decisions

- Atomicity is by construction (same transaction); the induced-failure test
  from the goal is satisfied structurally rather than by fault injection —
  there is no code path that writes the decision without the transition.
- Manifest `decisions` lists only transitions performed by the current call;
  the durable per-transition records are the `step_decision` events.

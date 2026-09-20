# Vivarium 8.3.0

Vivarium 8.3.0 adds `vivi step`: a mechanical adjudication of the backlog
graph into a dispatch/exception manifest, plus apply mode and atomic
decision records for graph transitions. It completes the orchestration
fast-path goal's core: the host gets one command where it previously
re-derived readiness and exceptions from board reads in context.

This is a minor-version release. No storage schema change: decision records
are graph events (`step_decision`) written in the same transaction as the
node completion they explain.

## Highlights

### `vivi step` — the dispatch/exception manifest

```sh
vivi step --project <root> --json
vivi step --project <root>            # human-readable
```

Every ready node in the `backlog` graph is adjudicated mechanically.
Dispatchable work lands under `dispatches` with kind, subject, `done_when`
clause count, and `write_scope` presence. Nodes needing attention land under
`exceptions` with a reason vocabulary that encodes standing protocol
rulings:

- `want_requires_promotion` — wants never dispatch before explicit promotion
- `lowered_awaiting_units` — a lowered need completes via its unit join
- `no_done_when` — a task whose completion could never be verified
- `item_missing` — a node whose message no longer resolves

Read-only: no mutation, no network, no provider. The `decisions` key is part
of the stable contract and fills only in apply mode.

### `vivi step --apply <handle>` — the settle-side contract

```sh
vivi task done --for hand <handle> --verdict pass --repo vivarium --tip abc123
vivi step --apply <handle> --project <root> --json
```

Apply completes a settled item's graph node if it is still open and lists
the transition under `decisions`. An unsettled item becomes a `not_settled`
exception — apply never settles work itself, and re-running on done nodes is
a clean no-op. The vocabulary is bounded to complete-or-hold: no path exists
from step to settling, admission, verdicts, or novel topology.

### Atomic decision records

Every backlog node completion — by lifecycle move, `graph complete`, or
`step --apply` — writes a `step_decision` graph event in the same
transaction as the state change, recording the deciding path
(`via=lifecycle`, `via=graph-complete`, `via=step-apply`). A later reader
reconstructs why a node unlocked from records alone.

## Behavior notes

- 8.2.0's backlog citizenship, unified `--depends-on`, and `need bind` join
  semantics carry forward unchanged.
- The judgment-provider integration from the orchestration goal is
  explicitly deferred (see the goal's status block); `vivi step` is fully
  functional without it.

## Goal and delivery records

- Goal: `docs/agent-orchestration-fast-path-goal.md` (status block)
- Deliveries: `docs/factory/agent-orchestration-phase-04-delivery.md`
  (step shadow), `.../phase-05-delivery.md` (apply + decision records)

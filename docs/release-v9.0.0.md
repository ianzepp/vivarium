# Vivarium 9.0.0

Vivarium 9.0.0 makes the project mailspace's whole backlog one executable
dependency graph and gives coordination hosts a machine contract for
orchestration. This is a major version because the default behavior of
everyday commands changes: work items become graph citizens at send time,
dependency validation is fail-fast, and the storage schema runs a one-time
repair pass over existing mailspaces.

This release folds the untagged 8.2.0/8.3.0 checkpoints (substrate, step,
decision records) plus the gap-closeout dogfood work into one major. The
process was verified end to end on this repository's own board: needs filed,
lowered, bound, dispatched, settled, and applied; joins closed parents;
provider screens wrote real calibration records.

## Breaking and behavior changes

- **Schema 7 — one-time upgrade repair.** `ensure_schema` now runs its
  idempotent DDL for upgrades, not only fresh databases, and the version
  bump forces one repair pass. This fixes tables that shipped inside an
  existing schema version never reaching older mailspaces — most visibly
  `vivi goal add` failing with `no such table: mailspace_goals`. Existing
  mailspaces migrate on first open; no data changes.
- **Task `--depends-on` fails fast.** Unknown or ambiguous dependency
  handles fail the send before anything is created. They previously
  accepted any string into `X-Vivi-Depends-On` headers and only broke at
  `task list --blocked` time.
- **Every work-kind send writes graph rows.** Tasks, needs, and wants mint
  a node (plus dependency edges) in the per-mailspace `backlog` graph at
  send time. `board --graph` shows the backlog graph alongside imported
  topologies once work items exist.
- **Lifecycle moves sync graph state.** `task done` / `need done` /
  `want done|drop` complete the item's node and unlock dependents; `reopen`
  re-locks, cascading through join parents and content siblings.
- **`step_decision` graph events** record every node completion's deciding
  path (`via=lifecycle|graph-complete|step-apply|sibling`) in the same
  transaction as the transition.

## One dependency substrate

```sh
vivi task send ... --depends-on <handle>   # task, need, or want handles
vivi need send ... --depends-on <handle>
vivi want send ... --depends-on <handle>
```

Dependencies become prerequisite edges; readiness spans kinds and is
computed, not inferred. Multi-recipient sends are one work item: edges
canonicalize to one deterministic work-role copy per content, and
completing any copy completes its siblings.

## Lowering as a graph fact

```sh
vivi need bind <need-handle> <task-handle> ...
```

Bound units join the need's subgraph; the need auto-completes when every
unit lands (retroactive binding of done units completes immediately).
Reopening a unit re-opens a done parent.

## `vivi step` — the dispatch/exception manifest

```sh
vivi step --project <root> --json
vivi step --apply <settled-handle> --project <root>
```

`step` adjudicates ready backlog nodes mechanically: dispatches carry kind,
subject, done_when clause counts, and write_scope presence; exceptions use
a fixed reason vocabulary (`want_requires_promotion`,
`lowered_awaiting_units`, `no_done_when`, `item_missing`, `not_settled`).
Wants never dispatch before explicit promotion. `--apply` completes a
settled item's node idempotently (never settles it) and lists transitions
under `decisions`. The taught dispatch sequence is
`task send` → `graph activate --task` → spawn, so in-flight work leaves the
manifest.

## Judgment provider (optional, off by default)

```toml
# user-level config.toml (often ~/.config/vivarium/config.toml)
[judgment]
provider = "typesafe"
key_cmd  = "cat ~/.config/secrets/typesafe-ai.key"
```

When configured, `step --apply` screens the settled item's receipt through
TypeSafe System One (one Noul per `done_when` clause plus a
completion-honesty check) and appends answers to
`.vivi/judgment-corpus.jsonl`. Shadow by design: provider answers never
gate mechanical completions; absent, unreachable, or timed-out providers
degrade to `judgment=skipped(<class>)`. Authentication is exclusively
`key_cmd` (`sh -c`, `password_cmd` semantics) — there is no inline key
field and no environment-variable path, so the secret is neither
file-leakable nor ambient to spawned processes. No read path makes
provider calls.

## Goal registry repaired

`vivi goal add` works on mailspaces created before the goals table existed
(the schema-7 repair above). Goals register and surface on the board.

## Deferred

- Provider-gated apply: flips receipt screens from recorded to gating only
  after calibration on real corpus data (separate ruling).
- `task list --blocked` still derives from `X-Vivi-Depends-On` headers
  (same input as the edges; no divergence today).

## Records

- Goal: `docs/agent-orchestration-fast-path-goal.md` (status block)
- Closeout: `docs/agent-orchestration-gap-closeout-goal.md`
  (`gol_2bf5462296239d6a`, verification log)
- Deliveries: `docs/factory/agent-orchestration-phase-01..06-delivery.md`

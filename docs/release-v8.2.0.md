# Vivarium 8.2.0

Vivarium 8.2.0 makes the project mailspace's whole backlog one executable
dependency graph. Tasks, needs, and wants mint work-graph nodes at send time,
`--depends-on` becomes graph edges on every work kind, and `vivi need bind`
records lowering so a need completes itself when its unit tasks land.

This is a minor-version release. No storage schema change: the unified
backlog graph is an ordinary work graph (code `backlog`) using the existing
tables. Existing mailspaces get the feature on first need/want/task send.

## Highlights

### Backlog citizenship at send

```sh
vivi need send  --from mind --to cto --subject "audit wave" --body "..."
vivi want send  --from mind --to cto --subject "retry helper" --body "..."
```

Every task, need, and want send mints an open node in the per-mailspace
`backlog` graph (node `source_id` = the item handle). `vivi graph ready` and
`vivi board --graph` now answer "what can run now" across the entire backlog,
not only explicitly imported topologies.

### One dependency substrate

```sh
vivi task send ... --depends-on <handle>   # task, need, or want handles
vivi need send ... --depends-on <handle>
vivi want send ... --depends-on <handle>
```

Dependencies become prerequisite→dependent edges in the `backlog` graph;
task→task, task→need, and cross-kind dependencies all derive readiness the
same way. Dependency handles are validated before anything is created — an
unknown handle fails the send. Completing an item (`task done`, `need done`,
`want done`/`want drop`) completes its node and unlocks dependents;
`task reopen` / `need reopen` re-lock them. `X-Vivi-Depends-On` headers
remain on the message as evidence.

Wants never promote automatically. A want whose dependencies are all met
stays a want until `vivi want promote` runs.

### Lowering as a graph fact

```sh
vivi need bind <need-handle> <task-handle> ...
```

Binding records unit composition: each unit task's node joins the need's
subgraph. When every bound unit is done, the need auto-completes (unlocking
its own dependents); retroactively binding already-done units completes the
need immediately. Bind requires an open need node and already-sent units.

## Behavior notes

- Task `--depends-on` now fails fast on unresolvable handles. It previously
  accepted any string into headers and only failed at `task list --blocked`
  time.
- `board --graph` shows the `backlog` graph alongside imported topologies.
  Agents that assert exact graph lists in scripts should expect the extra
  graph once work items exist.
- Mailspace events for backlog minting are graph events
  (`backlog_attached`, `unit_bound`) in `work_graph_events`, not mailspace
  events.

## Goal and delivery records

- Goal: `docs/agent-orchestration-fast-path-goal.md`
- Deliveries: `docs/factory/agent-orchestration-phase-01-delivery.md`
  (citizenship), `.../phase-02-delivery.md` (task unification),
  `.../phase-03-delivery.md` (lowering join)

Later phases of the same goal (`vivi step` adjudication, optional judgment
provider) are not part of this release.

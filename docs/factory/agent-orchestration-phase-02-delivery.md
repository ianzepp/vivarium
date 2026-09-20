# Agent Orchestration Phase 02: Task Dependency Unification

## Interpreted Phase Problem

Tasks still carry dependencies as a parallel mechanism: `X-Vivi-Depends-On`
headers read back by `task list --blocked` / `is_task_done`, disconnected from
work-graph edges. Phase 02 makes task `--depends-on` the same substrate: task
sends mint backlog nodes, task deps become edges (task→task, task→need,
task→want), and task lifecycle moves complete/reopen nodes so dependents of
any kind unlock mechanically.

The goal's Phase 2 is split at the behavior-family boundary: this delivery is
task unification only; lowering-as-expansion follows as Phase 03.

## Normalized Phase Spec

### Goal

One dependency substrate: every work item (task/need/want) mints a backlog
node at send; `--depends-on` is edges; lifecycle moves keep node state in
step; readiness spans kinds.

### Functional Requirements

- Minting moves into `Mailspace::send` for all three work kinds (role+kind
  gated); CLI layers stop attaching manually.
- Dep validation runs inside `send` before anything is created: a bad
  dependency handle fails the send with no message minted.
- Task handles are accepted as dependencies (Phase 1 refusal removed); open
  tasks mint `open` dep nodes, done items mint `done` nodes.
- `task done` completes the task's node (unlocking dependents); `task reopen`
  reopens it. Existing need/want semantics unchanged.
- Headers (`X-Vivi-Depends-On`) remain message evidence; graph edges are the
  readiness mechanism. `task list --blocked` keeps its header-based view this
  phase (it reads the same `--depends-on` input).

### Constraints

- `src/local_mailspace_command.rs` is at 972/1000 lines: send_task must not
  grow (validation inside `send` makes that possible).
- No schema changes; hygiene totals ratchet to measured values; standard gates
  green.

### Out Of Scope

- Lowering-as-expansion / join completion (Phase 03).
- Rewriting `task list --blocked` to read edges.
- `vivi step`, provider, docs (later phases).

## Repo-Aware Baseline

- `send_task` in `src/local_mailspace_command.rs:785` (untouched this phase).
- `Mailspace::send` in `src/mailspace/delivery.rs:31` — single send funnel
  for all kinds; `SendRequest` already carries `kind` and `depends_on`.
- Phase 1 surface: `src/mailspace/backlog.rs` (attach/validate/complete/
  reopen/sync), `src/storage/backlog_graph.rs`.

## Stage Graph

1. Domain: extend `resolve_backlog_dep` to accept tasks; extend
   `sync_backlog_node` to tasks; gate + validate + attach inside `send`.
2. CLI: drop manual validate/attach from `send_local_item`.
3. Tests: task mint, task→task and need→task unlock, task done/reopen sync,
   cross-kind readiness; invalid dep fails send before creation.

## Validation

- `cargo test --lib backlog`; integration: task send with task dep → blocked
  → `task done` → ready across kinds; parse coverage unchanged (task flag
  already tested). Full gates before commit.

## Decisions

- Dep validation tightened: task `--depends-on` previously accepted any
  string into headers; it now fails fast on unresolvable handles. Honest
  tightening — dangling deps already broke `task list --blocked` at read time.
- Auto-mint applies to programmatic task sends too (source-task minting from
  wants in lifecycle), which is consistent citizenship, not a regression.

# Agent Orchestration Phase 01: Backlog Graph Citizenship

## Interpreted Phase Problem

Work graphs are a separate artifact agents must remember to maintain. `task
send` has structured `--depends-on`; `need send` / `want send` carry
dependencies only as body prose; nothing compiles either into graph topology.
Phase 01 makes needs and wants graph citizens at birth so `graph ready` /
`board --graph` answer "what can run now" across the whole backlog with no
manual graph step, and lifecycle moves keep node state in sync.

## Normalized Phase Spec

### Goal

Every `need send` / `want send` mints a node in a per-mailspace `backlog` work
graph; `--depends-on` on need/want send creates dependency edges; completing a
dependency unlocks dependents mechanically; wants never promote implicitly.

### Functional Requirements

- `need send` / `want send` mint one open node per delivered copy in the
  `backlog` graph (auto-created on first mint). Node `source_id` = the item's
  display handle; label = subject.
- `--depends-on <handle>` (repeatable) on need/want send: dependency handles
  must resolve to needs/wants (open or done); each becomes an edge
  prerequisite→item. Missing nodes for dependencies mint retroactively, with
  `done` state when the dependency item is already done.
- Dependency headers (`X-Vivi-Depends-On`) are written on the item message,
  matching task behavior.
- `need done`, `want done`, `want drop` complete the item's node (emitting
  `node_ready` events for newly unlocked dependents); `need reopen` reopens it.
  Items without nodes (pre-feature backlog) are skipped silently. `want
  promote` does not change node state.
- A want whose dependencies are all complete remains a want until `want
  promote` runs.
- `graph ready` / `board --graph` surface the backlog graph like any other
  (no projection changes expected).

### Constraints

- No schema changes: reuse `work_graph*` tables; the `backlog` graph is an
  ordinary graph row.
- Mints insert directly (one transaction), not via Mermaid re-export/apply; no
  revision bump per mint. Revision 1 is synthesized at graph creation only.
- Refuse task handles in `--depends-on` with a clear error (task↔graph
  unification is Phase 2).
- Production errors in `VivariumError`; hygiene ceilings on touched files;
  `cargo fmt --check`, `cargo test --test hygiene`, `cargo test` green.

### Out Of Scope

- Task `--depends-on` → graph edge unification (Phase 2).
- Lowering-as-expansion (Phase 2).
- `vivi step` (Phases 3–5), provider integration, docs/skills updates
  (Phase 6), release publication.

## Repo-Aware Baseline

- Goal source: `docs/agent-orchestration-fast-path-goal.md` (Phase 1).
- Storage: `src/storage/graph.rs` — `insert_work_graph` / `insert_edges` free
  fns (private; made `pub(super)`), public handle fns, `complete_work_graph_node`,
  `set_work_graph_node_state`.
- Domain: `src/mailspace/graph_mutate.rs` — `ready_handles` /
  `newly_ready_after_done` / `validate_source_id` (private; made `pub(super)`).
- Send path: `send_local_item` in `src/local_work_command.rs` builds
  `SendRequest` (depends_on currently always empty for need/want).
- Lifecycle: `Mailspace::move_item` (`src/mailspace/delivery.rs`) is the single
  funnel for need/want/task folder moves; `before.local_role` + target role
  identify the transition.
- CLI: `NeedCommand::Send(LocalSendCommand)` / `WantCommand::Send(LocalSendCommand)`
  in `src/cli/mailspace_command/work_command.rs`; `TaskSendCommand` in
  `src/cli/mailspace_command.rs` is the `--depends-on` pattern.
- Display handles are ≥8-char hex (`short_handle_map`), valid as node
  source_ids (`[A-Za-z0-9_-]+`).

## Stage Graph

1. Storage mint primitive (`src/storage/backlog_graph.rs`, new)
   - `mint_backlog` transaction: ensure `backlog` graph (revision 1 with
     synthesized Mermaid), insert nodes with explicit state, insert edges
     (skip existing), `backlog_attached` events per minted node.
2. Domain surface (`src/mailspace/backlog.rs`, new)
   - `backlog_validate_deps`, `backlog_attach`, `backlog_complete_item`,
     `backlog_reopen_item`; node lookup by `source_id`; no-op when graph or
     node absent.
3. Lifecycle hook (`src/mailspace/delivery.rs`)
   - After a successful `move_item`: needs/wants→done completes the node;
     done→needs/wants reopens it.
4. CLI (`work_command.rs`, `mailspace_command.rs`, `local_work_command.rs`)
   - `NeedSendCommand` / `WantSendCommand` wrappers with `--depends-on`;
     pre-send dep validation; post-send attach per delivered handle.
5. Tests
   - Parse tests (`tests/cli.rs`); unit tests (`src/mailspace/backlog_test.rs`);
     end-to-end integration test (`tests/local_mailspace_cli.rs`): send with
     dep → blocked → `need done` → ready → board shows it → want not promoted.

## Validation

- `cargo test -p vivarium --lib` for new unit tests;
  `cargo test --test local_mailspace_cli backlog` for the integration test.
- Full gates before commit: `cargo fmt --check`, `cargo test --test hygiene`,
  `cargo test`.
- Manual smoke on a temp mailspace: `need send --depends-on`, `graph ready`,
  `need done`, `graph ready`, `want list`.

## Decisions

- Node identity: `source_id` = display handle (Open Question 2 resolved — no
  separate mapping table; the backlog graph namespace is the mapping).
- Deps to done items mint `done` nodes so dependents unlock immediately.
- Attach failure after a successful send errors loudly (item exists, node
  missing); pre-send validation makes this a rare race, not a flow.

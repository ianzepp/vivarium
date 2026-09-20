# Agent Orchestration Phase 04: `vivi step` Shadow

## Interpreted Phase Problem

The host (Mind/Fleet) has no single machine contract for "what can run now
and what needs attention." It re-derives readiness and exceptions from board
reads in context. Phase 04 ships the read half of `vivi step`: a mechanical,
read-only adjudication pass over the backlog graph producing a stable
dispatch/exception manifest.

## Normalized Phase Spec

### Goal

`vivi step [--json]` adjudicates every ready backlog node mechanically and
emits `dispatches` / `exceptions` / `decisions` (empty in shadow). No
mutation, no network, no provider.

### Adjudication rules (v1, all mechanical)

- Ready node whose item message is gone → exception `item_missing`.
- Ready want → exception `want_requires_promotion` (wants never dispatch
  before explicit promotion — standing operator ruling).
- Ready need with bound units (subgraph non-empty) → exception
  `lowered_awaiting_units` (completion derives from the join, not new work).
- Ready task/need with no `done_when:` clause in its body → exception
  `no_done_when` (completion could never be verified later).
- Otherwise → dispatch entry carrying clause count, `write_scope` presence,
  kind, subject, node handle.

### Constraints

- Read-only: no graph/mailspace writes, no network, provider-less.
- Output contract stable for host consumption (JSON schema documented in the
  release notes at Phase 6; text form for humans).
- New files (`src/mailspace/step.rs`, `src/local_step_command.rs`) keep
  `local_mailspace_command.rs` under its ceiling.
- Standard gates green; hygiene ratchet to measured.

### Out Of Scope

- Apply mode and decision records (Phase 05).
- Provider screens (Phase 06, optional).
- Suggested role/band in dispatch entries (host policy, not Vivi's).

## Repo-Aware Baseline

- `graph_show("backlog")` gives ready nodes; `work_graph_nodes_by_subgraph`
  (Phase 03) detects lowered parents; `read_message` + `extract_text` give
  bodies; `local_role` gives kind.
- Top-level command routing: `src/main.rs` `run_mailspace_command` family,
  `Command` enum in `src/cli.rs`.

## Stage Graph

1. Domain: `Mailspace::step_shadow()` → `StepManifest` struct (serde).
2. CLI: `Command::Step { project, json }`; dispatch via new
   `src/local_step_command.rs`; main.rs routing.
3. Tests: unit adjudication matrix; integration `vivi step --json`; parse
   test.

## Validation

- `cargo test --lib step`; integration; full gates.

## Decisions

- `decisions` key present-but-empty in shadow so the Phase 05 apply contract
  does not change shape.
- Body-field detection is the labeled-line convention (`done_when:`,
  `write_scope:`) already used by Tugboat task bodies; no new body format.

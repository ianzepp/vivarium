# Write Phase 08: Exec, Enqueue, And Queue Run Surface

## Interpreted Phase Problem

The existing `vivi agent` command names the caller instead of the effect. It
emits JSON dry-run plans, while ordinary write commands execute immediately.
That makes the safety model hard to reason about: callers need to know which
surface plans, which surface executes, and whether a plan is durable.

## Normalized Phase Spec

### Goal

Replace the ambiguous agent planning surface with explicit effect-oriented
commands:

- `vivi exec ...` executes external writes immediately.
- `vivi enqueue ...` persists intended writes for later review.
- `vivi queue ...` lists, inspects, drops, and runs queued work.

### Expected Outputs

- Remove `vivi agent ...` from the active CLI surface.
- Add `vivi exec archive|delete|move|flag|send`.
- Add `vivi enqueue archive|delete|move|flag|send|reply`.
- Add `vivi queue list|show|drop|run`.
- Queue entries are durable JSON files under the selected account mail root.
- Help text clearly distinguishes immediate execution from deferred queueing.
- Parser and queue-storage tests cover the clean-break behavior.

### Out Of Scope

- Interactive approval prompts.
- Cloud-agent permissions.
- Automatic classification-driven mutation.
- Remote queue workers or background daemons.

## Repo-Aware Phase Baseline

- Existing top-level mutation commands already execute remote-first writes and
  support dry-run JSON.
- Existing `vivi agent` wrappers only force dry-run JSON and audit a plan.
- Existing send executes SMTP immediately, while reply creates a local draft.
- There is no durable work queue.

## Stage Graph

1. CLI contract
   - Add effect-oriented command enums.
   - Remove the `agent` parser path.
   - Route `exec` to existing execution code.

2. Queue persistence
   - Store queued items as individual JSON files in creation order.
   - Track status as pending, executed, failed, or dropped.

3. Queue execution
   - `queue run <id>` executes one or more queued items.
   - `queue run --all` executes pending items in FIFO order.
   - Running a queued item reuses the same execution paths as `exec`.

4. Docs and tests
   - Update README and issue notes.
   - Update parser tests and add queue unit tests.
   - Verify help output for the new surfaces.

## Checkpoint Target

A caller can choose the effect directly: execute now with `exec`, queue with
`enqueue`, inspect with `queue show`, and execute deferred work with
`queue run`.

## Gate Plan

- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo run -- --help`
- `cargo run -- exec --help`
- `cargo run -- enqueue --help`
- `cargo run -- queue --help`

## Delivered Outputs

- Removed the active `vivi agent ...` parser and deleted the old agent runner,
  agent command enum, agent audit helpers, and agent config defaults.
- Removed top-level external-write commands from the public parser surface.
- Added `vivi exec archive|delete|move|flag|send` for immediate writes.
- Added `vivi enqueue archive|delete|move|flag|send|reply` for durable queued
  work.
- Added `vivi queue list|show|drop|run`.
- Queue entries are persisted as private JSON files under
  `.vivarium/queue/<id>.json` in the selected account mail root.
- `vivi queue run <id>...` executes named pending items.
- `vivi queue run --all` executes all pending items in FIFO order.

## Verification

- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo run -- --help`
- `cargo run -- exec --help`
- `cargo run -- enqueue --help`
- `cargo run -- queue --help`

## Poker Face Check

Completion score: 90%.

The requested clean-break command surface is in place and validated. The main
remaining gap is that queued `reply` currently records only a plain-text body
and does not expose HTML body controls; this matches the narrow phase but is a
candidate for a later mail-composition ergonomics pass.

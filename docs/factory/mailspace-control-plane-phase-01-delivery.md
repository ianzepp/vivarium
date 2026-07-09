# Mailspace Control Plane Phase 01: Dump Safety And Work List JSON

## Interpreted Phase Problem

Project-local mailspace work commands currently push agents toward full dumps
for board intake. `task dump` and `need dump` default to `--status all`, list
output is only handle/from/subject text, and there is no machine-readable work
list for tasks, needs, or wants. Phase 01 should make the smallest useful
control-plane checkpoint: open work is easy to list, done archaeology is
explicit, and large human dumps do not silently flood stdout.

## Normalized Phase Spec

### Goal

Make `task` / `need` / `want` list and dump safer for agent loops without adding
new board, brief, status, body-file, or want-lifecycle commands yet.

### Functional Requirements

- `task dump` and `need dump` default to `--status open`.
- `--status all` and `--status done` remain available for explicit archaeology.
- Human dump output refuses or truncates overly large stdout dumps with
  remediation guidance. JSON and `--output` remain the supported full-export
  paths.
- `task list`, `need list`, and `want list` support `--json`.
- Work list output includes richer, stable fields: handle, kind, status, role,
  date, from, to, subject, and last event when available.
- Human list output gains useful columns while remaining compact.
- README examples stop teaching full `--status all` dumps as the default board
  intake.

### Constraints

- Project-local only; no IMAP, Proton, account-store, or sync changes.
- No gate/stage-license vocabulary or commands.
- Do not add a parallel coordination database.
- Keep handles stable and folder roles unchanged.
- Keep production errors in `VivariumError`.
- Keep touched source files under hygiene limits.

### Out Of Scope

- `vivi board` / `brief`.
- `mailspace status actionable_open`.
- `--body-file` / stdin body intake.
- Want close/drop/archive and `want list --status`.
- Release publication.

## Repo-Aware Baseline

- Goal source: `docs/mailspace-agent-control-plane-goal.md`.
- CLI structs live in `src/cli/mailspace_command.rs` and
  `src/cli/mailspace_command/work_command.rs`.
- Local work dispatch lives in `src/local_work_command.rs`; task dispatch also
  has a small arm in `src/local_mailspace_command.rs`.
- `TaskDumpCommand.status` currently defaults to `all`.
- `Mailspace::list_kind` returns `StoredMessageView` values filtered by role,
  account, and effective kind.
- Dump records already include status, date, body, and event history.
- Dump rendering in `src/local_mailspace_dump.rs` currently prints every body
  and event to stdout with no size guard.
- Integration coverage for local mailspace CLI behavior lives in
  `tests/local_mailspace_cli.rs`.
- README currently documents full board-review dumps with `--status all`.

## Stage Graph

1. Dump defaults and guard
   - Change `TaskDumpCommand.status` default from `all` to `open`.
   - Add a human stdout dump guard/truncation policy with clear guidance to use
     `--status open`, `--status all`, `--since`, `--json`, or `--output`.
   - Keep JSON and file output untruncated.

2. Work list read model
   - Add a list item projection over `StoredMessageView` plus mailspace events.
   - Include last event fields when events exist.
   - Preserve existing kind filtering for tasks, needs, and wants.

3. CLI output
   - Add `--json` to task/need/want list commands.
   - Render compact richer human rows and stable JSON arrays.
   - Keep default task/need list status as `open`.

4. Tests and docs
   - Add integration tests for dump default open behavior.
   - Add integration tests for work list JSON shape and last-event fields.
   - Update README examples toward list-first intake and explicit audit dumps.

## Implementation Work

- Update `TaskDumpCommand` in `src/cli/mailspace_command/work_command.rs`.
- Add list `json: bool` fields to `TaskCommand::List`, `NeedCommand::List`,
  and `WantCommand::List`.
- Add work-list projection and rendering helpers in `src/local_work_command.rs`
  or a small adjacent module if hygiene requires extraction.
- Update task list dispatch in `src/local_mailspace_command.rs` to call the
  shared list renderer with the requested JSON mode.
- Update `src/local_mailspace_dump.rs` to apply the stdout-only human dump
  safety policy.
- Update `tests/local_mailspace_cli.rs` for CLI regressions.
- Update README mailspace examples.

## Checkpoints And Gates

### Checkpoint Target

Agents can inspect open tasks, needs, and wants using list commands, including
JSON output, and ordinary dump commands no longer print done archaeology by
default.

### Batching / Split Decision

Execute as one batch. Dump defaults, dump safety, list JSON, richer columns,
tests, and README examples all share the same local mailspace CLI surface and
one validation path. Splitting would create process-heavy micro-phases.

### Gate Plan

- Correctness pass checks that task/need dump defaults changed only for work
  dump commands, not mail/want dump semantics.
- Correctness pass checks that JSON/list projections do not read or mutate
  remote mail.
- Review confirms no gate/stage-license API was introduced.
- Commit only Phase 01 delivery spec plus implementation/docs/tests.

### Release Decision

This phase changes user-visible CLI defaults and output. Do not publish, but
record that a later release-prep phase should include a minor version bump and
release note once Phase 1-2 are complete.

## Validation

- `cargo fmt --check`
- `cargo test --test hygiene`
- `cargo test --test local_mailspace_cli`
- `cargo test`

## Companion Skill Plan

- Factory loop supervises implementation and checkpointing.
- Use cleanliness/polish after implementation because this phase touches shared
  CLI dispatch and test code.

## Open Questions

- Exact dump guard threshold is not specified by the goal. Factory may choose a
  conservative byte threshold and document the remediation message in tests.

## Phase Checkpoint

### Delivered Outputs

- `task dump` and `need dump` now default to `--status open`.
- `--status all` and `--status done` remain accepted for explicit work history.
- Human stdout dumps refuse large result sets and direct callers to narrow the
  query or export with `--json` / `--output`.
- `task list`, `need list`, and `want list` accept `--json`.
- Work list JSON includes handle, kind, status, role, date, from, to, subject,
  and last event details when present.
- Human work list output includes status, date, and last-event context while
  staying compact.
- README now teaches list-first intake and explicit audit dumps.

### Correctness Pass

- Confirmed the dump default change is scoped to `TaskDumpCommand`, which is
  used by `task dump` and `need dump`; `mail dump` and `want dump` keep their
  existing folder semantics.
- Confirmed list JSON uses only project-local `Mailspace` storage reads and
  event reads; no IMAP, Proton, sync, or account-store paths are touched.
- Confirmed no gate/stage-license command or product API was introduced.

### Verification Run

- `cargo test --test local_mailspace_cli`
- `cargo test --test cli`
- `cargo fmt --check`
- `cargo test --test hygiene`
- `cargo test`

All verification passed.

### Review And Bonsai Discovery

- Reviewed changed CLI structs, work dispatch, list projection, dump rendering,
  README examples, and integration tests.
- No phase-blocking review or bonsai findings.
- No deferred findings beyond the already-planned later goal phases.

### Cleanliness Pass

- Added `src/local_work_list.rs` instead of growing `src/local_work_command.rs`
  past the hygiene ceiling.
- Confirmed touched production files remain below the repo 400-line file limit
  and 60-line function limit.
- No additional behavior-preserving reshaping was needed.

### Housekeeping Pass

- Ran formatting and hygiene checks.
- Scanned touched production files for debug/panic/test-only residue.
- No generated files, lockfiles, caches, or dependency metadata changed.

### Polish Loop

Inspected phase-modified primary source files:

- `src/cli/mailspace_command.rs`
- `src/cli/mailspace_command/work_command.rs`
- `src/local_mailspace_command.rs`
- `src/local_mailspace_dump.rs`
- `src/local_work_command.rs`
- `src/local_work_list.rs`
- `src/main.rs`

No polish-specific code changes or per-file polish commits were needed after
the final verification pass.

### Gate Result

PASS. Phase 01 meets the checkpoint target and is ready to commit.

### Release / Version Decision

Defer release. This phase changes CLI behavior and docs, but the goal calls for
a release checkpoint after Phase 1-2 land together because Phase 2 adds the
board/status control-plane surface.

# Mailspace Control Plane Phase 02: Board And Actionable Status

## Interpreted Phase Problem

Phase 01 made work lists and dumps safer, but agents still need a single
project-local command that answers "what should I do?" without chaining
status, list, and dump commands. Status also still reports unread mail beside
work counts without naming the actionable bag as the primary loop signal.

## Normalized Phase Spec

### Goal

Add a local mailspace board view and actionable status counts over the existing
mailspace store, without adding a new coordination database or gate subsystem.

### Functional Requirements

- Add canonical `vivi board`.
- Board supports `--for <identity>`, `--project <path>`, `--json`, and a wants
  cap (`--wants <N>`).
- Board summarizes open tasks, open needs, and capped wants for one identity or
  all identities.
- Board text output is compact and list-first; board JSON is stable enough for
  agents to parse.
- `mailspace status` text and JSON expose `actionable_open` as tasks + needs,
  separately from unread mail and wants.
- README documents board-first intake.

### Constraints

- Project-local only; no IMAP, Proton, sync, account-store, or network effects.
- Board is a read model over existing mailspace messages and events.
- Open tasks + open needs are actionable work. Wants and unread mail are
  secondary.
- No `gate` command, stage license, or GO/NO-GO product API.
- Keep handles stable and folder roles unchanged.
- Keep touched source files under hygiene limits.

### Out Of Scope

- `brief`, `--since`, and watermark files.
- Body-file/stdin send ergonomics.
- Want close/drop/archive and want status filters.
- Release publication.

## Repo-Aware Baseline

- `src/cli.rs` owns top-level command shape.
- Project-local commands are intercepted before normal account loading in
  `src/local_mailspace_command.rs`.
- `MailspaceStatus` in `src/mailspace.rs` already has per-identity task, need,
  want, unread, and done counts.
- `Mailspace::list_kind` can return open task/need/want messages for an
  identity.
- `src/local_work_list.rs` established the Phase 01 list projection pattern.
- Integration coverage for temp project mailspaces lives in
  `tests/local_mailspace_cli.rs`; parser coverage lives in `tests/cli.rs`.

## Stage Graph

1. CLI and dispatch
   - Add top-level `vivi board`.
   - Route it through local mailspace dispatch before account config loading.

2. Status actionable counts
   - Add `actionable_open` to per-identity and total status structs.
   - Update human status rendering to show actionable work distinctly.
   - Preserve existing counts.

3. Board read model and rendering
   - Build identity board rows from open tasks, needs, and wants.
   - Apply wants cap per identity and expose truncation in JSON/text.
   - Render compact human output and JSON arrays/objects.

4. Tests and docs
   - Add parser tests for `vivi board`.
   - Add integration tests for board text/JSON and status actionable counts.
   - Update README to document status -> board/list -> show.

## Implementation Work

- Update `src/cli.rs` with a `Board` command.
- Add a local board command module or small local mailspace helper.
- Update `run_mailspace_command` to intercept `Command::Board`.
- Update `src/mailspace.rs` status structs and renderer.
- Update `tests/cli.rs` and `tests/local_mailspace_cli.rs`.
- Update README project mailspace examples.

## Checkpoints And Gates

### Checkpoint Target

Agents can run one board command to inspect actionable open work, and status
JSON has a first-class actionable-open count.

### Batching / Split Decision

Execute as one batch. Board output and actionable status counts share the same
mailspace read model and are both needed for the Phase 1-2 release checkpoint.

### Gate Plan

- Correctness pass checks board/status are read-only and project-local.
- Review confirms wants/unread mail remain secondary signals.
- Review confirms no gate/stage-license API was introduced.
- Commit only Phase 02 delivery spec plus implementation/docs/tests.

### Release Decision

After this phase lands, prepare a separate release/version phase or explicit
release note because Phase 1-2 together change CLI defaults and add the board
control-plane surface. Do not publish externally without operator approval.

## Validation

- `cargo fmt --check`
- `cargo test --test hygiene`
- `cargo test --test cli`
- `cargo test --test local_mailspace_cli`
- `cargo test`

## Companion Skill Plan

- Factory loop supervises implementation and checkpointing.
- Use cleanliness/polish after implementation because this phase touches CLI
  dispatch, status structures, and local board rendering.

## Open Questions

- None blocking. Use top-level `vivi board` as the canonical command per the
  goal recommendation.

## Phase Checkpoint

### Delivered Outputs

- Added canonical top-level `vivi board`.
- Board supports `--for`, `--project`, `--json`, and `--wants`.
- Board summarizes open tasks, open needs, and capped wants for one identity or
  the project roster.
- Board JSON exposes per-identity tasks, needs, wants, wants-hidden counts, and
  actionable totals.
- `mailspace status` text and JSON now include `actionable_open` as tasks +
  needs, separate from unread mail and wants.
- README now documents status/board-first intake.

### Correctness Pass

- Confirmed `Command::Board` is intercepted by local mailspace dispatch before
  normal account config loading.
- Confirmed board/status use only project-local `Mailspace` reads and event
  reads; no IMAP, Proton, sync, or account-store paths are touched.
- Confirmed board totals treat wants as secondary and actionable work as tasks
  plus needs only.
- Confirmed no gate/stage-license command or product API was introduced.

### Verification Run

- `cargo test --test cli`
- `cargo test --test local_mailspace_cli`
- `cargo fmt --check`
- `cargo test --test hygiene`
- `cargo test`

All final verification passed.

### Review And Bonsai Discovery

- Reviewed new board CLI args, local board read model, status count changes,
  README examples, and integration tests.
- No phase-blocking review or bonsai findings.
- No deferred findings beyond the already-planned later goal phases.

### Cleanliness Pass

- Moved board CLI args into `src/cli/board_command.rs` after hygiene exposed
  `src/cli.rs` crossing the 400-line limit.
- Kept board implementation in `src/local_board_command.rs` to avoid mixing
  board rendering into task/need/want dispatch.
- Confirmed touched production files remain below file and function hygiene
  limits.

### Housekeeping Pass

- Ran formatting, hygiene, targeted CLI/local tests, and full tests.
- Scanned touched production files for debug/panic/test-only residue.
- No generated files, lockfiles, caches, or dependency metadata changed.

### Polish Loop

Inspected phase-modified primary source files:

- `src/cli.rs`
- `src/cli/board_command.rs`
- `src/local_board_command.rs`
- `src/local_mailspace_command.rs`
- `src/mailspace.rs`
- `src/main.rs`

No polish-specific code changes or per-file polish commits were needed after
the final verification pass.

### Gate Result

PASS. Phase 02 meets the checkpoint target and is ready to commit.

### Release / Version Decision

Release-prep is now due: Phase 1-2 together changed work-dump defaults, added
machine-readable work lists, and added `vivi board` plus actionable status
counts. External publication still requires operator approval.

# Mailspace Control Plane Phase 05: Body Input And Want Lifecycle

## Interpreted Phase Problem

Agents can now list and board open work, but long residual evidence is still
awkward to send and wants still have only a promotion path. Obsolete wants need
a first-class close/drop path, and closed wants need to remain inspectable
through `want list --status`.

## Normalized Phase Spec

### Goal

Add explicit local send body-file/stdin ergonomics and complete the basic want
lifecycle without adding stage gates or a second coordination store.

### Functional Requirements

- Local `mail send`, `task send`, `need send`, and `want send` accept
  `--body-file <path>`.
- Local send commands accept `--body -` to read stdin.
- Existing `--body @path` behavior remains documented.
- `want done <handle> --for <identity> [--note ...]` closes obsolete wants by
  moving them to `Done` with stable handles.
- `want drop` is an alias for the same close path.
- `want list --status open|done|all` exposes open and closed wants, including
  JSON output.

### Constraints

- Project-local only; no external mail sends or remote account changes.
- Handles remain stable across folder moves.
- Closed wants remain kind `want`, not promoted to needs.
- No gate/stage-license command or API.

### Out Of Scope

- Review request templates.
- Bulk want archive.
- Watch/stream mode.

## Repo-Aware Baseline

- `LocalSendCommand` currently requires `--body` and supports `@path` through
  `read_body_arg`.
- Local send dispatch is in `src/local_mailspace_command.rs` and
  `src/local_work_command.rs`.
- `WantCommand` currently supports `send`, `list`, `show`, `dump`, and
  `promote`.
- `effective_kind` can preserve done wants through the existing `X-Vivi-Kind`
  message header.
- `src/local_work_list.rs` can be extended to print list output across multiple
  roles for `--status all`.

## Stage Graph

1. Body input
   - Add body input arg group to `LocalSendCommand`.
   - Add `--body-file` and stdin `--body -` handling.
   - Reuse existing `@path` behavior.

2. Want lifecycle
   - Add `want done` and `want drop`.
   - Add `want list --status open|done|all`.
   - Ensure closed wants stay listable/auditable as wants.

3. Tests and docs
   - Add parser tests for body-file and want status/close commands.
   - Add integration tests for body-file/stdin and closed want list JSON.
   - Update README examples.

## Implementation Work

- Update `src/cli/mailspace_command.rs`.
- Update `src/cli/mailspace_command/work_command.rs`.
- Update body readers in `src/mailspace.rs`.
- Update local send dispatch in `src/local_mailspace_command.rs` and
  `src/local_work_command.rs`.
- Extend `src/local_work_list.rs` for multi-role want status output.
- Update `tests/cli.rs`, `tests/local_mailspace_cli.rs`, and README.

## Checkpoints And Gates

### Checkpoint Target

Agents can send long local bodies from files/stdin and can close, list, and
audit obsolete wants without promoting them to needs.

### Batching / Split Decision

Execute as one batch. Body input and want lifecycle both touch the same local
send/work command surface and share the local mailspace integration suite.

### Gate Plan

- Correctness pass checks local body input is project-local and does not affect
  remote compose/send surfaces.
- Correctness pass checks closed wants remain kind `want`.
- Review confirms no gate/stage-license API was introduced.

### Release Decision

Defer publication. Local 4.5.0 release metadata exists; external publication
requires operator approval.

## Validation

- `cargo fmt --check`
- `cargo test --test cli`
- `cargo test --test local_mailspace_cli`
- `cargo test --test hygiene`
- `cargo test`

## Companion Skill Plan

- Factory supervises implementation and checkpointing.
- Use cleanliness/polish over changed command/list surfaces.

## Open Questions

- None blocking. Use `want done` as canonical and `want drop` as an alias per
  the goal recommendation.

## Phase Checkpoint

### Delivered Outputs

- Added `--body-file <path>` to local `mail`, `task`, `need`, and `want` send
  commands.
- Added stdin body intake through `--body -`.
- Preserved existing `--body @path` behavior.
- Added `want done` and `want drop`.
- Added `want list --status open|done|all` with JSON support.
- Closed wants move to `Done` with stable handles and remain kind `want`.
- README now documents body-file/stdin and closed-want status workflows.

### Correctness Pass

- Confirmed body-file/stdin changes are scoped to `LocalSendCommand` and local
  mailspace send paths; remote compose/send surfaces are unchanged.
- Confirmed `want done` and `want drop` move to the existing `done` role and
  record distinct event commands.
- Confirmed `want list --status done` filters done messages by effective kind,
  so closed wants do not mix with completed tasks or needs.
- Confirmed no gate/stage-license command or product API was introduced.

### Verification Run

- `cargo test --test cli`
- `cargo test --test local_mailspace_cli`
- `cargo fmt --check`
- `cargo test --test hygiene`
- `cargo test`

All verification passed.

### Review And Bonsai Discovery

- Reviewed local body input parsing, stdin/file reads, want close/drop dispatch,
  multi-role work-list rendering, README examples, and integration tests.
- No phase-blocking review or bonsai findings.
- No deferred findings beyond the already-planned future review-template and
  bulk-want archive ideas.

### Cleanliness Pass

- Split body input readers into `src/mailspace/body.rs` after hygiene showed
  `src/mailspace.rs` crossing the file limit.
- Extracted mail send and want close/list helpers to keep dispatch functions
  below the function limit.
- Confirmed touched production files remain below file and function hygiene
  limits.

### Housekeeping Pass

- Ran formatting, hygiene, targeted CLI/local tests, and full tests.
- Scanned touched production files for debug/panic/test-only residue.
- No generated files, lockfiles, caches, or dependency metadata changed.

### Polish Loop

Inspected phase-modified primary source files:

- `src/cli.rs`
- `src/cli/mailspace_command.rs`
- `src/cli/mailspace_command/work_command.rs`
- `src/local_mailspace_command.rs`
- `src/local_work_command.rs`
- `src/local_work_list.rs`
- `src/mailspace.rs`
- `src/mailspace/body.rs`

No polish-specific code changes or per-file polish commits were needed after
the final verification pass.

### Gate Result

PASS. Phase 05 meets the checkpoint target and is ready to commit.

### Release / Version Decision

Defer publication. Local 4.5.0 metadata exists, but this phase adds more
release-note-worthy behavior; update release notes before any operator-approved
publication.

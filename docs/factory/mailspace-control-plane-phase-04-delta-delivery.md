# Mailspace Control Plane Phase 04: Board Delta And Watermarks

## Interpreted Phase Problem

`vivi board` now answers what is open, but agents still need to ask what changed
since a known point without dumping done history or carrying ad hoc baseline
files outside Vivi conventions.

## Normalized Phase Spec

### Goal

Add delta filtering to the board read model with explicit, agent-owned
watermark files.

### Functional Requirements

- `vivi board --since <time>` filters board items to open work created or moved
  since the bound.
- Accepted time forms match dump filters: RFC3339, `YYYY-MM-DD`, `Nh`, `Nd`,
  and `Nw`.
- `vivi board --watermark-file <path>` can read the since bound from a file
  when `--since` is not passed.
- `--write-watermark` updates the watermark file after a successful board run.
- JSON and text output both reflect the filtered board.

### Constraints

- Watermark files are caller-owned paths; Vivi does not require them.
- Board remains project-local and read-only except for explicit watermark
  write-back.
- No second store, watch mode, or stage/gate API.

### Out Of Scope

- Separate `brief` command.
- Watch/stream mode.
- Want lifecycle changes.
- Body-file/stdin send ergonomics.

## Repo-Aware Baseline

- `src/local_board_command.rs` owns board construction and rendering.
- `src/cli/board_command.rs` owns board flags.
- Dump time parsing already supports the desired forms in `src/mailspace/dump.rs`
  but is not yet reusable outside the mailspace module.
- Mailspace events expose `occurred_at`, allowing moved/open items to count as
  changed after their original message date.

## Stage Graph

1. Shared time parsing
   - Expose the existing mailspace dump time parser for board use.

2. CLI and watermark input
   - Add `--since`, `--watermark-file`, and `--write-watermark` to board.
   - Resolve `--since` first, then watermark-file content when present.

3. Board filtering and write-back
   - Include open items whose message date or event timestamps are on/after the
     since bound.
   - Write a fresh RFC3339 watermark after successful rendering only when
     `--write-watermark` is passed.

4. Tests and docs
   - Add parser and integration tests for `--since` and watermark behavior.
   - Update README board examples.

## Implementation Work

- Update `src/cli/board_command.rs`.
- Update `src/local_board_command.rs`.
- Expose reusable time parsing from `src/mailspace/dump.rs` through
  `src/mailspace.rs`.
- Update `tests/cli.rs` and `tests/local_mailspace_cli.rs`.
- Update README examples.

## Checkpoints And Gates

### Checkpoint Target

Agents can run `vivi board --since ...` or use a watermark file to inspect only
recent open-work changes.

### Batching / Split Decision

Execute as one batch. Since filtering and watermark read/write are coupled at
the board command boundary and share one validation path.

### Gate Plan

- Correctness pass checks `--since` precedence over watermark file content.
- Correctness pass checks watermark write-back happens only on explicit
  `--write-watermark`.
- Review confirms no hidden coordination database or stage gate was introduced.

### Release Decision

Defer publication. Version metadata is already prepared locally for 4.5.0;
external publication remains operator-gated.

## Validation

- `cargo fmt --check`
- `cargo test --test cli`
- `cargo test --test local_mailspace_cli`
- `cargo test --test hygiene`
- `cargo test`

## Companion Skill Plan

- Factory supervises implementation and checkpointing.
- Use cleanliness/polish over changed board/time parsing surfaces.

## Open Questions

- None blocking. Use the existing dump time grammar for board since bounds.

## Phase Checkpoint

### Delivered Outputs

- Added `vivi board --since <time>`.
- Added `vivi board --watermark-file <path>`.
- Added `vivi board --write-watermark` with explicit `--watermark-file`
  requirement.
- Board since filtering includes open items whose message date or mailspace
  event timestamps are on/after the bound.
- Board uses the same time grammar as dump filters by re-exporting the existing
  mailspace parser.
- README now shows board delta and watermark examples.

### Correctness Pass

- Confirmed `--since` takes precedence over watermark-file contents.
- Confirmed missing or empty watermark files mean no since filter.
- Confirmed watermark write-back happens only with explicit
  `--write-watermark`.
- Confirmed wants-hidden counts are computed after since filtering, not from
  all historical open wants.
- Confirmed no second store, watch mode, or gate/stage API was introduced.

### Verification Run

- `cargo test --test cli`
- `cargo test --test local_mailspace_cli`
- `cargo fmt --check`
- `cargo test --test hygiene`
- `cargo test`

All verification passed.

### Review And Bonsai Discovery

- Reviewed board since resolution, watermark read/write behavior, cap counting,
  time parser exposure, README examples, and integration tests.
- No phase-blocking review or bonsai findings.
- No deferred findings beyond the already-planned later goal phases.

### Cleanliness Pass

- Kept watermark and since behavior grouped inside `src/local_board_command.rs`
  with `BoardCommand` carrying CLI options.
- Confirmed changed production files remain below file and function hygiene
  limits.
- No additional behavior-preserving reshaping was needed.

### Housekeeping Pass

- Ran formatting, hygiene, targeted CLI/local tests, and full tests.
- Scanned touched production files for debug/panic/test-only residue.
- No generated files, lockfiles, caches, or dependency metadata changed.

### Polish Loop

Inspected phase-modified primary source files:

- `src/cli/board_command.rs`
- `src/local_board_command.rs`
- `src/local_mailspace_command.rs`
- `src/mailspace.rs`
- `src/mailspace/dump.rs`

No polish-specific code changes or per-file polish commits were needed after
the final verification pass.

### Gate Result

PASS. Phase 04 meets the checkpoint target and is ready to commit.

### Release / Version Decision

Defer publication. Local 4.5.0 metadata is already prepared; external
publication still requires operator approval.

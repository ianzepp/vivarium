# Mailspace Control Plane Phase 03: Release Prep

## Interpreted Phase Problem

Phase 1-2 changed user-visible CLI behavior and added the new local board
surface. The goal requires a release checkpoint after those phases land, but
external publication still needs explicit operator approval.

## Normalized Phase Spec

### Goal

Prepare local release metadata for Vivarium 4.5.0 without publishing crates,
Homebrew artifacts, tags, or remote releases.

### Functional Requirements

- Bump package metadata from 4.4.0 to 4.5.0.
- Keep `Cargo.lock` package metadata in sync.
- Add release notes calling out the dump default break, list JSON, `vivi board`,
  and actionable status counts.
- Record that external publication remains out of scope.

### Constraints

- No external publishing, tagging, pushing, or Homebrew updates.
- No unrelated feature work.
- No dependency changes.

### Out Of Scope

- Phase 3 brief/since/watermark implementation.
- Crates.io publication.
- Homebrew formula publication.
- GitHub release/tag creation.

## Repo-Aware Baseline

- Current package version is 4.4.0 in `Cargo.toml` and the root package entry in
  `Cargo.lock`.
- Existing release note pattern is `docs/release-v4.0.0.md`.
- Existing release smoke checklist is `docs/release-smoke-checks.md`.

## Stage Graph

1. Version metadata
   - Update `Cargo.toml`.
   - Update `Cargo.lock` root package entry.

2. Release notes
   - Add `docs/release-v4.5.0.md`.
   - Include release checks and publication hold.

3. Validation
   - Run formatting/checks appropriate for metadata and docs.
   - Run targeted cargo validation to prove package metadata is coherent.

## Checkpoints And Gates

### Checkpoint Target

The repo has local 4.5.0 release metadata ready for operator review, with no
external publication performed.

### Gate Plan

- Correctness pass confirms no code behavior changed in release prep.
- Review confirms release notes mention the CLI default break.
- Commit only version metadata, release notes, and this delivery spec.

### Release Decision

Release-prep only. Stop before publication.

## Validation

- `cargo fmt --check`
- `cargo test --test cli`
- `cargo test --test local_mailspace_cli`

## Companion Skill Plan

- Factory supervises the release-prep checkpoint.
- Cleanliness/polish should be minimal because this phase is metadata/docs only.

## Open Questions

- None blocking.

## Phase Checkpoint

### Delivered Outputs

- Bumped package metadata to 4.5.0 in `Cargo.toml`.
- Updated the root package entry in `Cargo.lock` to 4.5.0.
- Added `docs/release-v4.5.0.md` with highlights, the dump-default breaking
  change, release checks, and an explicit publication hold.

### Correctness Pass

- Confirmed this phase changed release metadata and docs only.
- Confirmed no dependency versions or lockfile dependency graph entries changed.
- Confirmed release notes call out the `task dump` / `need dump` default change.
- Confirmed no external publication, tagging, pushing, or Homebrew work was
  performed.

### Verification Run

- `cargo fmt --check`
- `cargo test --test cli`
- `cargo test --test local_mailspace_cli`

All verification passed.

### Review And Bonsai Discovery

- Reviewed version metadata and release note wording.
- No phase-blocking review or bonsai findings.
- No deferred findings.

### Cleanliness / Housekeeping / Polish

- No implementation source files changed in this phase.
- Formatting check passed.
- No polish-specific edits or commits were needed.

### Gate Result

PASS. Local 4.5.0 release prep is complete.

### Release / Version Decision

Stop before publication. Crates, Homebrew artifacts, tags, and remote releases
still require operator approval.

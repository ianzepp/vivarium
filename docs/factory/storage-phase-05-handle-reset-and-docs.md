# Storage Phase 05: Handle, Reset, And Docs Cleanup

## Interpreted Phase Problem

After Phase 04, the remaining rewrite gaps are mostly identity and operational
surface gaps rather than core storage behavior:

- sync still injects legacy `local-folder-uid` message IDs on download
- ordinary CLI output still treats internal message IDs as the displayed handle
- prefix-resolution semantics from the rewrite are not implemented
- README and operational docs still describe Maildir as the source of truth
- reset/bootstrap expectations are not yet documented as the normal clean-break
  path

The runtime is now largely storage-native, but the user-facing identity model
and docs still reflect the old system.

## Normalized Phase Spec

### Goal

Finish the clean break by switching fresh syncs to opaque message IDs,
introducing short-handle presentation and resolution, and updating the docs to
describe blob-backed storage plus reset-first bootstrap.

### Inputs

- `docs/hash-addressed-storage-rewrite.md`
- `docs/hash-addressed-storage-factory-plan.md`
- completed storage phases 00 through 04
- current sync ingress, CLI output, lookup, and README surfaces

### Expected Outputs

- sync stops seeding `inbox-2050`-style message IDs
- list/show/thread/search/mutation-facing surfaces display short handles derived
  from `message_id`
- handle/content prefix resolution errors on ambiguity
- README and operational docs stop describing Maildir as the source of truth
- clean-break reset flow is explicit

### Out Of Scope

- migrating old caches in place
- cross-account embedding dedupe
- large doc rewrites outside the storage/runtime surface

## Repo-Aware Baseline

Live code after Phase 04 already has:

- blob-backed sync/read/search/thread/embedding/mutation behavior
- `storage.sqlite` as the only core DB
- remaining compatibility mostly confined to IDs, lookups, and docs

That makes the final phase a user-surface and bootstrap cleanup rather than a
new storage-engine phase.

## Checkpoint Target

Fresh Vivi state uses opaque message rows plus short-handle UX, and the docs
describe reset-backed blob storage rather than Maildir authority.

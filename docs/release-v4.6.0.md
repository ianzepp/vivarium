# Vivarium 4.6.0

Vivarium 4.6.0 adds project-local mailspace liveness and conversation
lineage.

## Highlights

- Adds `vivi mailspace watch` plus `mail|task|need|want watch` aliases for
  filtered local event alarms, JSON/text output, timeouts, once scans, and
  caller-owned event-id cursors.
- Adds `vivi mail reply`, `--reply-to` on local sends, and
  `vivi mail thread` for kind-agnostic captured reply lineage.
- Adds thread context to task, need, and want show commands and exposes
  `parent_content_id` and `link_source` in dump JSON.
- Turns lifecycle `--note` values into atomic captured reply messages while
  retaining the event-ledger audit note.
- Adds default-off, read-only historical inference for reply subjects and
  handle citations; inferred links are marked and never override captured
  links.

## Migration

Existing `.vivi/mail.sqlite` databases add the `mailspace_links` table lazily
through the normal schema initializer. Existing messages remain valid and are
not rewritten; only new captured replies and lifecycle notes receive
authoritative links.

## Publication Hold

This file prepares release metadata only. Do not publish crates, Homebrew
artifacts, tags, or remote releases without operator approval.

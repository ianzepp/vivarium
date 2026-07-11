# Vivarium 4.6.0

Vivarium 4.6.0 adds project-local mailspace liveness and conversation
lineage on top of the 4.5 control plane.

## Highlights

- Adds `vivi mailspace watch` for filtered local event alarms over the
  project `.vivi/mail.sqlite` event ledger. This is not IMAP/`sync-events`
  watch.
- Adds kind aliases `vivi mail watch`, `vivi task watch`, `vivi need watch`,
  and `vivi want watch`. Aliases force the kind filter; the canonical
  command defaults kinds to `mail,task,need` (want is opt-in via
  `--kinds`).
- Watch supports `--for`, `--kinds`, `--events`, `--statuses`,
  `--match-from`, `--match-subject-prefix`, `--handle`, `--until-count`,
  `--timeout`, `--once`, `--since`, `--poll-interval`, JSON/text output,
  and caller-owned event-id cursors via `--cursor-file` /
  `--watermark-file` with `--write-cursor` / `--write-watermark`.
- Adds `vivi mail reply <handle>` and `--reply-to <handle>` on local
  `mail` / `task` / `need` / `want` sends for kind-agnostic captured reply
  lineage.
- Adds `vivi mail thread <handle>` with `--json`, opt-in `--infer`,
  `--limit` (default 50), and `--max-depth` (default 50).
- Adds thread context to `task show`, `need show`, `want show`, and
  `mail show`.
- Exposes `parent_content_id` and `link_source` in dump JSON.
- Turns lifecycle `--note` values into atomic captured reply messages while
  retaining the event-ledger audit note.
- Adds default-off, read-only historical inference for reply subjects and
  handle citations; inferred links are marked and never override captured
  links.

## Migration

Existing `.vivi/mail.sqlite` databases add the `mailspace_links` table lazily
through the normal schema initializer. Existing messages remain valid and are
not rewritten; only new captured replies and lifecycle notes receive
authoritative links. Historical inference is a read-time view and does not
persist inferred rows unless a later capture writes them.

## Release Checks

Before publishing the tag, run:

```sh
cargo fmt --check
cargo test --test hygiene
cargo test
cargo build --release
target/release/vivi --version
```

This release does not change provider routing, sync, or send paths, so live
provider smoke checks from `docs/release-smoke-checks.md` are optional rather
than required for 4.6.0.

## Publication

GitHub release assets and the Homebrew tap (`ianzepp/homebrew-tap` formula
`vivarium`) are part of this release contract. Crates.io is not.

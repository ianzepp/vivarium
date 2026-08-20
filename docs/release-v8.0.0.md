# Vivarium 8.0.0

Vivarium 8.0.0 makes **absorb** a seal and adds a **historical archive** for
absorbed mailspace records. Absorb is available for mail, task, need, want, and
memo. After absorb, that row cannot be moved, reopened, prioritized, deleted, or
otherwise changed. A mailspace can point at a dedicated git repo; absorb and
`vivi mailspace archive export` write one Markdown file with TOML frontmatter
per sealed record.

This is a major-version release because absorb changes from inbox-mail
bookkeeping into an irreversible seal across all record kinds, and because
project mailspaces gain storage schema **5** (`absorbed_at` / `absorbed_by`)
plus a new archive config and export surface.

## Highlights

### Absorb seals every record kind

```sh
vivi mail absorb --project <root> --for mind <handle> [--note '...']
vivi task absorb --project <root> --for hand <handle>
vivi need absorb --project <root> --for ceo <handle>
vivi want absorb --project <root> --for mind <handle>
vivi memo absorb --project <root> --for mind <handle>
```

A second absorb of the same handle is a no-op on the database. Replies and
`task from` still create **new** records. `done` remains operational close and
can still reopen until absorb.

### Historical archive git repo

```sh
vivi mailspace archive --project <root> --set /path/to/vivi
vivi mailspace archive --project <root>
vivi mailspace archive export --project <root> [--json]
vivi mailspace archive --project <root> --clear
```

`--set` stores `archive = "..."` in `.vivi/mailspace.toml`. The path must exist
and be a git repository. Relative paths resolve from the project root. `~/...`
expands. Absorb still seals SQLite if no archive is configured.

When an archive is set, each absorb writes:

```text
{archive}/{mailspace-name}/{kind}/{message_id}.md
```

Files are rewritten only when the rendered bytes differ. Identical content is
left untouched. Vivi does not `git add` or `git commit` in the archive repo.

`vivi mailspace archive export` walks every sealed row and backfills missing
files. A second run reports `written 0` / `unchanged N` unless the export
format changed.

## Breaking / migration notes

| Before (7.3.0) | After (8.0.0) |
| --- | --- |
| `vivi mail absorb` only accepted inbox mail | Any mail kind the identity owns |
| Absorb marked mail read; the row stayed mutable | Absorb seals the row; later mutations fail |
| Tasks/needs/wants/memos had no absorb | `task`/`need`/`want`/`memo absorb` |
| `task done` / `reopen` / `want set-priority` / `memo delete` after absorb | Error: `{handle} is absorbed and can no longer be changed` |
| Storage schema **4** | Schema **5**; `absorbed_at` / `absorbed_by` plus event backfill on open |
| Open board/list included absorbed-but-still-open rows | Absorbed open work is omitted from board and open counts |

Existing mailspaces migrate on first open. No dump/restore is required. Records
absorbed under 7.x are backfilled onto the new columns from `absorbed` events.

Agents that treated `mail absorb` as "I read this, I may still move it" must
stop mutating after absorb. To keep reopen available, use `task done` /
`need done` and absorb only when the record should stay frozen.

## Compatibility

- Storage schema becomes **5**. The upgrade adds columns and backfills from
  existing absorb events; IMAP account `storage.sqlite` files get the same
  columns but are unused there.
- `vivi-pty` versions in lockstep (both report **8.0.0**) and continues to
  ship in the same release archives; the Homebrew formula installs both
  binaries.
- IMAP/SMTP and Proton transport paths are unchanged.
- Work graphs, roles, goals, and `vivi trace` are unchanged.

## Installation

```sh
# Homebrew
brew upgrade ianzepp/tap/vivarium

# curl installer
curl -fsSL https://raw.githubusercontent.com/ianzepp/vivarium/main/install.sh | bash

# From source
cargo install --path .
cargo install --path crates/vivi-pty
```

## Release Checks

Before publishing the tag, run:

```sh
cargo fmt --check
cargo test --test hygiene
cargo test
cargo test --features outbox
cargo clippy --all-targets -- -D warnings
cargo build --release
cargo build --release -p vivi-pty
target/release/vivi --version
target/release/vivi-pty --version
```

Live provider smoke (`docs/release-smoke-checks.md`) is optional for this
cut: 8.0.0 does not change IMAP/SMTP or Proton transport paths.

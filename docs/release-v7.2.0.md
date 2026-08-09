# Vivarium 7.2.0

Vivarium 7.2.0 adds a project-local **goal path registry** so a managing Mind
(and other roles) can keep durable pointers to factory and campaign goal files
after session compaction. Goals are not freeform memos: Vivi stores only the
path (and optional label / registrar), and the board always surfaces the list.

This is a minor-version release. Existing task, need, want, memo, graph, and
role flows are unchanged. Opening a mailspace upgrades storage schema **3 → 4**
by creating the `mailspace_goals` table (additive; no rewrite of message data).

## Highlights

### `vivi goal` — register goal document paths

```sh
vivi goal add --project <root> --path docs/factory/foo-goal.md \
  --label foo --for mind --json
vivi goal list --project <root> [--json]
vivi goal show --project <root> <handle|path> [--json]
vivi goal drop --project <root> <handle|path> [--json]
```

| Behavior | Detail |
| --- | --- |
| Owns | handle, project-relative path, optional label, optional `registered_by`, `created_at` |
| Does not own | goal body, progress, acceptance criteria, or any status field |
| Source of truth for content | the file at `path` |
| `add` | path must exist, be a file, and lie under the project root |
| `drop` | hard-unregister from the mailspace; **does not delete the file** |
| Existence | list / board report live `exists` so missing files show as `MISSING` |

Status of a campaign lives in the goal document (or whatever process the team
uses). Vivi does not scan or cache that status—include the path, or drop it.

### Board always shows registered goals

```sh
vivi board --project <root> --for mind --json
vivi board --project <root>
```

JSON includes a top-level `goals` array on every board (empty when none are
registered). Text boards print a `goals:` section when at least one path is
registered. Unlike work graphs (`--graph`), goals are not behind a flag—so a
Mind wake after compaction sees them without remembering optional switches.

Typical Mind wake:

1. `vivi board --project <root> --for mind --json`
2. For each `goals[].path`, read the file
3. Cross-check open tasks / graph frontier against those goals
4. Do not paste goal bodies into memos “for memory”

## Breaking / migration notes

| Area | Note |
| --- | --- |
| Schema | New table `mailspace_goals`; schema version **4**. Auto-created on open. |
| CLI | New top-level `vivi goal` command tree only; no renames of existing commands |
| Board JSON | New `goals` field (always present). Additive for parsers that ignore unknown keys |
| Memo / want | Unchanged; goals intentionally are **not** memo-like freeform storage |

No forced migration of historical data. Existing mailspaces open cleanly;
register paths with `goal add` when a Mind should monitor them.

## Compatibility

- Storage schema moves **3 → 4** (additive goals table only).
- Mail, task, need, want, memo, role, graph, watch, and board (task/need/want)
  behavior is unchanged aside from the extra `goals` board field.
- `vivi-pty` versions in lockstep (both report **7.2.0**) and continues to ship
  in the same release archives; the Homebrew formula installs both binaries.
- IMAP/SMTP and Proton transport paths are unchanged.

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

Live provider smoke (`docs/release-smoke-checks.md`) is optional for this cut:
7.2.0 does not change IMAP/SMTP or Proton transport paths.

# Vivarium 4.5.0

Vivarium 4.5.0 is the project-local mailspace control-plane checkpoint.

## Highlights

- Adds `vivi board` for compact project-local task, need, and want review.
- Adds `--json` to `task list`, `need list`, and `want list`.
- Adds `actionable_open` to `mailspace status` text and JSON, counting open
  tasks plus open needs separately from unread mail and wants.
- Adds richer work-list rows with status, date, and last-event context.
- Adds `--body-file` and `--body -` for project-local mail/task/need/want
  sends.
- Adds `want done`, `want drop`, and `want list --status open|done|all`.
- Adds a human stdout safety guard for large dumps; use `--json` or
  `--output <path>` for full exports.

## Breaking Change

`task dump` and `need dump` now default to `--status open` instead of
`--status all`. Use `--status all` when you intentionally need done history.

## Publication Hold

This file prepares release metadata only. Do not publish crates, Homebrew
artifacts, tags, or remote releases without operator approval.

## Release Checks

Before publishing the tag, run:

```sh
cargo fmt --check
cargo test --test hygiene
cargo test
cargo build --release
```

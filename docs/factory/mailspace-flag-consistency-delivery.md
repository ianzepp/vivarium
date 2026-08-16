# Delivery Spec: Mailspace list/watch/dump flag consistency

Status: **complete**

`--kinds` is only on `mailspace watch`. `want dump` uses work-dump flags.
`task`/`need`/`want` list accept `--from`/`--to` and optional `--for`.
`task`/`need` list accept `--status all`.

Follow-on to `docs/factory/mail-list-from-to-delivery.md`. Scan finding:
sibling verbs share names but not flags, and two commands parse flags they
ignore.

## Interpreted Unit

Make parsed flags match behavior on kind-specific watch and want dump.
Give `task` / `need` / `want` list the same `--from` / `--to` / optional
`--for` shape as `mail list`. Add `task list --status all` and
`need list --status all` to match want list and task dump.

## Normalized Spec

1. `vivi mail|task|need|want watch` must not accept `--kinds`.
   `vivi mailspace watch --kinds` remains the only kind mixer.
2. `vivi want dump` uses `TaskDumpCommand` (`--status open|done|all`,
   `--from`/`--to`/`--participant`, no `--folder` / absorb flags).
3. `vivi task|need|want list` accept `--from` and `--to` header filters.
   `--for` is optional when either is present. At least one of
   `--for`/`--from`/`--to` is required.
4. `task list` and `need list` accept `--status all`.
5. `task list --blocked` / `--blocking` still require `--for`.
6. Existing `--for` list output stays the same when from/to are omitted.

### Non-goals

- Memo list `--from`/`--to`
- Dump-only filters (`--since`, `--body`, `--participant`) on list
- Watch `--from` alias for `--match-from`
- Changing send `--from`/`--to` meaning

## Stage Graph

One Hand. Same CLI family, one checkpoint.

## Validation

- `cargo fmt --check`
- `cargo test --test hygiene`
- parse tests for watch kinds rejection, want dump shape, list from/to,
  list `--status all`
- integration: task list `--from` and `--status all`

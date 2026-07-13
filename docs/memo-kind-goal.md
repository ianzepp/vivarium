# Goal: Vivi Memo Kind

## Summary

Add a fifth work kind — `memo` — to Vivi's project-local mailspace. A memo is
a structured, durable record of a role's own observation or reasoning that
persists across sessions. Unlike tasks, needs, wants, and mail, a memo implies
no obligation: no work to do, no decision to request, no communication to
deliver. It is pure context preservation.

## Problem

Vivi's four existing kinds (task, need, want, mail) all imply an obligation on
someone — work, a decision, future consideration, or inter-role communication.
There is no semantically-aligned place for a role to record an observation it
wants to carry forward. Without a first-class kind, LLMs either:

1. Improvise (random scratchpad files, buried mail bodies)
2. Misuse an existing kind (self-addressed `want`, which carries the wrong
   semantics — "product idea to consider later" vs. "personal context to
   remember")
3. Drop the thought entirely at session end

A `memo` kind with a `save` (not `send`) verb gives LLMs a sanctioned,
typed channel with clear semantics: "record this for my own future context."

## Goals

- Add `memo` as a fifth work kind alongside task, need, want, and mail.
- Implement five subcommands: `save`, `delete`, `list`, `show`, `dump`.
- Use `save` (not `send`) to signal persistence, not communication.
- Require `--for` on all subcommands except `show` (handle is globally unique).
- Require `--for` on `dump` specifically so LLMs don't accidentally dump all
  identities' memos.
- Store memos in a `memos` folder role, separate from the actionable bag.
- Memos do NOT appear in `vivi board` aggregation — they are passive context.
- Memos have no lifecycle: no `done`, no `promote`, no `reopen`. Save and
  delete are the only state transitions.
- Include memo count in `vivi mailspace status` output.

## Non-goals

- Privacy or per-identity access control — the mailspace is shared by design;
  Mind can inspect all memos.
- Board integration — memos are not actionable work.
- Lifecycle operations (promote, reopen) — memos are not proto-work.
- A `send` verb — memos are not communication.
- Event-driven watch for memos — not needed for v1.

## Command Shape

```
vivi memo save  --project "$ROOT" --for head-ceo --subject '...' --body '...'
vivi memo delete --project "$ROOT" --for head-ceo <handle>
vivi memo list  --project "$ROOT" --for head-ceo [--json]
vivi memo show  --project "$ROOT" <handle> [--json]
vivi memo dump  --project "$ROOT" --for head-ceo [--json] [--output PATH]
```

`save` accepts `--body @/path` and `--body-file /path` for long bodies, same
as the other kinds.

## Invariants

1. `save` creates a self-addressed RFC5322 message with `X-Vivi-Kind: memo`
   header, stored in the `memos` folder for the `--for` identity. No sent copy.
2. `delete` moves the memo to `trash` (consistent with existing deletion model).
3. `list` prints one-liner: `handle  date  subject`.
4. `show` reuses existing thread/show display (same as task/need/want show).
5. `dump` reuses existing dump machinery with folder scoped to `memos`.
6. Memos are excluded from `vivi board` and from actionable bag counts.
7. `canonical_local_role` accepts `memo`/`memos` → `"memos"`.
8. `effective_kind` returns `"memo"` for the `memos` role.

## Acceptance Signals

- `vivi memo save --for head-ceo --subject 'test' --body 'body'` creates a memo
  and prints the handle.
- `vivi memo list --for head-ceo` shows the one-liner.
- `vivi memo show <handle>` shows full detail.
- `vivi memo delete --for head-ceo <handle>` removes it.
- `vivi memo dump --for head-ceo` dumps all memos for that identity.
- `vivi mailspace status` shows a `memos` count column.
- `vivi board --for head-ceo` does NOT include memos.
- `cargo test` passes.
- `cargo clippy --all-targets -- -D warnings` passes.

## Ground Truth

- `src/mailspace.rs`: `canonical_local_role` maps role strings; `Mailspace::status`
  computes per-identity counts; `IdentityStatus` struct holds counts.
- `src/mailspace/kind.rs`: `effective_kind` maps folder role → kind string.
- `src/mailspace/delivery.rs`: `send()` composes + ingests messages; `move_item()`
  moves between folder roles.
- `src/mailspace/dump.rs`: `dump_mail()` / `dump_tasks()` scan by folder role.
- `src/cli.rs`: `Command` enum; mailspace commands dispatched early in
  `run_mailspace_command`.
- `src/cli/mailspace_command.rs`: command definitions + exports.
- `src/cli/mailspace_command/work_command.rs`: `NeedCommand`, `WantCommand`,
  `TaskStatus`, dump command structs.
- `src/local_mailspace_command.rs`: `run_mailspace_command` routes early; handlers.
- `src/local_work_command.rs`: `handle_need_command`, `handle_want_command`.
- `src/local_work_list.rs`: `print_work_list` / `print_work_lists` for one-liner.
- `src/main.rs`: `Command::Memo` must be added to unreachable mailspace arm.

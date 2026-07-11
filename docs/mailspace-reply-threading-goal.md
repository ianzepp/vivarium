# Goal: Mailspace Reply Threading

## Summary

Give project-local Vivi mailspace messages (`mail` / `task` / `need` / `want`) a
real reply lineage: capture an explicit parent link when an agent replies, and
render conversation history from any handle. Today the mailspace stores flat,
unrelated messages; agents reconstruct conversations by guessing from
timestamps, subject prefixes, and handle citations pasted into bodies. This
goal finishes the thread model that the original local-mail/tasks delivery
specified but never shipped, keeping the bag model intact and adding **no**
stage-gate or coordination-database machinery.

## Problem

Multi-agent factory runs (hunter/codex, gatherer/reviewer, scout/strategist)
hold their entire coordination in project-local mailspace mail. The
conversation is inherently cross-kind and reply-shaped — a `mail` answers a
`need`, a `task` references a `mail`, a `want` is closed with a note pointing
elsewhere. None of that lineage is captured or queryable today:

- **No reply link is stored.** `vivi mail send` accepts `--from/--to/--cc/
  --subject/--body/--body-file` only. There is no `--reply-to` / `--in-reply-to`
  flag, no `vivi mail reply` subcommand, and no parent/thread/references column
  on the stored record (`DeliveredMessage` in `src/mailspace.rs`; `DumpRecord`
  in `src/mailspace/dump.rs`).
- **No thread view exists for the mailspace.** `vivi mail` exposes only
  `send/deliver/list/show/dump`. `vivi task show` renders the root message but
  not thread context.
- **Threading works for email, not for the mailspace.** The account-scoped
  email path writes `In-Reply-To`/`References` headers
  (`src/message/compose.rs`), indexes them (`src/email_index/links.rs`), and
  reconstructs conversations (`src/thread.rs`, `vivi thread <id>`). None of that
  runs over project-local mailspace messages.
- **The cost is real and observed.** Reconstructing a 12-hour multi-agent
  exchange (faberlang mailspace, 2026-07-10) required hand-sorting ~54 rows by
  timestamp and inferring links from subjects and in-body handle citations such
  as *"Answering need `cd294c6` and mail `0e0c0cd`."* That body-citation
  convention is the de-facto thread protocol, carried entirely by agent
  discipline, with no structural support.

## Prior Art (this goal finishes deferred design)

`docs/local-agent-mail-tasks-delivery-plan.md` specified a thread-first model
and then deferred it. The intended shape is on record:

- *"The email thread remains primary. Clarifying questions and status updates
  should be replies to the root task message, not detached task comments in a
  separate system."*
- Designed command shape:
  ```
  vivi task show 9f3a8c2
  vivi mail reply 9f3a8c2 --from cto --body "Do you want this in v1?"
  vivi task done 9f3a8c2 --for cto --note "Implemented and tested."
  ```
  i.e. `mail reply <handle>` resolves **any** handle (task/need/want/mail) as a
  thread root — kind-agnostic by original design.
- *"optional notes should be represented as normal reply messages in the
  thread, not by rewriting the task body."*
- *"generated message IDs are stable enough for threading but unique enough for
  repeated sends."*

Deferred items called out explicitly in that plan's "Partially completed or
deferred" section:

- `vivi mail reply <task-handle>` does not yet resolve task handles as thread
  roots.
- `vivi task show` renders the root message, but does not yet render full
  thread context.
- `task`/`need`/`want` lifecycle commands accept `--note`, but the note is only
  recorded on the `mailspace_events` move row. It does not create a message or
  appear in a conversation thread.

This goal exists to close those deferred items.

## Goals

1. **Reply capture:** `vivi mail reply <handle>` (and a `--reply-to <handle>`
   flag on `mail`/`task`/`need`/`want send`) record an explicit parent link,
   resolving any handle as a thread root regardless of kind.
2. **Thread view:** `vivi mail thread <handle>` (and thread-context rendering
   inside `task`/`need`/`want show`) assemble the conversation from any node —
   ancestors and descendants — with `--json` and a depth/age cap.
3. **Kind-agnostic lineage:** a parent link may target any mailspace kind, so a
   mail answering a need and a task referencing a mail both thread correctly.
4. **Note-as-reply:** preserve the existing lifecycle event note and, when
   `--note` is supplied, also create a normal reply message in the same
   operation. The event ledger remains the authoritative lifecycle audit; the
   reply is the conversational rendering of that note.
5. **Historical best-effort:** for messages sent before capture existed, infer
   links from body handle-citations, `Re:`/`Re[]:`-style subject prefixes, and
   timestamp ordering — and **clearly mark those links as inferred**, never
   conflating them with authoritative parent links.
6. **Dump/list surfacing:** `dump --json` exposes the parent link (and inferred
   flag) so agents and tooling can walk lineage without a separate store.

## Non-goals

- **No new coordination database.** Lineage is a read view over existing
  messages plus one captured relation; no parallel thread/gate table.
- **No stage GO/NO-GO gate objects** (consistent with
  `mailspace-agent-control-plane-goal.md` non-goals). Reply lineage is not a
  license or workflow state.
- No changes to IMAP/Proton/sync or the account-scoped email thread path
  (`src/thread.rs`, `email_index/`) beyond reusing its patterns; this goal is
  mailspace-local.
- No required rewrite of historical mail. Inferred linkage is opt-in and
  non-authoritative.
- No rename of identities or change to folder roles (`tasks`/`needs`/`wants`/
  `done`/`inbox`); handle stability across folder moves is preserved.

## Ground Truth Researched

- Binary `vivi` 4.5.0; project mailspace store is `<project>/.vivi/mail.sqlite`.
- `vivi mail send` flags: `--from --to --cc --subject --body --body-file
  --project` (and global flags). No reply/in-reply-to input.
- `vivi mail` subcommands: `send deliver list show dump` — no `reply`, no
  `thread`.
- `DeliveredMessage` (`src/mailspace.rs`): `handle`, `from`, `to`, `cc`,
  `subject`, `body`, `role`, `kind`. `DumpRecord` (`src/mailspace/dump.rs`):
  `handle, message_id, role, kind, status, date, from, to, cc, subject, body`.
  No parent/in_reply_to/thread/references field on either.
- Send path: `send_mail` in `src/local_mailspace_command.rs`; delivery core
  `SendRequest`/`DeliveryResult` in `src/mailspace/delivery.rs`; event ledger
  in `src/mailspace/event_log.rs`. Message rows are read via the shared storage
  layer (`storage.read_message`, `storage.list_mailspace_events`,
  `storage.display_handle`).
- Email-thread reference implementation: `src/message/compose.rs` (writes
  `In-Reply-To`/`References` on reply drafts), `src/email_index/links.rs`
  (indexes those headers), `src/thread.rs` + `vivi thread <id> --json --limit`
  (assembles local thread context for account email).
- Original design intent and deferred status:
  `docs/local-agent-mail-tasks-delivery-plan.md` (Task Identity And Threads;
  Partially completed or deferred).
- Sibling goal: `docs/mailspace-agent-control-plane-goal.md` (board/brief/
  delta/actionable/dump-safety) — complementary, not overlapping.

## Reference Packet

Before implementing, inspect:

| Path | Why |
| --- | --- |
| `src/mailspace.rs` | `DeliveredMessage` and status types to extend with parent |
| `src/mailspace/delivery.rs` | `SendRequest` / `send` — where a reply target is resolved and stored |
| `src/mailspace/dump.rs` | `DumpRecord` to surface parent content id + link source |
| `src/mailspace/event_log.rs` | event ledger for reply/done-note events |
| `src/mailspace/kind.rs` | kind taxonomy for cross-kind link validation |
| `src/local_mailspace_command.rs` | `send_local_mail`/`send_mail`; add `reply` + `--reply-to` |
| `src/local_work_command.rs` | task/need/want send/done; `--note` as reply; show thread context |
| `src/storage.rs`, `src/storage/schema.rs` | message rows, migrations, handle resolution |
| `src/message/compose.rs` | reference: how email writes In-Reply-To/References |
| `src/email_index/links.rs`, `src/thread.rs` | reference: link indexing and thread assembly |
| `src/cli/mailspace_command/` | clap shapes for mail/work commands |
| `docs/local-agent-mail-tasks-delivery-plan.md` | original thread-model spec being completed |
| `docs/mailspace-agent-control-plane-goal.md` | sibling goal; share dump/list touch points |
| `README.md`, `AGENTS.md` | user/agent docs and validation |

## Constraints And Invariants

- **Mailspace stays project-local and side-effect-free** for
  `mail`/`task`/`need`/`want` ops; no network, no account-store writes.
- **Handle stability invariant:** a parent link targets a stable message
  identity that survives folder moves (tasks→done, wants promotion). Resolve
  via the same identity basis the delivery plan specifies (root message id /
  content id), not current folder.
- **Authoritative vs inferred must never be conflated.** Captured parent links
  are authoritative; historically-inferred links carry an explicit `inferred`
  marker in storage and output.
- **Kind-agnostic by default.** A parent may be any mailspace kind; validation
  rejects only unknown/stale handles, not cross-kind links.
- **Read view, not a second store.** Lineage is derived from messages + one
  relation; do not build a parallel thread database or gate table.
- Prefer extending existing modules; hold hygiene ceilings (file/function size)
  via extraction. Production errors stay in `VivariumError`/`thiserror`; clap
  derive for CLI; `--project` walk-up discovery unchanged.
- `--project` walk-up and existing `mail`/`task`/`need`/`want` semantics must
  not break.

## Architecture Direction

Model lineage as **one captured relation over existing messages**, plus a
read-side assembler:

```text
mail/task/need/want message (content_id shared across local copies)
        │
        ├── parent_content_id  (authoritative, when reply target supplied)
        │
        ▼
   thread assembler  ──►  vivi mail thread <handle>  |  show --thread
        │
        └── inferred links (body handle-citations, subject prefixes, time)
                 marked inferred; never overwrite an authoritative parent
```

- **Canonical identity:** `content_id` identifies one logical local message
  across its sender and recipient copies. A new `mailspace_links` table stores
  `child_content_id` (primary key), `parent_content_id`, and `source`
  (`captured` or `inferred`), each referencing `blobs(content_id)`. Do not put
  the relation on `messages`: that would duplicate or disagree across copies.
- **Capture:** `mail reply <handle>` and `--reply-to <handle>` on send resolve
  the handle to a `message_id` at send time and store the link. Unknown or
  ambiguous handles fail closed with candidate matches (same behavior as
  existing handle resolution).
- **Assembly:** walk ancestors and descendants from any node; cap depth/age;
  serialize text + `--json`.
- **Notes:** `--note` on done/reopen records a reply message carrying the note
  body, instead of mutating the root body — matching the original plan.

Captured replies also write RFC5322 `In-Reply-To` and `References` headers for
portable message shape, but `mailspace_links` is the authoritative query seam.
Headers are not a substitute for the local relation and historical inference
never rewrites blobs.

## Supporting Skills

- `factory`: multi-phase implement/verify/commit against this goal.
- `delivery`: compile each phase into a delivery spec before coding.
- `mail`: when changing Vivi CLI workflows and docs.
- `goal-check`: optional second pass before factory vision if desired.
- `red-green` / `correctness`: CLI behavior and regression tests for reply,
  thread assembly, inferred-link marking, handle-stability across moves.
- `zombie-docs`: keep this goal and README claims honest against shipped code.

## Implementation Shape

Rough factory phases (delivery may merge/split at boundaries):

### Phase 1 — Storage + capture (smallest useful)

- Add the `mailspace_links` table and indexes through idempotent
  `ensure_schema`; existing mailspaces require no row rewrite.
- `vivi mail reply <handle>` and `--reply-to <handle>` on
  `mail`/`task`/`need`/`want send`; resolve handle → `message_id`; store link;
  fail closed on unknown/ambiguous.
- `dump --json` surfaces `parent_content_id` and `link_source`.

### Phase 2 — Thread view

- `vivi mail thread <handle>` assembles ancestors + descendants with
  `--json`, `--limit`, age/depth cap.
- `task`/`need`/`want show` render thread context below the root (finishing the
  deferred `task show` behavior).
- Tests with a fixture mailspace: linear chains, forks, cross-kind links,
  stable handles after tasks→done.

### Phase 3 — Note-as-reply

- `--note` on all existing task/need/want lifecycle verbs keeps the current
  event note and additionally records a reply message parented to the item.
  If reply creation fails, the move and its event must roll back as one atomic
  operation; extracting a storage transaction seam is part of this phase.

### Phase 4 — Historical inferred linkage (opt-in)

- Best-effort inference from body handle-citations, `Re:`/`Re[N]:` subject
  prefixes, and timestamp ordering; mark `inferred = true`; never overwrite an
  authoritative parent. Expose via `thread`/`dump` with a clear visual/json
  distinction. Guardrail: refuse to assert inferred links that conflict with an
  authoritative parent.

### Phase 5 — Docs, examples, release notes

- README agent-workflow section: reply/thread usage; document authoritative vs
  inferred. CHANGELOG/release note for the new subcommands and any default
  change. Integration test covering reply → thread → done-note.

## Release Posture

Decision: **release checkpoint when Phase 1–2 land** (new `mail reply`/
`mail thread` commands and a schema migration). Minor version bump with a
migration note. The schema change is additive (nullable column) and
non-breaking for existing mailspaces. Publication (crates/Homebrew) only with
operator approval after local `cargo test` green.

## Exit Strategy

Decision: **included**

- New commands and the link table are additive; remove or feature-gate only
  if broken — prefer fix.
- Inferred linkage may be disabled via flag/env if it produces noisy or wrong
  reconstructions; authoritative capture is never removed to satisfy inference.
- If the migration cannot be made safe for existing `.vivi/mail.sqlite`, land
  thread capture on new rows only and document the cutover rather than rewriting
  history.

## Acceptance Criteria

- `vivi mail reply <handle>` and `send --reply-to <handle>` record an
  authoritative parent link for any mailspace kind, resolving through stable
  identity (survives tasks→done, wants promotion).
- `vivi mail thread <handle>` and `task`/`need`/`want show` render the full
  conversation (ancestors + descendants) from any node, in text and `--json`.
- A `mail` can reply to a `need`; a `task` can reply to a `mail`; both assemble
  into one thread (kind-agnostic).
- Lifecycle `--note` preserves the existing event-ledger note and atomically
  creates a reply in the thread without rewriting the root body.
- Inferred links for pre-capture history are produced only on request, marked
  `inferred`, and never override an authoritative parent.
- `dump --json` exposes `parent_content_id` and `link_source`.
- Existing `mail`/`task`/`need`/`want` semantics, handle stability, and
  `--project` discovery are unchanged.
- `cargo fmt --check`, `cargo test --test hygiene`, and `cargo test` pass.

## Validation

- Unit/integration tests with temp project mailspaces: reply capture, linear and
  forked threads, cross-kind links, handle stability after folder move,
  inferred-vs-authoritative marking, ambiguous/unknown handle failure.
- Manual: seed mail+need+task across identities, reply cross-kind, run
  `mail thread` from a leaf and from the root; promote a want and confirm the
  thread still resolves; confirm `--note` appears as a reply.
- Review: confirm docs and help text do not introduce gate/license semantics.
- Hygiene ceilings still hold on touched modules.

## Locked Decisions

- Store authoritative and inferred relations in `mailspace_links`, keyed by
  logical-message `content_id`; also emit standard reply headers for captured
  replies.
- Historical inference is default-off behind `mail thread --infer`; it is a
  read-side result unless the user later authorizes a separate persistence
  command. `dump` reports only persisted captured links in this goal.
- The project-local surface is `vivi mail thread`; do not overload the existing
  account-email `vivi thread` command.
- Apply note-as-reply consistently to every lifecycle verb that already accepts
  `--note`: task and need done/reopen, plus want promote/done/drop.
- `mail reply <handle>` defaults recipients to the parent message's sender and
  excludes the replying identity; `--to`/`--cc` may explicitly override/add.
  It requires `--from`, `--body`/`--body-file`, accepts optional `--subject`,
  and otherwise uses `Re: <parent subject>` without stacking `Re:` prefixes.

## Stop Conditions

- Stop if the feature would require a second coordination DB or gate table to
  meet acceptance (revisit architecture with operator).
- Stop if handle stability across folder moves cannot be preserved for parent
  links (then capture on new rows only and document the cutover).
- Stop before any network-facing or account-store changes.
- Stop if hygiene or the full test suite cannot be made green without weakening
  policy.

## Factory Handoff

| Item | Value |
| --- | --- |
| Repo | `/Users/ianzepp/work/ianzepp/vivarium` |
| Goal artifact | `docs/mailspace-reply-threading-goal.md` |
| Prior art | `docs/local-agent-mail-tasks-delivery-plan.md` (Task Identity And Threads; deferred items) |
| Sibling goal | `docs/mailspace-agent-control-plane-goal.md` (board/brief/delta) |
| Motivating evidence | faberlang mailspace 2026-07-10 multi-agent exchange; manual reconstruction of ~54 rows |
| Suggested start | Phase 1 (storage + capture) |
| Ready for | **factory** (vision → production → delivery → loop) |

## Handoff Readiness

**Ready for factory** — the problem, prior art, kind-agnostic direction,
non-goals (no gate/coordination-DB), first milestone, validation, and stop
conditions are grounded in the live CLI surface, the storage/delivery code, and
the original deferred design spec. Open questions are bounded and carry
recommendations.

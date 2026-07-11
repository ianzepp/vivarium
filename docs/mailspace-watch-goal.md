# Goal: Mailspace Watch (Blocking Board Alarms)

## Summary

Add a **project-local mailspace watch** surface so agents (especially a fleet
**Mind** / gatherer ops loop) can **block until the board changes** instead of
busy-polling `board` / `list` on a fixed cadence or scraping `mail.sqlite` with
ad hoc scripts.

Watch is the long-running form of **board delta**: one process waits for
delivered mail, task/need/want lifecycle events, or a specific handle flip, then
prints a machine-readable event and exits (or streams until timeout). It does
**not** replace the bag model, does **not** invent stage GO/NO-GO gates, and does
**not** merge with IMAP/`sync-events --watch` (remote account watch stays
separate).

## Problem

Multi-agent fleets coordinate through project mailspace messages
(`.vivi/mail.sqlite`: `task` / `need` / `want` / `mail`). Real control loops
(e.g. faberlang Mind/Hand fleet under `$fleet`) hit a latency gap:

- **Mind files work and doorbells a Hand**, then only notices turn-end mail or
  `task done` on the **next scheduled cycle** (often 3–5 minutes), even though
  the delivery hit the local store immediately.
- **No first-class “wait for reply” primitive.** Agents invent `fswatch` on
  `mail.sqlite`, custom fingerprint files, or tighten the whole gatherer
  interval—fragile, noisy, and outside Vivi’s CLI contract.
- **Board delta already answers “what changed since T?”**
  (`docs/factory/mailspace-control-plane-phase-04-delta-delivery.md`) but is
  **snapshot-only**. There is no blocking/streaming mode that sleeps until the
  next matching event.
- **IMAP watch is the wrong layer.** `vivi sync-events --watch` and account
  `watch` target remote Proton/IMAP. Project turn-ends never pass through that
  path; watching IMAP does not wake Mind for local mailspace delivery.
- **`agent poll` is also the wrong layer.** It claims trusted external inbox
  threads for Codex-style dispatch, not “block until `hunter-2` turn-end lands
  in the project mailspace.”

Without watch, the Mind remains a pure poller. With watch, the Mind can run
**expecting-reply mode**: block on a filtered event, then run one paid cycle.

## Relationship To Prior Goals

| Prior work | Relationship |
| --- | --- |
| `docs/mailspace-agent-control-plane-goal.md` | Parent control-plane goal. **Deferred:** “Watch/stream mode.” This goal **lifts** that deferral into a dedicated factory goal. |
| Phase 04 board delta + watermarks | **Prerequisite read model.** Watch should reuse event timestamps, kinds, and watermark/cursor ideas—not invent a second history store. |
| Phase 04 constraints | Explicitly out of scope then: “No … watch mode.” That was correct for Phase 04 sizing; this goal is the follow-on. |
| IMAP `watch` / `sync-events --watch` | **Orthogonal.** Keep remote watch commands; name project watch so agents do not confuse the two. |
| `docs/mailspace-reply-threading-goal.md` | Orthogonal. Threading improves conversation view; watch improves **liveness**. No dependency either way. |

## Goals

1. **Primary command:** `vivi mailspace watch` blocks (or streams) until matching
   project-mailspace events occur for a selected identity (and optional filters).
2. **Kind coverage:** Support events for **mail**, **task**, **need**, and
   **want** (not wants alone). Optional sugar aliases:
   - `vivi mail watch …`
   - `vivi task watch …`
   - `vivi need watch …`
   - `vivi want watch …`
   each mapping to `mailspace watch --kinds <kind>`.
3. **Event model:** Emit structured events with at least:
   - `event` (`delivered`, `moved`, or `sent_copy_created`)
   - `status` (derived destination state such as `tasks`, `needs`, or `done`)
   - `kind` (`mail|task|need|want`)
   - `handle`, `for` (identity), `from`, `subject`, `at` (timestamp)
   Text and `--json` lines both required for agent use.
4. **Filters (agent-critical):**
   - `--for <identity>` (required or strongly defaulted)
   - `--kinds mail,task,need,want`
   - `--events delivered,moved` (raw ledger event types)
   - `--statuses tasks,needs,wants,done,inbox` (derived destination states)
   - `--match-from <identity>`
   - `--match-subject-prefix <str>` (e.g. `strategist report:`, `turn end:`)
   - `--handle <handle>` — wait until **that** item changes (done/reopen/etc.)
5. **Stop / run modes:**
   - `--until-count N` (default `1`) — exit after N matching events; `0`
     means follow until interrupted
   - `--timeout <duration>` — exit non-zero if nothing matched
   - `--once` — single poll of “anything new since cursor,” no long block
   - long-running stream mode (optional flag or default when `--until-count` omitted
     / set high) with clear docs for fleet Mind wrappers
6. **Cursor / watermark:**
   - `--since <time>` (same forms as board/dump: RFC3339, `Nh`, `Nd`, …)
   - `--cursor-file` / `--watermark-file` read
   - `--write-cursor` / `--write-watermark` after successful match (opt-in)
   Cursor is **caller-owned**; Vivi does not require a global Mind registry.
   The persisted cursor is the last scanned monotonic `mailspace_events.event_id`,
   not a timestamp. `--since` establishes the initial lower bound only when no
   cursor file exists.
7. **Implementation honesty:** Prefer **poll + event-id cursor** over mailspace events
   (simple, testable). Optional later optimization: FS notify on `mail.sqlite`
   with debounce—must not change CLI contract.
8. **Docs:** README + agent examples for “file work → watch → one cycle.” Explicit
   contrast with IMAP watch. No GO/NO-GO stage-gate API.

## Non-goals

- No push into a live LLM session (no automatic Grok interrupt). Watch **exits or
  prints**; wrappers (tmux send-keys, scheduler, Mind loop) decide how to wake.
- No peer-to-peer agent chat bus; no piping tmux TUIs.
- No second coordination database or stage-gate subsystem.
- No IMAP/Proton event bridging into mailspace watch.
- No requirement that Hands rename tmux sessions after handles (lease stays on
  the board; watch observes board events).
- No rewriting of control-plane board defaults beyond what watch needs.
- No UI/TUI conference room.

## Ground Truth Researched

- Project mailspace: local SQLite under `.vivi/` (faberlang and similar camps).
- CLI today: `vivi board`, `task|need|want|mail list/show/send/done`,
  `mailspace status`—all snapshot/read or mutators, no blocking watch.
- Board delta: `docs/factory/mailspace-control-plane-phase-04-delta-delivery.md`
  (`--since`, watermark file, write-back); events expose `occurred_at`.
- Control-plane goal deferred “Watch/stream mode”
  (`docs/mailspace-agent-control-plane-goal.md`).
- Remote watch exists: `src/watch.rs` (IMAP), `sync-events --watch` (Proton
  events)—must not be overloaded for local mailspace.
- Fleet process need: Mind as message bus; Hands send turn-end mail; Mind should
  optionally block until inbound rather than wait for 5m gatherer fire
  (`$fleet` / multi-agent camp ops).

## Architecture Direction

```text
mailspace store (events + messages)
        │
        ▼
  watch loop: poll (or fsnotify+debounce)
        │ filter: identity, kinds, events, from, subject, handle
        ▼
  emit event line(s)  →  exit | continue
        │
        ▼
  Mind wrapper / scheduler / operator
```

- **Single source of truth:** existing mailspace messages + event history.
- **Canonical source:** query `mailspace_events` in ascending `event_id` order;
  add one storage query that reads events after an event id with account/kind
  filtering performed without rescanning every message.
- **Cursor:** decimal `event_id` plus a trailing newline, owned by the caller.
  Advance it to the last event scanned after successful output, including
  non-matching events, so filters do not repeatedly rescan history. Write via a
  temporary sibling followed by rename. Missing/empty files mean no cursor;
  malformed files fail closed.
- **Event vocabulary:** preserve ledger `event_type` values (`delivered`,
  `moved`, `sent_copy_created`) in the raw `event` field. Also emit derived
  `kind` from `X-Vivi-Kind`/role and `status` from the destination role. Do not
  invent `done`, `reopened`, or `promoted` event names in this goal.
- **Exit codes:** `0` matched (or `--once` completed), `1` timed out without a
  match, and ordinary Vivi nonzero error handling for invalid input/storage
  failures.
- **Concurrency:** multiple watchers OK; no exclusive lease of the whole
  mailspace required for v1 (read-only observation).
- **Polling:** 250 ms default interval, overridable with `--poll-interval` for
  tests/operators. Open the mailspace per poll so a long-lived read transaction
  never hides newly committed events.

## Implementation Shape (phased)

### Phase 0 — Storage event scan

- Add a paged `mailspace_events` query ordered by `(event_id)` with an exclusive
  lower bound and deterministic tests.
- Reuse `parse_time_bound` only to translate `--since` into the initial event-id
  boundary; do not use timestamps as the durable cursor.

### Phase 1 — Core `mailspace watch` (MVP)

- `vivi mailspace watch --for <id> [--kinds …] [--until-count 1] [--timeout …]
  [--since …] [--json]`
- Poll implementation; default kinds are `mail,task,need`; `want` is opt-in.
- Integration tests with temp mailspace: send task → watch sees delivered;
  task done → watch with `--handle` sees done.

### Phase 2 — Filters + cursor file

- `--match-from`, `--match-subject-prefix`, `--handle`, `--events`
- `--cursor-file` / `--write-cursor` (or watermark naming aligned with board)
- `--once` for scripted non-blocking delta check

### Phase 3 — Kind aliases + docs

- `task|need|want|mail watch` sugar
- README fleet/Mind examples; contrast IMAP watch
- CHANGELOG / release note

### Phase 4 (optional) — FS notify optimization

- Debounced watch on store file; same CLI; fall back to poll if notify fails

## Acceptance Criteria

- An agent can run one command that **blocks until** a matching local mailspace
  event occurs, then exits with structured output—without ad hoc `fswatch`
  scripts.
- Watch can wait on **mail delivery**, **task/need done**, and optional **want**
  lifecycle events.
- Watch can wait on a **specific handle** changing state.
- Watch can filter by recipient identity, sender, subject prefix, and kinds.
- Cursor/watermark files allow a Mind loop to avoid re-firing on old events.
- Help text and README make clear this is **project-local**, not IMAP watch.
- `cargo fmt --check`, `cargo test --test hygiene`, and `cargo test` pass.
- **No** stage-gate API; **no** second store required for acceptance.

## Validation

- Unit tests for filter matching and cursor advance.
- Integration tests with fixture mailspaces and controlled event times.
- Manual smoke:
  1. Terminal A: `vivi mailspace watch --for reviewer --kinds mail --json --timeout 2m`
  2. Terminal B: `vivi mail send --from hunter-1 --to reviewer --subject "turn end: test" --body ok`
  3. A prints one JSON event and exits 0.
  4. Repeat with `task send` + `task done --handle …` and `--handle` filter.
- Review: no confusion with `sync-events --watch` in help strings.

## Locked Decisions

- `vivi mailspace watch` is the canonical command. Kind aliases are Phase 3;
  there is no bare `vivi watch` in this goal.
- Default kinds are `mail,task,need`; `want` is opt-in.
- Default mode is alarm-shaped (`--until-count 1`); `--until-count 0` follows.
- Phase 1 is polling only. Filesystem notification is an optional later
  optimization that cannot alter output or cursor semantics.
- Timeout exits `1`; invalid arguments and storage errors use normal Vivi error
  handling.

## Stop Conditions

- Stop if watch requires a second coordination DB or gate table—revisit with
  operator.
- Stop if correct event observation cannot be done from existing message/event
  history without a full store redesign—file residual architecture work first.
- Stop before changing IMAP/Proton watch behavior under this goal.
- Stop if tests/hygiene cannot go green without weakening policy.

## Factory Handoff

| Item | Value |
| --- | --- |
| Repo | `/Users/ianzepp/work/ianzepp/vivarium` |
| Goal artifact | `docs/mailspace-watch-goal.md` |
| Feedback sources | Faberlang fleet Mind latency (turn-end mail vs 5m gatherer); operator design for Mind-as-bus + expecting-reply; control-plane deferred “Watch/stream mode” |
| Depends on | Board delta / event timestamps (control-plane Phase 04 lineage) |
| Suggested start | Phase 0 → Phase 1 MVP `mailspace watch` |
| Ready for | **factory** (vision → production → delivery → loop) |

## Handoff Readiness

**Ready for factory** — problem, architecture, non-goals (especially no IMAP
confusion and no gate subsystem), phased shape, acceptance, validation, and
stop conditions are grounded enough for factory admission. Open questions are
bounded and have recommendations.

# Goal: Mailspace Agent Control Plane

## Summary

Upgrade project-local Vivi mailspace (`task` / `need` / `want` / `mail` / `mailspace`)
from a mail-and-dump surface into an **agent control plane**: compact board and
delta views, actionable queues, machine-readable list/status, safer long-body
intake, and dump defaults that do not drown long-running agent loops. Preserve
the bag model (open work items) and deliberately **do not** add a first-class
stage GO/NO-GO gate subsystem.

## Problem

Multi-agent factory runs (hunter/codex, gatherer/reviewer, scout/strategist) on
project mailspaces hit these product gaps during real campaigns (e.g. Faber AI
Workbench Stage 3/4):

- **`dump` is the heartbeat by accident.** Full task/need/want dumps include
  bodies, event histories, and done archaeology. Agents copy handoff prompts that
  say “dump the board,” burn context, and miss flips in the noise.
- **`list` is too thin for selection.** Handle + from + subject is not enough to
  prioritize without `show`ing everything. Missing: age, last event, kind, owner
  expectations, and machine-readable output.
- **No single “what is open / what changed” command.** Agents chain
  `mailspace status` + three dumps/lists, invent baseline JSON files beside
  `.vivi/`, and still disagree about board state.
- **Unread inbox counts dominate mental models** while actionable work is open
  tasks/needs. Status tables report unread mail next to work without ranking
  **actionable bag** first.
- **Long evidence bodies are awkward.** `@path` body reads exist for some send
  paths; agents still want explicit `--body-file` / stdin `-` and docs that
  match real multi-paragraph residual evidence.
- **Want bags never drain.** Promote exists; archive/drop/done-for-wants and
  board caps do not. Campaigns accumulate dozens of obsolete wants.
- **Gate vocabulary leaked into process design.** Codex valued durable GO/NO-GO
  tokens; the same tokens caused hunter exit while gatherer later flipped
  closeout, leaving supervisors in empty 5m loops. Vivi must not encode
  “stage license” as a core object. Open tasks/needs are the work signal.

## Goals

1. **Board view:** `vivi board` (or `vivi mailspace board`) prints a compact
   per-identity or project-wide summary of open tasks, needs, wants (capped),
   and optional short inbox subject lines—default human text plus `--json`.
2. **Delta / brief:** `vivi board --since <time|duration>` and/or
   `vivi brief --since …` show only items created or moved since a watermark
   (RFC3339, `Nh`/`Nd`, or a handle’s timestamp). Optional
   `--watermark-file` for agent baselines.
3. **Actionable list filters:** `vivi task list` / `need list` (and board)
   support richer columns and `--json`. An `--actionable` (or board default)
   mode surfaces open tasks + needs for an identity and **does not** promote
   wants or unread mail as primary work.
4. **Dump safety:** task/need dump default status is **`open`** (not `all`).
   Large dumps truncate or refuse with guidance to pass `--handle`,
   `--status open`, `--since`, or `--json` to a file. Done archaeology remains
   available via explicit `--status all|done`.
5. **Body intake:** Document and complete long-body send ergonomics:
   existing `@path` behavior, plus `--body-file <path>` and/or `--body -`
   (stdin) on local `mail`/`task`/`need`/`want` send commands where missing.
6. **Want lifecycle:** `want` supports closing obsolete items (done/drop/archive
   or equivalent folder move) in addition to `promote`. Board caps or
   `--wants N` prevent want scrapbooks from dominating output.
7. **Status honesty:** `mailspace status` (text + `--json`) reports
   `actionable_open` (tasks+needs) distinctly from unread mail and wants, so
   agents can key goal loops on **bag emptiness**.
8. **Docs + agent guidance:** README / AGENTS-oriented examples show
   **list/board first, show one handle, dump only for audit**—not dump as the
   default loop intake.

## Non-goals

- **No first-class stage gate objects** (`vivi gate status`, binding
  `requested_gate`, stage GO/NO-GO protocol as product API). Review requests
  remain ordinary tasks (optional template later), not licenses.
- No replacement of campaign/delivery markdown control planes.
- No IMAP/Proton/sync changes; mailspace-local only unless a shared CLI helper
  is strictly required.
- No full project-manager features (milestones, assignees beyond identities,
  kanban UI).
- No forced rename of identities to hunter/gatherer (process vocabulary stays
  outside Vivi core; board semantics support that process).
- No mass-delete without explicit dry-run/confirm patterns for hygiene commands.
- Do not break existing handle stability or folder roles (`tasks`/`needs`/
  `wants`/`done`/`inbox`).

## Ground Truth Researched

- Package `vivarium`, binary `vivi` 4.4.0; `AGENTS.md` validation:
  `cargo fmt --check`, `cargo test --test hygiene`, `cargo test`.
- Local mailspace landed (`docs/local-agent-mail-tasks-delivery-plan.md`,
  commit lineage including `0d132d0` era): `.vivi/`, identities, mail/task
  send/list/show/done, dumps.
- CLI surface today: `vivi task|need|want list|dump|send|show|…`,
  `vivi mailspace status`, dump filters include `--since` / `--json` /
  `--status` (dump default **all** for tasks—agent hazard).
- `src/mailspace.rs` `read_body_arg`: `@path` reads file bodies for send paths
  that use it; not the same UX as `--body-file` / stdin and easy for agents to
  miss.
- `src/local_work_command.rs`, `src/cli/mailspace_command/`,
  `src/mailspace/dump.rs`: list is terse; dump is full audit records.
- Campaign feedback (Faber AI Workbench, 2026-07-09):
  - Codex want `a32d0dd` on faberlang mailspace: brief/delta, gate status,
    body-file, actionable list, review template, board delta, `--json`
    everywhere; liked GO/NO-GO durability.
  - Strategist merge (mail `0274350`): agree on brief/board/actionable/json/
    body-file; **reject** binding gate subsystem; prefer bag = open
    tasks/needs; review request only as ordinary task intake if added.
  - Operator process lesson: hunter exited on NO-GO; gatherer later GO’d;
    supervisors looped empty; Stage 4 “NO-GO” meant “not selected,” not ban.

## Reference Packet

Before implementing, inspect:

| Path | Why |
| --- | --- |
| `src/cli/mailspace_command/` | clap shapes for mailspace/work |
| `src/local_work_command.rs` | task/need/want dispatch |
| `src/local_mailspace_command.rs` | mail + status |
| `src/mailspace/` | core mailspace types, dump, send, list |
| `src/mailspace/dump.rs` | dump records and filters |
| `src/local_mailspace_dump.rs` | dump rendering / json |
| `docs/local-agent-mail-tasks-delivery-plan.md` | prior delivery history |
| `README.md` | user/agent docs to update |
| `AGENTS.md` | validation and code standards |
| `tests/` | integration patterns for CLI |

## Constraints And Invariants

- Local mailspace remains **project-local**, no network side effects for
  `mail`/`task`/`need`/`want` board ops.
- Handles remain stable across folder moves.
- Prefer extending existing modules; keep hygiene ceilings (400-line files,
  60-line functions) via extraction, not one mega-command file.
- Production errors stay in `VivariumError` / `thiserror` (no new `anyhow`).
- Clap derive for CLI; `--project` walk-up discovery continues to work.
- **Work signal invariant:** open tasks + open needs for an identity are
  actionable work. Wants and unread mail are secondary. Status/board must not
  imply a stage “permission bit.”
- Clean break OK for dump default (`all` → `open`) if release notes call it
  out; provide `--status all` for old behavior.
- Do not parse or require agents to open `.vivi/mail.sqlite` directly.

## Architecture Direction

Treat board/brief as a **read model over existing folder mail**, not a new
coordination database:

```text
mail.sqlite + blobs  →  query open/changed work items  →  board | brief | list --json
                              ↓
                     optional watermark file (agent-owned)
```

- **Canonical store:** existing mailspace messages + events.
- **Board/brief:** composition layer (filter, sort, cap, serialize).
- **No parallel gate table.** If a “review request” helper is ever added, it
  only `task send`s a structured body to the gatherer identity.

Optional later (out of this goal unless free): subject/body templates for
review requests—still tasks, still non-binding.

## Supporting Skills

- `factory`: multi-phase implement/verify/commit against this goal.
- `delivery`: compile each phase into a delivery spec under `docs/factory/`
  (or adjacent phase docs) before coding.
- `mail`: when changing Vivi CLI workflows and docs.
- `goal-check`: optional second pass before factory vision if desired.
- `red-green` / `correctness`: CLI behavior and regression tests for list/board
  defaults.

## Implementation Shape

Rough factory phases (delivery may merge/split at boundaries):

### Phase 1 — Dump safety + list JSON/columns (smallest useful)

- Default task/need dump status → `open`; document `--status all`.
- Dump size guard or truncate with remediation message.
- `task list` / `need list` / `want list`: `--json`; richer columns (created/age,
  from, handle, subject; last event if cheap).
- Tests for default dump and list JSON shape.

### Phase 2 — Board + status actionable counts

- `vivi board` (name may be top-level or under `mailspace`) with
  `--for`, `--project`, `--json`, wants cap.
- `mailspace status` gains clear actionable totals (text + json).
- Docs: agent intake = status → board/list → show handle.

### Phase 3 — Brief / since / watermark

- `--since` on board (and list where missing).
- Optional `--watermark-file` read/write for agent loops.
- Tests with fixture mailspace and controlled timestamps/events.

### Phase 4 — Body intake + want lifecycle

- `--body-file` / stdin `-` aligned across local send commands; document `@path`.
- Want close/drop/archive (or done-equivalent) + list status if needed.
- Optional send-time subject dedupe **warn** (non-fatal) for open items.

### Phase 5 — Docs, examples, release notes

- README agent workflow section; deprecate dump-as-heartbeat in examples.
- CHANGELOG / release note for dump default break.
- Manual smoke script or integration test covering board → show → done.

### Deferred (explicitly later)

- `vivi review request` template helper (task-only; no gate).
- `task next` selection policy.
- Bulk want archive with dry-run.
- Watch/stream mode.

## Release Posture

Decision: **release checkpoint required** when Phase 1–2 land (CLI default
behavior change on dump + new board command). Prefer a minor version bump with
explicit migration note: “task/need dump defaults to open.”

Publication (crates/Homebrew) only with operator approval after local
`cargo test` green.

## Exit Strategy

Decision: **included**

- New commands are additive; remove or feature-gate only if broken—prefer fix.
- Dump default reversion: users pass `--status all` or a temporary env/flag only
  if a real regression appears; do not keep dual defaults long-term.
- Watermark files are agent-owned paths; Vivi does not require them.
- If board query performance is poor on huge mailspaces, cap defaults and
  document `--since` rather than inventing a second store mid-goal.

## Acceptance Criteria

- An agent can answer “what should I do?” with one `board` or
  `task/need list --json` without a full dump.
- An agent can answer “what changed since T?” with board/brief `--since` /
  watermark without re-reading done history.
- `task dump` / `need dump` without flags does **not** print the full done
  archive by default.
- `mailspace status --json` exposes actionable open work separately from unread
  mail and wants.
- Long residual evidence can be sent via file or stdin without shell-escaping a
  multi-KB string (and `@path` remains documented).
- Obsolete wants can be closed without promoting them to needs.
- README shows list/board-first intake; no new docs teach dump as the default
  loop.
- **No** `gate` subcommand or binding stage license API ships under this goal.
- `cargo fmt --check`, `cargo test --test hygiene`, and `cargo test` pass.

## Validation

- Unit/integration tests with temp mailspaces for board, dump defaults, since
  filters, want close, body-file/stdin.
- Manual: init mailspace, seed task/need/want/done, compare dump vs board size,
  flip a task done and observe brief delta.
- Review: confirm help text and README do not introduce GO/NO-GO stage gates as
  product features.
- Hygiene ceilings still hold on touched modules.

## Open Questions

1. **Command name:** top-level `vivi board` vs `vivi mailspace board`?
   Recommendation: top-level `board` for agent discoverability; alias under
   mailspace OK.
2. **Want close verb:** `want done`, `want drop`, or `want archive`?
   Recommendation: `want done` (symmetry) with optional note; “drop” as alias.
3. **Watermark write-back:** should `board --watermark-file X` update X on
   success, or only read? Recommendation: `--write-watermark` opt-in.
4. **Dedupe warn on send:** Phase 4 or deferred? Recommendation: warn-only in
   Phase 4 if cheap; else defer.

Factory may pick recommendations when unanswered, and record the choice in the
phase delivery spec.

## Stop Conditions

- Stop if implementation would require a second coordination DB or gate table
  to meet acceptance (revisit architecture with operator).
- Stop if dump default change is rejected by operator—then implement board/
  brief first and leave dump default with a loud help note only.
- Stop before any network-facing or account-store changes.
- Stop if hygiene or full test suite cannot be made green without weakening
  policy.

## Factory Handoff

| Item | Value |
| --- | --- |
| Repo | `/Users/ianzepp/work/ianzepp/vivarium` |
| Goal artifact | `docs/mailspace-agent-control-plane-goal.md` |
| Feedback sources | Faberlang want `a32d0dd` (codex); strategist mail `0274350`; operator hunter/gatherer process notes |
| Suggested start | Phase 1 (dump safety + list JSON/columns) |
| Ready for | **factory** (vision → production → delivery → loop) |

## Handoff Readiness

**Ready for factory** — problem, architecture, non-goals (especially no gate
subsystem), first milestone, validation, and stop conditions are grounded enough
for factory `vision` admission. Open questions are bounded and have
recommendations.

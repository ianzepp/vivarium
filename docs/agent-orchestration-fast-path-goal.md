# Goal: Agent Orchestration Fast Path (Unified Work Graph + `vivi step`)

## Summary

Remove graph management as a separate manual step and remove the Mind-turn
bottleneck from multi-agent orchestration. Two coupled changes:

1. **Unified dependency substrate** — wants, needs, and tasks all become
   citizens of the executable work graph. Every want/need mints a graph node at
   creation; `--depends-on` becomes available on need/want send; task
   dependencies and graph edges stop being parallel systems. The graph becomes
   the single picture of "what can run now" across the whole backlog.
2. **`vivi step`** — a one-tick adjudication and routing command. Given a
   settled task attempt, it runs mechanical receipt checks, applies graph
   transitions, records an atomic decision record, and emits a **dispatch
   manifest + exception list** for the host. An optional, config-gated judgment
   provider (System One / Jev class) handles the semantic screens. Vivi never
   spawns processes and never admits work; it decides eligibility, records
   transitions, and hands the host a manifest.

## Problem

Live Tugboat operations on project mailspaces hit two structural costs:

- **Three dependency systems, manually synced.** `task send` has a structured
  `--depends-on` flag; `need send` / `want send` carry dependencies only as
  body prose (`precondition:` convention); work graphs are imported/edited as a
  separate artifact. Nothing compiles one into another, so "manage the graph"
  is a whole separate step agents skip — and `graph ready` answers only about
  the explicitly-imported subgraph, not the backlog. Cadence's `pull_forward`
  and `unlocked_want` lenses exist because the graph cannot answer them.
- **The Mind is a serial router.** A Tugboat Mind turn that ingests one
  completion, reconciles paper, files successor tasks, and spawns takes 30–120
  seconds. At 10 continuously running seats returning every ~2 minutes the
  expected completion rate (~5/min) exceeds what one-quantum-per-turn routing
  can drain (needs < ~12s per completion), so the Mind saturates indefinitely
  and the operator channel starves. Almost none of that quantum is judgment:
  receipt reconciliation, readiness, successor filing, and receipts are
  mechanical given an admitted graph; the genuine judgment (verdicts, forks,
  admission) already belongs to Auditor/Head/Mind seats elsewhere.

The fix is not a faster Mind; it is making the happy path not a Mind turn at
all, per the existing repo invariant: *Vivi decides which nodes are eligible.
The Mind dispatches. Fleet (`prepare --node` → claim → activate) proves
execution.*

## Goals

1. **Birth citizenship.** `need send` and `want send` mint a graph node
   automatically (open node; handle-stable identity; stub nodes carry label
   only — done_when not required until lowered). No separate import step.
2. **Dependency sugar.** `--depends-on <handle>` (repeatable) on `need send`
   and `want send`, creating edges to the dependency's node. Mirrors the
   existing task flag.
3. **One substrate.** Task `--depends-on` and graph edges unify: a task bound
   to a node makes its dependency an edge between nodes. `graph ready` and
   `board --graph` then answer across tasks, needs, and wants together.
4. **Lowering as expansion.** A need lowered into units expands into a unit
   subgraph (authored topology, `graph apply` path); the need's completion is
   the join of its unit nodes. Expansion happens before activation, per
   existing rewrite law.
5. **Mechanical unlock.** When a dependency completes, dependents become
   unlocked — a `graph ready` fact surfaced on the board. **Wants promote only
   when requested** (`want promote` stays the only promotion path; settled
   operator ruling 2026-09-20 — no automatic promotion, not even opt-in).
6. **`vivi step` (shadow → apply).** One command advancing the loop one tick:
   ingest a settled attempt → mechanical checks (receipt field presence,
   internal consistency, declared paths vs write scope from the task record)
   → transition (`graph complete` / hold) → atomic decision record → emit
   `dispatches` / `exceptions` / `decisions`. `--shadow` adjudicates and logs
   without applying.
7. **Judgment provider (optional, config-gated).** A provider interface for
   System One-class typed judgments (TypeSafe/Jev) used only by `step`:
   receipt-coverage screens, failure-smell screens, and prose-precondition
   resolution that *proposes* edges. Absent/offline/timed-out provider →
   mechanical-only adjudication; fuzzy checks downgrade to exceptions. Never a
   network call on any read path.
8. **Bounded decision vocabulary.** `step` may proceed, hold-with-reason, or
   file a debt handle. It may never admit units, waive audits, close goals,
   issue audit verdicts, or create novel topology (only traverse existing
   edges and pre-authored repair patterns). Everything else downgrades to the
   exception list — which is the Mind's slow-path queue.
9. **Docs + skill truth.** `skills/vivi/SKILL.md`, `AGENTS.md` graph section,
   and README updated to the unified model; graph management no longer taught
   as a separate step.

## Non-goals

- **No process spawning inside Vivi.** `step` emits a manifest; Fleet/the host
  executes. No Fleet roles in Vivi core (standing invariant).
- **No auto-promotion** of wants under any condition (settled this session).
- **No ambient conditions as topology.** "When a seat is free," "when urgency
  changes," time-of-day gates stay dispatch-time predicates evaluated by the
  host; they are never nodes or edges.
- **No verdict vocabulary in `step`.** `block_range` / `block_ship` /
  `admitted` remain Auditor outputs; `step`'s provider screens never emit them.
- **No second coordination database.** Extend the existing graph tables and
  mailspace store; no parallel dependency store.
- **No delivery-spec templating in v1.** Auto-filing successor task bodies
  from goal/delivery documents is deferred (needs a spec-parsing contract
  first).
- **No changes to email/IMAP/Proton paths.** Mailspace-local plus the one
  opt-in provider call in `step`.
- **No breaking handle stability** or existing folder roles.

## Ground Truth Researched

Verified against live CLI (`--help`) on 2026-09-20 at vivarium 8.1.0:

- `vivi task send` has `--depends-on <DEPENDS_ON>` (repeatable, task handle);
  `vivi need send` and `vivi want send` have **no** dependency flag — prose
  convention only.
- `vivi graph node add` and `vivi graph edge add` exist: incremental runtime
  topology, not just Mermaid import/apply.
- `vivi graph ready [--json]` reports the ready/blocked/active frontier;
  `vivi board --graph` projects frontier summaries without replacing board
  items.
- `vivi graph activate` binds a task attempt to a ready node; `complete` marks
  done and may unlock successors; active/completed prerequisites cannot be
  rewritten incompatibly (expansion-before-activation is already law).
- `vivi want promote` exists and is manual; want close/drop landed with the
  control-plane goal.
- `AGENTS.md` carries the Fleet invariant and names the graph tables
  (`work_graphs`, `work_graph_nodes`, `work_graph_edges`, revisions, events,
  attempts) in `mail.sqlite`; readiness is derived from normalized edges +
  node state.
- Adjacent but disjoint: `docs/agent-dispatch-delivery-plan.md` covers
  email-driven external agent dispatch (Hermes/trusted-sender). This goal is
  mailspace-graph orchestration; no overlap in scope.
- Precedent format: `docs/mailspace-agent-control-plane-goal.md` (landed).
- Session design record (2026-09-20): Tugboat + typesafe-ai skill session with
  operator; settled rulings captured under Goals 5–8 above.

## Reference Packet

| Path | Why |
| --- | --- |
| `src/mailspace/` | core mailspace types, send, list, dump |
| `src/local_work_command.rs` | task/need/want send dispatch (add `--depends-on`) |
| graph implementation (see `AGENTS.md` table; `work_graph_*` in `mail.sqlite`) | node/edge add, ready, activate, complete |
| `src/cli.rs` + `src/cli/mailspace_command/` | clap shapes; where `step` lands |
| `docs/mailspace-agent-control-plane-goal.md` | landed precedent for board/status shape |
| `docs/agent-dispatch-delivery-plan.md` | adjacent email-dispatch layer (do not merge) |
| `skills/vivi/SKILL.md` | agent-facing CLI law to update |
| `AGENTS.md` | Fleet invariant, validation, code standards |
| `tests/` | integration patterns for CLI behavior |

## Constraints And Invariants

- **Fleet invariant:** Vivi computes eligibility and records transitions; the
  host/Fleet spawns. No execution roles in core.
- **Vivi decides, code applies, provider suggests.** Judgment-provider outputs
  are advisory inputs to `step`'s bounded state machine; the provider never
  mutates state directly, and every provider-mediated decision is recorded
  (question id + version, answer, confidence) atomically with the transition
  it caused. A later reader reconstructs why a node unlocked from records
  alone.
- **Local-first preserved.** All read paths (`board`, `graph *`, `* list`)
  remain network-free and offline-safe. The only outbound call in this goal is
  the opt-in provider inside `step`, behind config, with timeout→downgrade.
- **Secrets:** provider key read via the configured secret mechanism
  (`password_cmd` precedent); never inline, never logged, never in decision
  records.
- **Atomicity:** a `step` transition is all-or-nothing — node completion,
  unlock effects, decision record, and manifest derive commit together or not
  at all.
- Handles stay stable across folder moves and graph attachment.
- Production errors in `VivariumError` / `thiserror`; clap derive; `tracing`;
  hygiene ceilings as enforced by `cargo test --test hygiene` (file/function
  size) — extract modules rather than growing a mega-command.
- Validation gate: `cargo fmt --check`, `cargo test --test hygiene`,
  `cargo test`.

## Architecture Direction

```text
                    ┌────────────────────────────────────────────┐
 want/need/task send├─ mint node (+ --depends-on edges)          │
                    │                                            │
 graph apply        ├── lowering: need node → unit subgraph      │
                    │                                            │
 vivi step          ├── adjudicate receipt → complete / hold     │
 (per settled       ├── atomic decision record (+ provider       │
  attempt or idle)  │   screens when configured)                 │
                    ├─ emit manifest: dispatches, exceptions     │
                    └────────────────────────────────────────────┘
                                        │
                     host / Fleet: prepare --node → claim → activate
                     Mind: consumes `exceptions` only (slow path)
```

- **Canonical store:** existing `work_graph_*` tables + mailspace messages.
  Backlog attachment is a mapping layer (handle ↔ node identity), not a new
  store.
- **`step` is a pure-ish state machine over store inputs:** mailspace-recorded
  evidence only in v1 (receipt fields, declared scope strings, done_when
  clauses). Git/filesystem verification of receipts stays host-side — Vivi
  does not reach into managed repos.
- **Provider interface:** small trait (typed question in → typed answer +
  confidence out), mockable, vendor-agnostic. TypeSafe/Jev is the first
  implementation, config-gated off by default.

## Supporting Skills

- `factory`: phased implementation against this goal.
- `delivery`: compile each phase into a delivery spec before coding.
- `vivi`: CLI behavior reference; update its skill as behavior lands.
- `typesafe-ai`: when implementing the provider (live docs are source of
  truth for the API contract; key handling law).
- `red-green` / `correctness`: behavior tests for unlock, adjudication,
  atomicity, no-network read paths.

## Implementation Shape

### Phase 1 — Backlog citizenship (smallest useful)

- Mint open nodes on `need send` / `want send`; handle↔node identity mapping.
- `--depends-on` on need/want send → edges.
- `board --graph` / `graph ready` surface unlocked wants/needs.
- Tests: birth minting, edge creation, unlock on dependency completion,
  promotion never fires implicitly.

### Phase 2 — One substrate

- Task `--depends-on` unified with graph edges via node binding.
- Lowering-as-expansion support: need node → unit subgraph through the
  `apply` path; join completion; expansion rejected after activation.
- Tests: cross-kind readiness (`graph ready` spans kinds), join semantics,
  rewrite rejection.

### Phase 3 — `vivi step --shadow`

- Mechanical adjudication state machine: field presence, scope consistency,
  done_when clause inventory; proceed/hold outputs; no mutation.
- Manifest + exception emission shapes (text and `--json`).
- Tests: fixture mailspace; shadow mutates nothing.

### Phase 4 — `step` apply + decision records

- Atomic transitions (complete/hold) with decision records; unlock effects.
- Exit codes / output contract stable for host consumption.
- Tests: atomicity on induced failure; record completeness; bounded
  vocabulary (no path exists to admission/close/verdict mutations).

### Phase 5 — Judgment provider (optional integration)

- Provider trait + config (off by default; secret via configured mechanism).
- Screens: receipt coverage per done_when clause; failure-smell; prose
  precondition → edge **proposal** (applied only by `step` rules with the
  resolution recorded).
- Shadow-compare mode: log provider decisions alongside mechanical outcomes to
  build a calibration corpus before any apply-by-default behavior.
- Absence/timeout/offline → mechanical-only; fuzzy checks downgrade to
  exceptions.
- Tests: mock provider (deterministic), timeout downgrade, no provider calls
  on read paths, key never in logs/records.

### Phase 6 — Docs, skills, release notes

- `skills/vivi/SKILL.md` graph section rewritten (no separate management
  step; dependency grammar: structural edges vs ambient predicates).
- `AGENTS.md` graph table gains `step`; README examples.
- Release notes: additive commands; behavior notes for board/ready output
  growth.

### Deferred (explicitly later)

- Auto-filing successor task bodies from delivery-spec node templates.
- Cadence catalog scoring as a read command over the unified graph.
- Host-side Fleet wrapper consuming the manifest (seats outside this repo).
- Ambient-predicate structured flags (`--when`).

## Release Posture

Decision: **release checkpoint** when Phases 1–2 land (graph semantics and
board/ready output change for existing users). Minor bump with notes:
"needs/wants now appear in work graphs; task dependencies are graph edges."
Provider phase ships dark (off by default) and needs no gating release.

## Exit Strategy

Decision: **included**

- Mint-on-send and `--depends-on` are additive; disable-by-config only if
  broken — prefer fix.
- If unified substrate is rejected in practice, graph import/apply continues
  to work unchanged; backlog attachment can be feature-flagged off.
- Provider remains permanently optional; `step` mechanical-only is a
  supported posture, not a degraded one.

## Acceptance Criteria

- `vivi need send --depends-on <handle>` creates the edge; completing the
  dependency flips the dependent to unlocked in `graph ready` / `board --graph`
  with no manual graph command.
- A want whose dependencies are all complete remains a want until someone runs
  `want promote` (tested).
- `vivi step --shadow` on a fixture mailspace produces adjudications and a
  manifest while mutating nothing; `vivi step` applies a transition and its
  decision record atomically (failure leaves no partial state).
- With no provider configured (or provider offline), `step` completes
  mechanical adjudication and downgrades every fuzzy check to an exception —
  no error, no hang, no network attempt on read paths.
- Every provider-mediated decision appears in a decision record with question
  id, answer, and confidence; reconstructing why a node unlocked requires only
  Vivi records.
- `step` has no code path that admits work, closes goals, waives audits,
  issues verdict vocabulary, or creates edges not pre-authorized by its rules.
- One substrate: no dependency information is representable in exactly one
  place (graph edges); task `--depends-on` is an edge, not a second mechanism.
- `cargo fmt --check`, `cargo test --test hygiene`, and `cargo test` pass.

## Validation

- Integration tests with temp mailspaces: minting, unlock, join completion,
  shadow/apply, atomicity, promotion-negative, no-network reads.
- Manual: seed a want depending on a need; complete the need; observe unlock
  and absence of promotion; run `want promote` explicitly; observe transition
  with event ledger note.
- Review: help text and skills doc teach structural-vs-ambient dependency
  grammar; nothing teaches auto-promotion or provider-mutates semantics.
- Calibration: collect shadow-corpus provider decisions before enabling any
  provider-informed apply.

## Open Questions

1. **Command name:** top-level `vivi step` vs `vivi graph step` vs
   `vivi cycle step`? Recommendation: top-level `step` (agent
   discoverability), graph-scoped alias acceptable.
2. **Node identity:** reuse the mailspace handle as the node source id, or a
   separate handle↔node mapping? Recommendation: separate stable mapping keyed
   by handle (graph ids already have their own stability rules).
3. **Where provider config lives:** `accounts.toml`-adjacent section vs
   dedicated config file. Recommendation: dedicated `[judgment]`-style section
   in the existing config mechanism with the secret via `password_cmd`
   pattern.
4. **Scope strings source of truth for `step` checks:** parse `write_scope:`
   from task bodies (convention) vs structured field on send. Recommendation:
   structured optional `--scope` flag on task send in this goal; body
   convention tolerated as fallback input.
5. **Shadow default:** should `step` refuse to apply until a first run has
   shadow-logged? Recommendation: no hard gate; `--shadow` documented as the
   onboarding path, apply is explicit.

Factory may pick recommendations when unanswered and record the choice in the
phase delivery spec.

## Stop Conditions

- Stop if unification requires breaking handle stability or a second
  coordination store (revisit architecture with operator).
- Stop if provider isolation cannot be guaranteed (any network reach from a
  read path, any secret in logs) — ship mechanical-only, move provider
  host-side.
- Stop if graph table migration cannot stay atomic across existing mailspaces.
- Stop if the operator rejects provider integration inside Vivi core —
  Phases 1–4 stand alone; Phase 5 moves out of repo.
- Stop if hygiene or full tests cannot stay green without weakening policy.

## Factory Handoff

| Item | Value |
| --- | --- |
| Repo | `/Users/ianzepp/work/ianzepp/vivarium` |
| Goal artifact | `docs/agent-orchestration-fast-path-goal.md` |
| Feedback sources | 2026-09-20 Tugboat/typesafe-ai design session (operator); live CLI verification same date |
| Suggested start | Phase 1 (backlog citizenship) |
| Ready for | **factory** (delivery compile → phased loop) |

## Handoff Readiness

**Ready for factory** — problem, settled rulings (promotion on request only;
bounded `step` vocabulary; provider advisory-only and config-gated; ambient
predicates never topology), architecture, phased shape, validation, and stop
conditions are grounded. Open questions are bounded with recommendations.
Phase 1 depends on no open question.

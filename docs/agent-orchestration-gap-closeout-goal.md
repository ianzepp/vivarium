# Goal: Agent Orchestration Gap Closeout (Dogfood Run)

## Summary

Close the process gaps found in the 2026-09-20 end-to-end review of the
revised Tugboat + Vivi process, and verify the process itself by running
this goal through a development build of `vivi` (backlog citizenship,
`need bind`, `graph activate`, `vivi step`, settle + apply, provider
screens). New gaps discovered during the run are appended to this document.
Exit condition: gap items below are closed, the dogfooded sequences are
recorded as verified, and the skill rewrite has an honest basis.

## Dogfood contract (what this run exercises)

- Work items are filed via the dev binary (`target/debug/vivi`), not chat.
- Dispatch sequence: `task send` → `graph activate --task` → implement →
  `task done` (settle) → `vivi step --apply <handle>` (screen + record).
- Lowering uses `need bind` in the backlog graph; imported Mermaid graphs
  are out of scope for delivery topology (the plan-graph ruling).
- `[judgment]` configured user-level with `key_cmd`; screens run shadow and
  the corpus accumulates.

## Gap items

### G1 — Migration defect blocks old mailspaces — CLOSED (1bf79f7)

`ensure_schema` now runs the idempotent DDL on upgrades; schema version 7
forces one repair pass. Verified live on this repo's own mailspace
(`vivi goal add` registered `gol_6fdd549276d30efd`). Regression test added.

### G2 — No in-flight marker (double-dispatch risk) — TEACHING, verified this run

A sent-but-being-worked task's node stays `open`+`ready`, so `vivi step`
re-offers it. Closing sequence taught and exercised here: dispatch =
`task send` → `graph activate --task <handle>` → spawn. Active nodes are
excluded from readiness.

### G3 — Provider screen unreachable in normal flow — CLOSED (code + teaching, 64b1796)

Dogfood found the teaching-only plan insufficient: the lifecycle hook
completes the node at settle, so `step --apply` screened nothing. Apply now
screens every settled item regardless of which path completed the node.
Verified live: corpus records with real `jev-latest` answers after the
config fix (see finding F3).

### G4 — Plan-graph vs backlog duality — RULING, recorded here

Delivery lowering uses `need bind` in the backlog graph only. Imported
Mermaid graphs are for non-item topology. No code.

### G5 — Reopen does not cascade the join — CLOSED (e11c265)

Reopening a bound unit now re-opens its done parent (visited-set recursion
guards hypothetical bind cycles); re-completing the unit closes the join
again. Regression tested. Dogfooded as task 557ac8f8 under need 885652cd.

### G6 — Multi-recipient minting splits identity — CLOSED (64b1796)

Dependency edges canonicalize the cited copy to one deterministic work-role
copy per content (lowest message id; `sent` copies excluded); completing any
copy completes its content siblings, and reopen propagates the same way.
Dogfooded as tasks 265e176c + 9442a6db under need 7143887e; the join
closed the need live when both units settled.

### G7 — `task list --blocked` reads headers, not edges — DEFERRED (want)

Same input today, so no lie; migrate opportunistically.

## New gaps found during this run

- **F1** — `graph activate` failure prints "complete requires
  graph:source-id": wrong verb in the shared argument-error text. Cosmetic;
  fix with the next graph CLI touch.
- **F2** — Backlog node addressing requires the `backlog:<handle>` form for
  `graph activate`/`complete`. Since backlog source ids *are* handles, a
  bare-handle convenience (default graph = backlog) would remove friction
  from the taught dispatch sequence. Candidate small enhancement.
- **F3** — User-level config resolution: the live config dir on this host is
  `~/.config/vivarium` (legacy path), not `~/.vivarium`. The skill rewrite
  must teach the real resolution (`VIVI_HOME` env, legacy detection) or
  judgment config silently no-ops. Discovered by an empty corpus.
- **F4** — Calibration observation, not a defect: Jev's first live screen
  scored receipt coverage low (noul 0.31) because the settled item's state
  carries verdict/repo/tip metadata but not the validation output. The
  corpus is accumulating exactly the signal a future gating ruling needs;
  if receipts should carry a validation-claim line for screens, that is a
  body-convention teaching, not code.

## Verification log

- 2026-09-20: migration repair verified live on this repo's mailspace
  (goal registration succeeded after upgrade).
- 2026-09-20: dogfood board run — needs 885652cd + 7143887e filed, lowered,
  bound; units dispatched (send → activate), implemented, settled
  (`task done` with verdict/repo/tip), applied (`step --apply`); both
  parents auto-completed through the join; `graph ready` drained to empty;
  manifest excluded active nodes and flagged lowered parents throughout.
  Two real provider screens in `.vivi/judgment-corpus.jsonl`.

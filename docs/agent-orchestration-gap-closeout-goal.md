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

### G3 — Provider screen unreachable in normal flow — TEACHING, verified this run

Screens only fire on `step --apply`. Closing sequence taught and exercised:
on settle, run `vivi step --apply <handle>` (idempotent; triggers the
screen and corpus append).

### G4 — Plan-graph vs backlog duality — RULING, recorded here

Delivery lowering uses `need bind` in the backlog graph only. Imported
Mermaid graphs are for non-item topology. No code.

### G5 — Reopen does not cascade the join — OPEN (task filed)

A unit reopened after its parent need auto-completed leaves the parent
`done` (stale join). Fix: reopening a bound unit whose parent is done
re-opens the parent.

### G6 — Multi-recipient minting splits identity — OPEN (task filed)

A need sent to N identities mints N nodes; a dependent citing copy A never
unlocks when copy B completes. Fix: (a) dependency edges canonicalize the
cited handle to one copy per content; (b) completing any copy completes
all sibling nodes of the same content.

### G7 — `task list --blocked` reads headers, not edges — DEFERRED (want)

Same input today, so no lie; migrate opportunistically.

## New gaps found during this run

(appended as discovered; none yet)

## Verification log

- 2026-09-20: migration repair verified live on this repo's mailspace
  (goal registration succeeded after upgrade).

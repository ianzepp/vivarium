# Vivarium 9.2.0

Vivarium 9.2.0 answers the second v9 field feedback batch — the 2026-09-20
faberlang defect handoff (~270 work-kind sends in one day: a 252-item bulk
filing loop plus ~20 role-addressed task sends). The theme: backlog
citizenship is now enforced and repairable end-to-end, joins settle real
mailbox state, and the operational surfaces (audit, frontier, apply) say
what they mean.

## Backlog citizenship audit (V-1)

`vivi graph audit [--repair]` checks the backlog invariant — every work
item sent while the graph existed has a node in step with its folder
state — and reports `missing_node`, `node_state`, `node_kind`, and
`orphan_node` findings. `--repair` mints missing nodes from folder state
and dependency headers, completes nodes for settled items, settles need
mailboxes whose join fired, and corrects node kinds minted before kinds
were persisted. `graph activate` and `need bind` errors point at the
repair path when a handle resolves to a work message with no node.

The triggering field defect turned out to be a stale Homebrew vivi 8.1.0
on PATH: it writes byte-identical send events but never minted nodes, so
three same-second task sends looked like a minting race. `vivi mailspace
status` now prints the running binary's version so stale installs are
visible. Keep one vivi on PATH.

- Node minting persists `kind` (needs and wants are no longer `task`
  nodes); audits repair pre-9.2 boards.
- Node lookups (complete, bind, connect, reopen, join) tolerate handle
  drift — shortest-prefix handles grow when later sends collide — by
  matching node source ids as message-basis prefixes.

## Joins settle the need itself (V-2)

The `need bind` join completed the need's graph node within a second of
the last unit settling, but the need's mailbox item stayed in `needs` —
open on every board and list — until someone ran a manual `need done`.
The join now settles every content-sibling copy of the need
(`need done` / `via=join` in the event log), reopening a bound unit
restores the need to open the same way, `need bind` refuses non-need
parents, and `vivi need show` prints the bound units with their states.

## Dispatch ergonomics (V-3, V-4, V-5)

- Cross-identity lifecycle moves fail with the holder named:
  `message '<handle>' is held by identity '<holder>'; retry with --for
  <holder>` — the sender's receipt handle names the recipient's copy.
- `vivi graph connect <dependent> <prereq>` adds a prerequisite edge
  discovered after filing (idempotent; refuses self edges, active/done
  dependents, and handles without nodes) — no more drop-and-refile.
- Duplicate work sends warn on stderr (never blocking) when the same
  sender has a still-open item with the same subject in the recipient's
  folder — the signature of a re-run filing loop.

## Legibility (V-6)

- `vivi graph ready` prints a `counts` line before the id lists and
  gains `--kind task|need|want|decision|stub|parked` for bounded reads.
- `vivi step --apply <handle>` on an item the lifecycle already settled
  records `via=lifecycle (already settled; nothing to apply)` under
  `decisions` instead of silence, and surfaces untracked items as
  `untracked_item` exceptions pointing at `graph audit --repair`.
- `vivi trace` remains an audit-grade read (tens of seconds on large
  boards); it is not a status-loop tool.

## Performance

Large-board reads stopped rebuilding the short-handle map per item:
`source_kind` reuses the caller's storage handle (a full audit on the
119 MB / 72k-message faberlang board went from 97s to 8s), and
`messages.content_id` gains an index for sibling and canonical-copy
lookups. Schema is unchanged (index only, `IF NOT EXISTS`).

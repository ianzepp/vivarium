# Vivarium 9.1.0

Vivarium 9.1.0 answers the first field feedback from v9 graph coordination
(the mintedgeek VFS epic #35 campaign run, ~40 dispatched seats, two imported
work graphs). The theme: the graph's planning vocabulary is now executable —
decisions, stubs, and parked nodes are first-class gates that can never be
dispatched as work, dotted couplings never block readiness, and the Mermaid
import subset is documented where failures happen.

## Operator gates (node kinds — schema 8)

Imported nodes carry kinds. Rhombus `id{label}` imports as `decision`; an
`id:::kind` suffix or a `class <ids> <kind>` statement marks `decision`,
`stub`, or `parked`; stadium `id([label])` and rect nodes are ordinary
dispatchable work; `graph node add` gains `--kind`.

- `graph ready` partitions gated ready nodes into a `gates` line/field
  (with kind) instead of listing them as ready work; import and apply
  receipts carry the same split.
- `graph activate` refuses gated kinds with a pointer to
  `graph complete --note` — an operator ruling is recorded, never
  dispatched. Completing a gate stays the resolution path.
- Storage: `work_graph_nodes.kind` (default `task`) and
  `work_graph_edges.style` (default `solid`) via the schema-8 idempotent
  repair pass; existing mailspaces migrate on first open, no data changes.

## Non-gating couplings (dotted edges)

`-.->` and `-.-` import as `dotted` edges: topology evidence that never
gates readiness — only `-->` is a prerequisite arrow. Export round-trips
dotted edges, and `graph apply` treats a solid↔dotted flip as an edge
revision (subject to the same frozen-prerequisite rules).

## Mermaid subset: widened and documented

Stadium nodes, rhombus decisions, dotted edges, `id:::kind` suffixes,
`class` statements, and `classDef`/`style` tolerance (styling lines are
ignored) all import now. Parse errors name the offending text and print the
accepted subset, and `vivi graph import --help` documents the subset in
full. Export re-imports cleanly with kinds and couplings intact.

## Step legibility

- `no_done_when` exceptions list the labeled fields the body does carry
  (`verdict`, `repo`, …), so coordination tasks read as coordination, not
  as defects.
- Want exceptions render as parked: "parked: wants never dispatch before
  explicit promotion" (plus the missing-clause fact when also clauseless).
  The reason vocabulary is unchanged.
- `task send` / `need send` warn on stderr when the body declares no
  `done_when:` clause — the author learns at filing time, `--json` stdout
  stays clean, and wants stay quiet (parked by design).

## No more own-inbox lifecycle twins

`task done --note` (and every lifecycle note) no longer delivers a `Re:`
twin into the acting identity's own inbox for self-addressed items — the
normal shape of wants. Other participants still receive their receipt; the
actor keeps the read `sent` copy as the thread record.

## Smaller

- `graph complete` accepts `--task` and ignores it with a stderr hint
  ("task binding happens at `graph activate`") instead of a generic usage
  error.
- `vivi step` adjudicates only the `backlog` graph; imported topologies
  never enter the manifest — now stated explicitly in README, the skill,
  and `graph import --help`.

## Records

- Feedback source: `factory/notes/2026-09-20-vivi9-graph-coordination-feedback.md`
  (mintedgeek workspace)
- Issues log: `issues.md` entries dated 2026-09-20 (all six items)

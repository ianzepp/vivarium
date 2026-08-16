# Goal: Project mailspace filter and search (not dump, not DSL)

Status: **partial** — `mail`/`task`/`need`/`want` list accept `--from` /
`--to` with optional `--for`; `task`/`need` list accept `--status all`;
`want dump` uses the work-dump flags. Still open: hard `--limit`, verdict
filter, project search, dump exit.

## Summary

Give agents a **small, closed** query surface over project-local mail
(`.vivi/mail.sqlite`) so cross-cutting process-health work—especially audit
verdict lifecycle checks—does not require per-identity dumps, Python over JSON,
or IMAP `vivi search`. Vivi stays a mail/work tool with **filter** (structured
columns) and **search** (text), hard output budgets, and no general query
language. Deep analytics belong later in Swarm’s database, not in a Vivi DSL.

## Problem

A real meta-analysis session (audit residual / `block_ship` lifecycle →
follow-up tasks → completion evidence for regression tests) showed the mail
**delivery** path is fine and the **read/query** path is not.

Observed methodology (six steps):

1. Enumerate mail by identity (auditor-1..4 sent, mind inbox/sent) over ~10 days  
2. Keep messages whose subject/labels indicated residual or block_ship  
3. Read full YAML audit bodies for findings and severity  
4. Find mind follow-ups by subject text or date proximity  
5. Read task bodies for “tests required?”  
6. Cross-reference completion mail for whether tests landed  

That work needed stitching **at least three identities** per finding and ~40
tool calls across sessions. Pain mapped to product gaps:

| Gap | Detail |
| --- | --- |
| No structured filter | Cannot ask “all residual / block_ship in 10 days, any identity” |
| Identity-scoped list | `vivi mail list` requires `--for`; analysis is cross-identity |
| Dump is the wrong tool | Match-then-render bulk export; >25 records or 64 KiB needs `--confirm-large`; agents treat refusal as “no results” |
| `vivi search` is account mail | Indexes IMAP/Proton path, not project mailspace |
| Lifecycle joins are manual | Report → task → completion joined by subject/date, not explicit edges |
| Trace cost / trust | ~22s per call; inferred edges need manual verification |
| No aggregates | Counts by auditor / verdict / follow-up outcome hand-tallied |

Related product judgment from goal discussion (not inventable later):

- **Filter ≠ search.** Filter = list with closed column predicates. Search =
  free text over subject/body (optionally scoped by the same closed filters).
- **Do not grow dump.** On large mailspaces, agents need ranked/bounded
  results, not export-the-world + confirm gates.
- **No Vivi DSL.** Swarm has a real database for deep joins and analytics.
  Vivi may use SQLite for **product fields** exposed as **clap flags**, not
  expression languages, YAML path queries, or SQL-over-mail.

## Goals

1. **Filter (list class):** Cross-identity project-mail listing with a **closed**
   set of column filters and a **hard default `--limit`**. No free-text query
   required. Empty result means zero matches, never a large-output refusal for
   normal use.
2. **Search (text class):** Project-mailspace text search (subject/body) with
   the same closed filters as **scope**, hard `--limit`, and hit-shaped output
   (handle, subject, snippet/score)—not full-body dump of every match.
3. **Structured product fields:** Lift fields needed for process-health
   filtering (at least **verdict**: residual / block_ship / clean_pass / …)
   into stored columns or events at write/close time so filters map 1:1 to
   storage. Subject-string hacks are not the long-term API.
4. **Agent-safe output:** Default caps; print match counts and “N more;
   tighten filters” when truncated. No `--confirm-large` as the primary path
   for analysis.
5. **Demote or remove dump:** `mail dump` / `task dump` are not the analysis
   surface. Prefer delete or hide behind an explicit export path
   (`--output` / admin) once list+search cover the jobs dump was faking.
6. **Strict lifecycle posture (supporting, not the whole goal):** Trace (or
   equivalent) must be usable for “report → follow-up → completion” without
   treating inferred edges as truth—e.g. default or flag for **explicit links
   only**—and without full-table `list_messages()` cost on large spaces when a
   seed-handle walk is enough.
7. **Docs for agents:** README / skill-facing notes state filter vs search,
   project vs account search, and that process-health analytics beyond closed
   filters belong in Swarm.

## Non-goals

- **No query DSL:** no expression language, no `WHERE`-style strings, no
  YAML/JSON path predicates, no arbitrary group-by language in clap.
- **No Swarm replacement:** no warehouse, dashboard, or multi-fleet BI in Vivi.
- **No improving dump as the main answer:** do not invest in smarter dump
  confirm UX as the analysis product.
- **No inventing structure at read time only:** do not pretend arbitrary body
  YAML is queryable without lifting known fields at write/close (or a
  deliberate, versioned extractor into columns).
- **No required rewrite of historical mail** beyond best-effort backfill of
  lifted fields where cheap and honest.
- **No change to account IMAP/Proton `vivi search`** beyond clarifying that it
  is not project mailspace search (unless a tiny routing flag is free and
  non-ambiguous—default is separate project surface).
- **No full process-health mega-command in v1** (`vivi analytics` with
  pre-baked “finding lifecycle report”) unless it is thin sugar over
  filter + explicit-trace + count—and only after primitives exist.
- **No Fleet role logic inside Vivi core** (existing project invariant).

## Ground Truth Researched

- Conversation (this session): audit verdict lifecycle analysis narrative;
  dump agent failure mode; filter vs search split; Swarm migration → no DSL.
- `src/cli.rs` / `src/cli/mailspace_command.rs`: `mail list` requires
  `--for`; `mail dump` / `task dump` have optional `--for` plus subject/body/
  time filters; `task done --verdict`; top-level `search` is account index.
- `src/local_mailspace_dump.rs`: refuses stdout over **25 records** or **64
  KiB** without `--confirm-large` or `--output`.
- `src/mailspace/dump.rs`: `DumpFilters` / `DumpRecord`; body substring filter
  exists; no verdict field filter.
- `src/mailspace/delivery.rs`: verdict stored in task-close **metadata** on
  done, not as a first-class list filter.
- `src/mailspace/trace.rs`: `storage.list_messages()` then graph +
  `add_inferred_edges`; edge sources include `inferred`.
- `src/mailspace/thread.rs`: inferred links for historical best-effort;
  reply-threading goal already established explicit vs inferred distinction.
- `src/email_index/`: FTS5 for **account** email search—pattern for project
  FTS if chosen, not a shared index today.
- Related goals: `docs/mailspace-reply-threading-goal.md` (explicit parent
  links); `docs/factory/vivi-trace-goal.md` (trace + inferred edges).

## Reference Packet

Before implementing, inspect:

- `src/cli/mailspace_command.rs` + `work_command.rs`: list/dump/task CLI shape  
- `src/local_mail_list.rs`, `src/local_mailspace_dump.rs`, `src/stdout_budget.rs`  
- `src/mailspace/dump.rs`, `delivery.rs`, `trace.rs`, `thread.rs`  
- `src/email_index/search.rs` + schema FTS (pattern only)  
- `docs/mailspace-reply-threading-goal.md`, `docs/factory/vivi-trace-goal.md`  
- Fleet / vivi skill docs if agent-facing command names change  

## Constraints And Invariants

| # | Invariant |
| --- | --- |
| 1 | **Filter flags map 1:1 to stored columns or events.** If it is not stored, it is not a filter. |
| 2 | **Search is free text** over subject/body (and documented fields only); filters on search are scope, not a second language. |
| 3 | **Hard default limits** on list/search stdout; unbounded full-set export is explicit and rare. |
| 4 | **Empty = zero matches.** Agent-normal paths must not fail with “refusing large stdout” for ordinary analysis. |
| 5 | **Project mailspace ≠ account mail.** Naming and help text must not send agents to IMAP search for `.vivi` work. |
| 6 | **Explicit edges beat inference** for process-health. Inferred edges labeled; strict mode available. |
| 7 | **Clean break over dump compatibility** when list/search replace dump’s analysis role (repo change stance). |
| 8 | **No Vivi DSL** competing with Swarm; closed clap surface only. |
| 9 | Production errors stay in `VivariumError`; clap derive; hygiene ceilings (file/function size). |

## Architecture Direction

```text
Write path                          Read path
─────────                          ─────────
deliver / reply / task done   →    list  (filter: closed columns, hard limit)
  lift product fields              search (text + optional scope filters)
  (verdict, parent handle, …)      show   (one/few handles, full body)
  record explicit links            trace  (seed BFS; strict = explicit only)
        ↓
  SQLite columns / events / links
        ↓
  [later] sync/export events → Swarm DB for deep analytics
```

- **Canonical ownership:** project mailspace storage + local CLI dispatch
  (`mailspace/*`, `local_mailspace_command`, list/search modules)—not
  `email_index` account path.
- **Verdict:** product field on the kinds that carry it (task done metadata
  today; audit mail if writers put it in YAML—lift at ingest/done, do not
  query arbitrary YAML).
- **Dump:** retire as analysis UI; optional file export only if still needed
  for humans/offline.
- **Stats:** optional later thin counts over the same closed filters; not a
  separate language.

## Supporting Skills

- `delivery` / `factory`: lower this goal into specs and implement  
- `clean-break`: remove dump surface when replaced  
- `vivi` skill: update agent-facing command guidance after ship  
- `zombie-docs`: keep README/command claims honest  
- `goal-check`: optional readiness pass before factory vision  

## Implementation Shape

Rough slices only (not a delivery plan):

| Slice | Intent |
| --- | --- |
| **1 — Filter first** | Cross-identity `mail list` / `task list` with closed filters including time window; hard `--limit`; verdict filter where stored; agent-safe empty/truncated semantics |
| **2 — Lift + backfill** | Ensure verdict (and any other agreed product fields) are queryable columns/events; document write path for auditors/mind |
| **3 — Project search** | `mail search` (and/or task) over project bodies/subjects; scope filters; hard limit; never IMAP by default |
| **4 — Dump exit** | Remove or demote dump; migrate tests/docs |
| **5 — Trace hygiene** | Strict/explicit-only mode; avoid full-table load for seed walks where possible |
| **Later / Swarm** | Aggregates, finding lifecycle sugar, warehouse analytics |

**First useful milestone:** an agent can run one bounded list/filter command
and get all residual (or block_ship) items in a time window **across
identities**, with handles to `show`/`trace`—without dump or Python.

## Release Posture

Decision: **release prep when the CLI surface ships** (user-visible command
change). Version bump + README when list/search (and dump removal) land;
coordinate with normal vivarium release notes.

## Exit Strategy

Decision: **included**

- New list/search flags are additive until dump is removed.  
- Dump removal is a clean break: document migration (“use list/search”) in
  release notes; do not keep dump forever as a compatibility layer.  
- If Swarm absorbs process-health analytics later, Vivi keeps only the closed
  agent surface; no stranded DSL to delete.  
- If lifted fields prove wrong, stop adding filters; fix write path rather
  than inventing body-query escapes.

## Acceptance Criteria

- [ ] Cross-identity filter list works without required `--for` for the
      analysis cases (or an equally clear cross-identity list command).  
- [ ] At least **verdict** is filterable where the product stores it; documented
      write path so auditors/mind populate it.  
- [ ] Project mail **search** exists and does not hit the account IMAP index by
      default.  
- [ ] Default output is **bounded**; ordinary agent use never depends on
      `--confirm-large`.  
- [ ] Dump is removed or clearly demoted so agents are not steered to it for
      analysis.  
- [ ] Filter vs search is documented for operators and agent skills.  
- [ ] No general-purpose query/expression language ships under this goal.  
- [ ] Tests cover filter, search, limits, and empty-result behavior.  

## Validation

- `cargo test` (narrow tests for list/search/filter first, then suite).  
- `cargo test --test hygiene`.  
- Manual agent-shaped flows:  
  - residual/block_ship in last Nd across identities → handles only  
  - search keyword in project mail with `--limit`  
  - confirm account `vivi search` still separate  
  - dump gone or refuses to be the happy path  
- Review: clap surface remains a closed flag set; no DSL creep in help text.

## Open Questions

1. **Command names:** extend `mail list` / add `mail search`, or introduce a
   thin `vivi find` that only wraps the same closed flags? (Preference from
   discussion: extend list + dedicated search verb.)  
2. **Verdict on audit *mail* vs task done only:** do audit reports write
   verdict as lifted metadata at send time, or only tasks on `done`? Analysis
   needed both.  
3. **FTS vs substring** for project search v1? Substring may be enough for
   first milestone; FTS if corpus is large.  
4. **Dump:** hard delete vs `export --output` only?  
5. **Trace perf/strict:** same delivery unit as filter/search, or a follow-on
   goal that depends on reply-threading quality?  
6. **Swarm sync:** out of scope here, or name a future export event shape only?

## Stop Conditions

- Stop if implementation grows an expression language, SQL passthrough, or
  “query DSL” under another name.  
- Stop if dump is “improved” instead of replaced for analysis.  
- Stop if project search is implemented by silently calling account/IMAP
  search.  
- Stop if filters are added for fields that are not stored (YAML grepping
  dressed as columns).  
- Stop if scope expands into full Swarm-class analytics or multi-fleet BI
  without an explicit new goal.  

## Handoff

| Label | Meaning |
| --- | --- |
| **Ready for delivery** | One focused goal; first milestone (filter + limits + verdict lift) is delivery-sized; search/dump-exit/trace can be later stages or follow-on specs |

Downstream: `delivery` (or `factory` with vision) for slice 1; optional
`goal-check` before multi-phase execution.

**Not ready for campaign** unless dump-exit, FTS, trace perf, and Swarm export
are forced into parallel tracks with separate owners—default is one goal with
ordered slices.

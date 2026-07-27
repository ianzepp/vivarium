# Goal: Linear integration for project mailspaces

## Summary

Pilot Linear as a planning and visibility projection for Vivi project mailspaces.
Vivi remains the local execution and agent coordination system. Linear receives
selected campaigns, work items, and checkpoint receipts so humans can view,
search, and manage higher-level work without replacing the local agent bus.

Linear pilot project: [Vivarium: Linear Integration](https://linear.app/ianzepp/project/vivarium-linear-integration-b799697c70ce)

## Starting invariant

**Vivi owns local work truth. Linear is a one-way projection until a later
explicit design adds inbound sync.**

During the pilot, Vivi owns titles, bodies, kind, status, lifecycle events,
receipts, role context, and graph readiness. Linear may hold mirrored records,
operator-facing summaries, and durable links back to local artifacts. Linear
edits do not mutate Vivi records unless a later phase deliberately implements an
inbound path with conflict rules.

## Problem

Vivi has a strong local work model for agents:

- `task` is concrete assigned work with a done condition.
- `need` is missing input or a decision that can change scope.
- `want` is non-blocking polish, product feedback, or a later idea.
- `mail` is deliberation, status, and handoff.
- `memo` is private role memory.
- `graph` is executable topology and readiness over task attempts.
- `board` is the compact local view of actionable work.

Linear has a strong shared planning model for humans and cross-project history:
projects, issues, comments, milestones, project updates, labels, teams, and
workspace search.

The useful integration is not to make every agent handoff a Linear event. The
useful integration is to publish the durable subset of Vivi work into Linear
while preserving Vivi's local semantics and low-noise execution loops.

## Current ground truth

| Evidence | Fact |
| --- | --- |
| `AGENTS.md` | Project mailspaces contain tasks, needs, wants, mail, memos, roles, and executable work graphs. |
| `README.md` | `vivi board` is the current compact local work surface. |
| `vivi 7.1.0 --help` | `board` supports `--project`, `--json`, `--process`, `--graph`, `--since`, and watermark files. |
| `vivi task done --help` | task completion can record `--note`, `--verdict`, `--repo`, and `--tip` receipts. |
| Linear CLI | Workspace `ianzepp`, team `IAN`, and project `Vivarium: Linear Integration` exist. |

## Canonical mapping

| Vivi / local concept | Linear target | Pilot rule |
| --- | --- | --- |
| Workspace / project root | Linear team plus optional project label | `~/work/ianzepp/vivarium` maps to team `IAN` for the first pilot. Do not infer teams from arbitrary path owners until configured. |
| Campaign document | Linear project | One campaign maps to one Linear project when the campaign coordinates multi-stage work. The local document remains the detailed routing authority. |
| Campaign stage | Linear project milestone or parent issue | Use a milestone when the stage is a visible project checkpoint. Use a parent issue when it is a bounded delivery-sized unit. Do not create milestones for every factory phase by default. |
| Delivery spec | Linked local artifact in issue description or comment | Keep the durable spec in `docs/` or `docs/factory/`. Put a concise summary and local path in Linear. |
| Factory phase | Linear issue | A factory phase maps well to one issue when it has a clear completion gate and validation evidence. |
| Implementation stage inside a delivery spec | Linear sub-issue only when independently schedulable | Most implementation stages stay inside the delivery spec. Create Linear sub-issues only for real ownership, dependency, validation, or migration boundaries. |
| `vivi task` | Linear issue | Create or update an issue with kind label `vivi-task`. Task completion syncs status and a concise receipt. |
| `vivi need` | Linear issue | Create or update an issue with kind label `vivi-need`. Mark as blocking/decision input through labels or issue state. |
| `vivi want` | Linear issue in backlog | Create or update an issue with kind label `vivi-want`. Keep in backlog unless promoted locally or selected for planning. |
| `vivi mail` | Linear comment only when attached to durable work | Do not mirror all mail. Promote only decision summaries, status receipts, or handoffs that explain a synced issue. |
| `vivi memo` | Local-only | Do not sync by default. Memos are private role memory, not stakeholder-visible work. |
| `vivi role` | Local-only initially | Roles guide agent capacity and ownership locally. Linear assignees may be set for human ownership, not for every agent seat. |
| `vivi graph` | Linear dependencies only after the task mapping exists | Keep graph readiness local. Later sync may project selected edges into Linear dependencies, but Linear must not become the graph scheduler in the pilot. |
| `vivi board` | Sync input/read model | Dry-run and sync should use `vivi board --json --graph` plus targeted `show` commands where possible, not direct `.vivi` storage reads. |
| Factory closeout | Linear issue comment or project update | Post concise evidence: commits, tests, verdict, deferred work, and local artifact paths. Avoid large delivery-spec pastes. |

## Labels and states

Use labels to preserve Vivi kind without overloading Linear state names.

| Label | Meaning |
| --- | --- |
| `vivi-task` | Mirrored concrete work item. |
| `vivi-need` | Mirrored missing input, decision, or blocker. |
| `vivi-want` | Mirrored backlog idea or non-blocking polish. |
| `vivi-receipt` | Issue/comment includes execution evidence from a factory or campaign checkpoint. |
| `vivi-migration` | Record was imported from historical local mailspace state. |
| `vivi-sync` | Record is managed by the Vivi-to-Linear sync path. |

Pilot state mapping:

| Vivi state | Linear state posture |
| --- | --- |
| Open task/need selected for work | Active or unstarted issue, depending on local lifecycle. |
| Open want | Backlog issue. |
| Done task/need/want | Completed issue or comment receipt, depending on whether the issue was created before completion. |
| Reopened local item | Reopen the bound Linear issue and add a comment with the local event handle. |
| Local graph node active/done | Comment receipt on the bound task issue; do not drive Linear state directly from graph state until dependency sync is designed. |

Exact Linear workflow-state names are workspace configuration, not a hardcoded
Vivi rule. The sync code should resolve state names/types from Linear at runtime
or accept explicit config.

## One-way pilot conflict rule

Vivi wins conflicts.

- If a Vivi item is bound to a Linear issue, Vivi title/body/kind/status are the
  source of truth during sync.
- If a Linear title, description, label, or state differs from Vivi, the dry-run
  reports the difference before write mode changes it.
- Linear comments written by humans are preserved and are not imported into Vivi
  during the one-way pilot.
- New unbound Linear issues do not become Vivi tasks until a later inbound sync
  design exists.
- Deleted or archived Linear records are treated as conflicts, not silent local
  deletions.

## Binding and provenance requirements

Every synced Linear record must be idempotent. The binding store should record:

- Linear workspace slug
- Linear team key
- Linear project ID or slug
- Linear issue ID and issue identifier, when applicable
- Vivi project root
- Vivi kind: `task`, `need`, `want`, `mail`, `memo`, `graph`, or `campaign`
- Vivi handle or stable local artifact path
- Last synced local event timestamp or content hash
- Last synced Linear updated timestamp, used only for conflict reporting

Every Linear issue created from Vivi should include provenance in a small footer
or first comment:

```text
Source: Vivi project mailspace
Project root: <configured project key or path alias>
Kind: task|need|want
Vivi handle: <handle>
Sync mode: one-way Vivi -> Linear
```

Do not store Linear API tokens or credentials in the repo or binding file.

## What remains local-only

Keep these local unless a human explicitly promotes them:

- Private `memo` records.
- Raw `.vivi/` storage and blobs.
- Noisy agent handoff mail.
- Failed or abandoned exploratory attempts.
- Full delivery specs when a concise Linear summary and local path are enough.
- Role charters, model choices, process liveness, and PTY state.
- Graph scheduling authority and ready-frontier decisions.

## Historical migration posture

Migrate the durable history, not the whole coordination stream.

Include by default:

- Active campaigns.
- Open tasks.
- Unresolved needs.
- Accepted or promoted wants.
- Completed factory phases with useful receipts.
- Closeout evidence that explains project history.

Skip by default:

- Private memos.
- Transient mail chatter.
- Duplicate or superseded wants.
- Abandoned experiments with no current decision value.
- Large raw bodies that would make Linear noisy.

Historical migration must start with a dry-run report. The report should show
would-create, would-update, would-skip, and ambiguous records before any Linear
write.

## Agent permission model

| Actor | Linear permission posture |
| --- | --- |
| Operator | Full approved access. |
| Mind / Head roles | Read Linear and prepare proposed writes. Direct writes only when the campaign or operator grants that role ownership. |
| Hand roles | No direct Linear writes by default. Report through Vivi with evidence. |
| Auditor roles | Read-only by default. |
| Sync service / CLI command | The only default writer during the pilot. It writes deterministic projections from Vivi. |

This keeps Linear low-noise and prevents every agent handoff from becoming a
shared external notification.

## Implementation path

| Stage | Lowers to | Goal |
| --- | --- | --- |
| 1. Mapping artifact | Done in this document | Establish canonical terms, local-only records, and conflict rule. |
| 2. Dry-run exporter | Delivery / factory | Produce a no-write report for one explicit project root. |
| 3. Binding store | Delivery / factory | Make sync idempotent and conflict-aware. |
| 4. One-way sync | Delivery / factory | Create/update Linear project issues from selected Vivi tasks, needs, and wants. |
| 5. Checkpoint receipts | Delivery / factory | Post concise factory/campaign receipts to bound issues or project updates. |
| 6. Historical migration policy | Delivery / factory | Add include/skip rules and dry-run review gates for older mailspaces. |
| 7. Optional inbound sync | Future design | Only after one-way projection proves useful. |

## Dry-run exporter implications

The next slice should start read-only and should prove the projection shape
before adding any Linear write path.

Use these public Vivi surfaces first:

- `vivi mailspace status --project <root> --json` for mailspace existence and
  aggregate counts.
- `vivi board --project <root> --json --graph` for the current open work board
  and graph frontier.
- `vivi task list --project <root> --for <role> --json` for open and done task
  handles by identity.
- `vivi need list --project <root> --for <role> --json` for open and done need
  handles by identity.
- `vivi want list --project <root> --for <role> --status all --json` for open
  and closed wants.
- `vivi task|need|want show --project <root> <handle> --json` only for records
  selected for projection.

This avoids direct `.vivi` storage reads and keeps the first implementation
usable from outside the Rust internals. If the later product direction is a
native `vivi linear ...` subcommand, it can reuse the same read model internally
rather than changing the mapping.

The current `~/work/ianzepp/vivarium` mailspace has no open board items, so the
first dry-run test should either use a fixture mailspace or accept a seeded
project root. Do not mistake an empty board for a complete exporter.

## First useful milestone

An operator can run a dry-run against one project root and see exactly what
Linear project/issues would be created or updated from open Vivi tasks, needs,
and wants, with skipped local-only records counted and explained.

## Acceptance criteria

- The mapping names the Linear target for each Vivi kind.
- The mapping states what remains local-only.
- The mapping states the one-way pilot conflict rule.
- The next implementation stage is clear and does not require direct `.vivi`
  storage reads.

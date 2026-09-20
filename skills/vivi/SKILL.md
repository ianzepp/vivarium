---
name: vivi
description: Use the Vivi CLI for project mailspaces, roles, goals, work graphs, local email search and drafts, queues, and explicitly authorized email mutations.
---

# Vivi

Use `vivi` as the interface. Do not inspect or mutate `.vivi` databases, blob
trees, email caches, or indexes directly unless the user explicitly requests
low-level debugging.

The command surface evolves. Before relying on exact flags or versioned
features, run:

```sh
vivi --version
vivi --help
vivi <command> --help
```

Use `--project <root>` whenever the project cannot be inferred safely from the
current directory. If the task concerns Vivi product implementation or release
behavior, use only the source repo named by the user.

## Project Mailspaces

A project mailspace is durable coordination state. Use these kinds
consistently:

| Kind | Meaning |
|---|---|
| `task` | Concrete work assigned now, with an owner and done condition |
| `need` | Must-do-soon work awaiting lowering or assignment |
| `want` | Deferred work or an idea whose precondition is not yet met |
| `mail` | Question, report, decision, or handoff communication |
| `memo` | Private durable context for one role; not work or communication |

The board aggregates open work. Kind-specific `list` commands orient within one
queue. `show` loads one handle. Use `dump` only for bounded audits or recovery;
full dumps are noisy and can hide the current frontier.

Default inspection order:

```sh
vivi board --project <root> --for <role> --process --graph --json
vivi task list --project <root> --for <role> --status open
vivi need list --project <root> --for <role> --status open
vivi want list --project <root> --for <role>
vivi mail list --project <root> --for <role>
vivi task show --project <root> <handle>
```

Prefer JSON plus a narrow time, status, sender, or handle filter for automation.
Use `vivi trace <handle>` to reconstruct communication lineage; do not confuse
that tree with an executable work graph.

## Roles and Goals

Roles are durable seats with a mailbox, kind, status, charter, harness, model
preferences, cadence, and labels. Read the charter before operating as a role:

```sh
vivi role show <role> --project <root>
vivi role charter show <role> --project <root>
vivi role status <role> --project <root>
```

Capacity and model fields are preferences. Use `role status` or
`board --process` when process state matters. A host starting a role should
pass a short pointer to the charter and one assignment handle rather than
pasting the standing procedure.

Registered goals are the project's explicit working set:

```sh
vivi goal list --project <root> --json
vivi goal show --project <root> <handle-or-path>
```

Goal registration does not replace reading the referenced document. Adding or
dropping a goal changes registration only; it does not create or delete the
document.

## Lifecycle

Use `send` to create a project-local record and `mail reply` to continue its
thread. Use task dependencies for small standalone relationships and work
graphs for multi-unit executable topology.

Closing a task, need, or want records its current disposition. Reopen when the
CLI permits and evidence changes. `want promote` moves deferred work into the
must-do queue.

`absorb` seals a record. After absorption it cannot be changed, reopened,
promoted, dropped, reprioritized, or deleted. A later reply or derived task is
a new record. Absorb only when the identity is finished with the item and its
state should remain frozen.

Memos preserve durable role context. They are not routing, assignment, or
completion evidence. Delete superseded unsealed memos; absorb memos that should
be retained as immutable history.

When a mailspace has a configured archive, absorption exports one Markdown
record to that dedicated Git repository. Vivi does not stage or commit the
archive. Use the archive commands and `--help`; do not write exported files by
hand.

## Work Graphs

Work graphs are project-local DAGs. Topology can be authored through the
supported Mermaid subset (`graph import` / `apply`) — or it accumulates
automatically: every `task` / `need` / `want` send mints an open node in the
per-mailspace `backlog` graph, and `--depends-on` on any work-kind send
(accepting task/need/want handles, validated before send) becomes a
prerequisite edge. Confirm the installed Vivi version and command help
before use.

Core semantics:

- import or apply validates the whole graph atomically;
- source node identifiers remain stable across label changes;
- a node is ready when all prerequisites are done;
- activation binds one task attempt to a ready node;
- completion may unlock successors, and writes a `step_decision` graph
  event in the same transaction recording the deciding path;
- active or completed prerequisites cannot be rewritten incompatibly;
- graph readiness describes eligibility, not scheduling policy.

Lifecycle moves keep backlog nodes in step: `task done` / `need done` /
`want done|drop` complete the item's node and unlock dependents; `reopen`
re-locks them; `want promote` never changes node state and wants never
dispatch in `vivi step` before explicit promotion. Lowering is a graph fact:
`vivi need bind <need> <task>...` binds unit tasks to a need, and the need
auto-completes when every bound unit lands.

Use `graph show`, `graph ready`, and `board --graph` for inspection. Use
`vivi step [--json]` for a mechanical adjudication of the backlog into
`dispatches` (ready, verifiable work with clause counts) and `exceptions`
(reason vocabulary: `want_requires_promotion`, `lowered_awaiting_units`,
`no_done_when`, `item_missing`, `not_settled`). `vivi step --apply <handle>`
completes an already-settled item's node (never settles it) and lists the
transition under `decisions`. The coordination host decides which ready
nodes to dispatch and records attempts through Vivi tasks.

## Watches and Cycle Intake

Project-local watch commands observe mailspace events. Prefer `--once` for
fail-fast cycles or an explicit timeout for waiting. Use cursor or watermark
files when repeated cycles must not replay old events. Add graph event filters
only when node readiness or state changes are relevant.

`cycle intake` collects pending tasks, needs, mail, and recent memos for one
identity. It is a bounded intake surface, not a replacement for inspecting the
specific handle selected for work.

## Email

For IMAP-backed email, sync before claiming current state when freshness
matters. Use `list`, `search`, `show`, and `thread` for bounded inspection.
Keyword search is the default; semantic or hybrid search requires a healthy
embedding index. Rebuild indexes through Vivi rather than editing storage.

Distinguish local preparation from remote effects:

| Surface | Effect |
|---|---|
| `compose` / `reply` | Create a local draft; do not send |
| `enqueue` | Store a proposed remote action for later review |
| `queue run` | Execute queued remote actions |
| `exec` | Execute a remote write immediately |

Remote writes include sending, archiving, deleting, moving, flagging, and
running queued actions. They require authority appropriate to the user's
request. Never run `vivi exec send` without explicit approval for that send.
Prefer queue-first when work is agent-prepared, uncertain, or awaiting review.

For a non-trivial message:

1. Create the local draft.
2. Inspect the generated `.eml`, including recipients, subject, plain text,
   HTML, threading headers, and attachments.
3. Revise locally if needed.
4. Send only after explicit approval.

Use `password_cmd` or the configured secret mechanism. Never print credentials
or place passwords in shell history.

## Safety

- Use the CLI rather than underlying stores.
- Verify current help before exact mutations.
- Narrow reads before dumps and broad searches.
- Treat absorb as permanent.
- Treat `queue run` as a remote mutation, not a review action.
- Do not emulate unsupported labels with folder moves.
- Do not infer a source repository or account when the user has not identified
  it.
- Report whether an operation was local, queued, or executed remotely.

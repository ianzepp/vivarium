# Request Notes: Agent Cycle Improvements For Vivi

## Summary

These notes capture concrete Vivi command-shape improvements requested after
running a multi-agent Faber fleet through repeated correctness-findings cycles.
The theme is not a new coordination model; it is making the existing
mailspace/task/need/want model easier for LLM operators to use reliably.

The most important gaps are:

- Advisory mail can be read, but not marked as absorbed by a role.
- Wants carry priority and routing mostly in subject text instead of structured
  fields.
- Converting a prioritized want into executable tasking loses lineage unless the
  operator preserves it manually.
- Fleet-style cycle intake requires several separate `list`, `show`, `board`,
  and sensor calls.

## Context

In a fleet loop, Mind receives findings from Hands, routes them to CTO, CTO
creates wants, and Mind keeps Hands supplied with non-overlapping work. The
current primitives are enough to make that work, but they force LLM operators to
spend attention on stale signals and manual summarization.

This matters because LLM reliability improves when state transitions are typed,
queryable, and durable. The desired command shapes below are intended to reduce
chat-memory dependence and reduce the amount of prose parsing needed in a
long-running operator loop.

## Request 1: Absorb Ordinary Mail

Add a first-class way for a role to mark advisory mail as handled without moving
it into task/need/want lifecycle. In fleet terms, this is bookkeeping when a
signal has been dispositioned; it is not an integration acceptance gate.

Example:

```sh
vivi mail absorb --project "$ROOT" --for mind <handle> \
  --note "Converted findings to CTO prioritization request 2ee9730"
```

Desired behavior:

- Record the absorbing identity, timestamp, note, and absorbed status.
- Preserve the original mail and thread.
- Exclude absorbed mail from future wake-candidate style cycle intake.
- Do not treat absorbed mail as accepted work, cleared review debt, or evidence
  that a task/result is good enough to integrate.
- Allow `mail list` / `mail dump` filters such as `--status unabsorbed`,
  `--status absorbed`, or `--absorbed-by mind`.

Why it helps:

- Prevents already-handled advisory mail from becoming repeated sensor noise.
- Gives Mind a durable record of the disposition for each report.
- Makes "read" distinct from "handled", which is the important cycle boundary.
- Preserves the fleet distinction: absorb is cycle bookkeeping; accept is an
  integration bar.

## Request 2: Structured Findings

Add a structured findings kind or command family for correctness/audit reports.
This does not need to replace mail; it can be a typed work artifact built on
mailspace storage.

Example:

```sh
vivi finding send --project "$ROOT" \
  --from hand-3 \
  --to mind \
  --scope faber-runtime \
  --severity high \
  --subject "Zero-extent broadcast panics" \
  --file faber-runtime/src/tensor.rs:289 \
  --body-file finding.md \
  --recommendation "Define zero-extent broadcast semantics and add tests."
```

Useful fields:

- `scope`
- `repo`
- `severity`
- `file` / `line`
- `repro`
- `recommendation`
- `validated_with`
- `source_task`

Follow-on command shapes:

```sh
vivi finding list --project "$ROOT" --for mind --status open --json
vivi finding forward --project "$ROOT" --to head-cto <handle>...
vivi finding triage --project "$ROOT" --for head-cto --create-wants <handle>...
```

Why it helps:

- Reduces lossy re-summarization by Mind.
- Makes CTO prioritization more mechanical and auditable.
- Allows findings to link directly to the wants they produce.

## Request 3: Structured Want Priority And Metadata

Keep wants as the backlog primitive, but add queryable metadata rather than
encoding everything in the subject line.

Example:

```sh
vivi want set-priority --project "$ROOT" --for mind <handle> \
  --priority P1 \
  --rank 20 \
  --repo faber-runtime \
  --lane correctness \
  --blocks-claim "tensor runtime does not panic on valid shape metadata" \
  --reason "Public Tensor API can panic on zero-extent broadcast."
```

Useful list shape:

```sh
vivi want list --project "$ROOT" --for mind \
  --status open \
  --sort priority,rank,created \
  --repo faber-runtime \
  --lane correctness \
  --json
```

Why it helps:

- Makes wants a real queue, not just parked prose.
- Lets Mind select the next unit by priority, repo, and lane.
- Preserves CTO ranking without relying on subject prefixes.

## Request 4: Create Tasks From Source Handles With Lineage

Add a direct task-creation path from an existing mailspace source handle when
capacity opens. The first supported source should be a want, but the command
shape should not bake `want` into the verb because mailspace handles are already
resolved globally.

Example:

```sh
vivi task from <source-handle> \
  --project "$ROOT" \
  --for mind \
  --to hand-2 \
  --subject "[P1][correctness] Fix zero-extent broadcast panic" \
  --body-file task.md
```

Desired behavior:

- Resolve `<source-handle>` with the existing global handle resolution.
- Infer the source kind from the resolved message's role, headers, and events.
- Initially support wants as sources, returning a clear unsupported-source
  error for other source kinds until their lifecycle behavior is defined.
- Create a task whose metadata records the source handle/content identity.
- For a source want, mark the want as `in_progress`, or record a lifecycle
  event showing that it has active tasking.
- Allow task completion to close or annotate the source item when that source
  kind supports it.
- Preserve thread context between the source item and task.

Why it helps:

- Implements the intended want -> task flow without manual bookkeeping.
- Gives operators a clear answer to "is this want already being worked?"
- Makes post-task cleanup less error-prone.
- Avoids a family of narrowly named commands such as `task from-want`,
  `task from-need`, or `task from-memo`.

## Request 5: Lane And Repo Occupancy View

Add a command that summarizes which identities are occupying which repos/scopes.
This can be computed from open tasks plus optional metadata.

Example:

```sh
vivi lanes --project "$ROOT" --json
```

Desired output fields:

- `identity`
- `current_handle`
- `kind`
- `subject`
- `repo`
- `scope`
- `lane`
- `status`
- `since`
- `conflicts`

Why it helps:

- Supports floater hands safely in multi-repo containers.
- Lets Mind assign parallel work without repeatedly opening every task body.
- Makes non-overlap a queryable invariant.

## Request 6: Cycle Intake Command

Add a single command optimized for Mind-style cycle intake.

Example:

```sh
vivi cycle intake --project "$ROOT" \
  --for mind \
  --cursor-file .vivi/mind-cycle.cursor \
  --write-cursor \
  --json
```

Desired output:

- New unabsorbed mail for the identity.
- Completed tasks since cursor.
- Open needs.
- Idle identities, if known from task state or configured roster.
- Open wants, optionally priority-sorted and capped.
- Advisory wake candidates excluding absorbed mail.
- Links from findings to created wants/tasks when available.

Why it helps:

- Reduces tool chatter in long-running loops.
- Provides a stable compaction/restart surface for LLM operators.
- Separates "new work to disposition" from historical board state.

## Request 7: Batch Routing Helpers

Add batch commands for common findings routing and forwarding flows.

Example:

```sh
vivi mail forward --project "$ROOT" \
  --from mind \
  --to head-cto \
  --handles 78095a0 aa3c3bf \
  --subject "prioritize faber-runtime tensor findings"
```

If structured findings exist:

```sh
vivi finding batch --project "$ROOT" \
  --from-mail 78095a0 aa3c3bf \
  --to head-cto \
  --subject "prioritize faber-runtime tensor findings"
```

Why it helps:

- Preserves original evidence while still allowing Mind to add routing context.
- Avoids repeated manual paste-and-summarize operations.
- Makes CTO triage easier to audit.

## Request 8: Stale Signal Suppression

If `mail absorb` is not enough for every case, add explicit signal suppression
for known-stale cycle inputs.

Example:

```sh
vivi signal suppress --project "$ROOT" \
  --kind mail_wake_candidate \
  --handle 8e6af66 \
  --for mind \
  --reason "Answered by head-ceo report 5aecdd3"
```

Why it helps:

- Lets an operator quiet a known-stale signal without deleting history.
- Keeps fleet sensors focused on genuinely actionable events.

This may be unnecessary if absorbed mail and cycle intake fully cover the same
use case.

## Request 9: Native Dump Filtering And Projection

Operators are starting to run `vivi mail dump --json` and pipe the results into
`jq` for common filtering and projection tasks. When the same `jq` shapes repeat,
prefer adding first-class Vivi flags so agents do not need to spend tokens and
attention on JSON post-processing.

Example current shape:

```sh
vivi mail dump --project "$ROOT" --participant mind --json |
  jq 'map(select(.date >= "2026-07-14T03:44:00")) | .[-20:] |
      map({handle, date, from, to, subject})'
```

Native command shape to consider:

```sh
vivi mail dump --project "$ROOT" \
  --participant mind \
  --since 2026-07-14T03:44:00 \
  --limit 20 \
  --fields handle,date,from,to,subject \
  --json
```

Why it helps:

- Makes recurring query intent explicit and discoverable.
- Reduces `jq` dependence for LLM operators.
- Keeps dump output small when only handles and headers are needed.
- Complements existing filters such as `--participant`, `--since`, `--before`,
  and absorb-status filters.

## Priority Recommendation

If only two requests are implemented first:

1. `vivi mail absorb`
2. `vivi task from <handle>` with wants as the initial supported source

Those two would directly improve the current fleet loop by reducing stale
advisory signal noise and making the want-to-task queue flow durable.

The next most valuable layer is structured priority metadata on wants, followed
by cycle intake.

## Non-goals

- Do not replace tasks, needs, wants, mail, or memos.
- Do not make Vivi responsible for tmux pane control or fleet scheduling.
- Do not introduce automatic LLM interruption.
- Do not require global server state beyond the project-local mailspace.
- Do not force all users into the fleet model; these should remain useful as
  general project-local work coordination primitives.

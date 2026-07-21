# Vivarium Issues & Agent Observations

## [2026-07-21] Role PID liveness is broken for subagent harnesses

**Severity:** High — produces false signals across the entire fleet sensor chain  
**Version affected:** vivi 6.3.0 (and all prior with `vivi role set --pid`)

### What happened

Fleet Hands running as subagents (Grok Build, Codex) call `vivi role set <name> --pid $$ --project <root>` at boot. Under these harnesses, `$$` is the ephemeral bash shell spawned by `run_terminal_command`, not the agent process itself. The shell exits within seconds; the agent continues working for minutes or hours.

The registered PID is dead before the agent finishes its first tool call. Vivi role status reports the process as dead. The board propagates this. Fleet sensors read `state=stopped` and emit `runtime_hand-N_stopped` signals every cycle.

### Evidence

Observed across two live fleets (swarm, faber) during 12 monitoring cycles (~2 hours, 2026-07-21):

| Observation | Evidence |
|---|---|
| Hand-5 on swarm registered PID 63489 | `vivi role status hand-1` → "dead — no live process with this pid" |
| Same hand was mid-unit with ~44 tool calls | subagent still running; work progressing |
| Hand-5 cycled through two open bag handles across cycles while showing `state=completed` | work advanced despite "stopped" status |
| Faber hand-3 closed HV-04C residual while flagged `state=stopped` | work completed despite "stopped" status |
| Swarm Mind detected and killed a dual-spawn of hand-5 on the same H2 task | sensor reported stopped → Mind spawned replacement → original still alive → duplicate detected |

### The chain of dependencies

The poison flows through the entire observation stack:

```
vivi role status (checks PID: alive/dead/CPU)
  → vivi board (reports role state from status)
    → fleet-sensors.py (reads board, emits runtime_hand-N_stopped signals)
      → Mind cycle (dispositions signals, decides whether to wake/reinit/respawn)
```

A dead PID at the source becomes a `runtime_hand-N_stopped` signal in sensors, which the Mind is obligated to disposition every cycle for every hand.

### Two stacking problems

**Problem 1: Protocol compliance.** The hand-protocol.md execution cycle lists `--clear-pid` at the end but does not mandate `--pid` registration at the start. Vivi charters are silent on PID. Mind spawn prompts sometimes include the full boot list, often abbreviate it. A hand can follow its protocol faithfully and never register a PID.

**Problem 2: Wrong process identity.** Even when a hand does register `$$`, the value is meaningless for subagent harnesses. Each `run_terminal_command` spawns a new ephemeral bash. tmux/vivi-pty long-lived sessions make `$$` meaningful because the pane process IS the agent; one-shot tool shells do not.

| Pattern | Frequency | Cause |
|---|---|---|
| Never set PID | Common | Thin charter + hand-protocol missing boot register + flaky spawn boot |
| Set PID once, immediately dead | When they try | `$$` = ephemeral tool shell |
| Clear PID at end | Partial | When they complete cleanly and remember protocol |
| True mid-unit agent death | Rare | Real stalls (separate issue) |

### Downstream damage

1. **False signals every cycle.** Every hand with a stale PID produces `runtime_hand-N_stopped` every cycle. Two fleets × ~6-8 hands each = 12-16 false signals per cycle.

2. **Wasted Mind cycles.** The Mind sees stopped signals and either acts (tries to wake/reinit a hand that's already running — causing double-spawns) or correctly ignores them (which trains it to ignore ALL stopped signals, including real ones).

3. **Double-spawns.** Confirmed on swarm: sensor reported hand-5 as stopped → Mind spawned a replacement → two agents running the same H2 task on the same code → Mind detected the duplicate and killed one. Wasted compute and attention.

4. **Head cadence starvation.** Mind cycle budget consumed by false stopped-signal disposition leaves less attention for `head_due_*` cadence signals. Both fleets showed Heads overdue for days with no action.

### Workaround in use

Both fleet Minds independently arrived at the same workaround: ignore PID for capacity decisions. Use direct verification instead — board state, git status, subagent completion notifications, and mail. This is documented as a standing rule but is fragile: it requires the Mind to distrust its own sensor layer.

### Design question

The role record currently carries only `pid` and `host`. There is no `ppid`, session id, or alternative liveness field. Options:

| Option | Pro | Con |
|---|---|---|
| Vivi gains `--liveness-source` field: `pid` (self-checked) or `parent` (Mind-owned, no PID health check) | Correct model for the harness; honest | Schema change; new concept |
| Vivi gains subagent session id / heartbeat | Accurate liveness | Needs the harness to provide a durable identity |
| Drop PID entirely for subagent harnesses; use completion as the only signal | Simplest; matches event-driven model | Loses stuck-agent detection (backup loop must handle it by elapsed time) |
| Keep PID but add `--harness subagent` that suppresses PID-based liveness in board/sensors | Minimal change | PID still gets set to wrong value; just hidden |

### Recommendation

The clean break is: **subagent fleets should not use OS PID for liveness at all.** The subagent's completion notification is the correct liveness signal. The backup loop detects stuck agents by elapsed time without completion, not by PID health. Keeping PID around for subagents is a tmux-era artifact that actively harms the control plane.

A `--harness subagent` flag on the role record (or inferring from the existing `harness` field) could suppress PID-based liveness in `vivi role status` and board output, so the sensor chain stops emitting false `state=stopped` signals without requiring a schema migration.

## [2026-07-21] `memo search` does not exist

**Severity:** Medium — forces full dump for selective retrieval  
**Version affected:** vivi 6.3.0

### What happened

`vivi memo list --project <root> --for <role>` dumps every memo for a role. There is no keyword, handle, or subject query to find memos mentioning a topic.

The fleet skill documents this gap already:

> **Known gap: no `memo search`.** `memo list` dumps every memo for a role;
> there is no keyword or handle query to find memos mentioning a topic. This
> forces a Mind to load all of a role's memos into context, which defeats
> the purpose of selective durable memory.

### Impact

Standing law, policy, architecture decisions, and capacity records live in memos. A Mind with 20 memos (current observed count on swarm) must load all 20 to find the one that mentions "Railway deploy paused" or "lowering law." After compaction, when the Mind most needs durable memory, the cost of retrieving it is highest.

Both fleet Minds independently flagged this as a priority gap during retrospective.

### Proposed command

```sh
vivi memo search --project <root> --for <role> <query>
vivi memo search --project <root> --for mind "railway deploy"
vivi memo search --project <root> --for mind --subject "ACCEPT*"
```

Search should match subject and body. Output format can match `memo list` but filtered to hits. No schema change required — memos already have subject and body fields.

## [2026-07-21] Task lifecycle lacks depends-on and blocking fields

**Severity:** Medium — forces prose-based dependency tracking  
**Version affected:** vivi 6.3.0

### What happened

A task that depends on another task completing first (e.g., "HV-07C after A+B") is expressed as prose in the body, not as a queryable structured field. The Mind cannot query "what tasks are blocked by this open task?" or "can I start this task yet?" without reading body prose.

Both fleet Minds independently requested this during retrospective:

- **Swarm:** "depends-on (HV-07C after A+B) as first-class, not prose in the body"
- **Faber:** "link unit → delivery id / tip SHAs as structured fields"

### Proposed schema addition

```sh
# Dependencies (repeatable for multi-dependency chains)
vivi task send --to hand-3 --depends-on <handle> --depends-on <handle> ...

# Query blocked tasks
vivi task list --blocked                    # open tasks with unmet depends-on
vivi task list --blocking <handle>          # what depends on this task?
```

Board and sensors could then surface blocked tasks without body parsing.

### Notes

- `--depends-on` is repeatable for multi-dependency chains.
- Backward-compatible addition; existing tasks simply lack the field.
- Separate from the verdict/tip SHA issue (see below).

## [2026-07-21] Task close lacks verdict, repo, and tip SHA fields

**Severity:** Medium — forces verdict scavenging and multi-repo git log  
**Version affected:** vivi 6.3.0

### What happened

Two structured fields are missing from task close:

1. **Verdict on close.** Auditor tasks return `clean_pass`, `residual`, or `block_ship` as a note string, not as a structured field. The Mind must parse the note to determine the outcome.

2. **Tip SHAs and repo on close.** When a Hand completes work, the commit SHA and affected repo are in the note prose, not structured fields. Multi-repo fleets (faber has hosts + examples repos) cannot query "what landed where" without reading notes.

Both fleet Minds independently requested these during retrospective:

- **Swarm:** "Structured task close: verdict + tip SHAs + repo"
- **Faber:** "verdict + tip SHAs + repo on task done"

### Proposed schema additions

```sh
# Verdict on close
vivi task done <handle> --for auditor-1 --verdict clean_pass
vivi task done <handle> --for auditor-1 --verdict residual --note 'P2: ...'
vivi task done <handle> --for auditor-1 --verdict block_ship --note 'F1: ...'

# Tip SHAs and repo on close (repeatable for multi-repo units)
vivi task done <handle> --for hand-2 --repo examples --tip e968cc3
vivi task done <handle> --for hand-2 --repo hosts --tip 0de5c36
```

### Notes

- `--verdict` is most valuable for auditor tasks but could be used by any closing role.
- `--repo` + `--tip` are repeatable for multi-repo units.
- Backward-compatible addition; existing tasks simply lack the fields.

## [2026-05-07] `vivi agent archive` does not support `--execute`

**Severity:** Medium — breaks documented agent workflow  
**Version affected:** vivi 2.2.1

### What happened

During an email triage session, I attempted to use the agent-safe plan-first workflow:
1. Plan a batch of archives via `vivi agent archive <handles>` → succeeded (dry-run output)
2. Execute the planned batch via `vivi agent archive <handles> --execute` → **failed**

The skill documentation (`mail/SKILL.md`) lists this as valid:
```
vivi agent archive <handle> --execute
```

But in practice, `vivi agent archive` does not accept `--execute`. The clap parser rejects it with:
```
error: unexpected argument '--execute' found
  tip: to pass '--json' as a value, use '-- --json'
```

### Workaround used

Fell back to the standard (non-agent) surface:
```sh
vivi archive <handle>
```
This executed successfully for all handles. The agent plan was still useful for review before execution — it just couldn't be replayed via `--execute`.

### What's actually going on

The `agent` subcommands (`archive`, `delete`, `move`, `flag`) appear to be **plan-only** surfaces that produce structured output but do not have an execute path. The skill doc conflates the agent surface with the human-facing mutation commands:

| Command | Plan? | Execute? |
|---------|-------|----------|
| `vivi archive <handle>` | No (direct) | Yes |
| `vivi archive <handle> --dry-run` | Yes (preview) | No |
| `vivi agent archive <handle>` | Yes (structured output) | **No** |
| `vivi agent archive <handle> --execute` | — | **Does not exist** |

The skill doc also lists `--execute` for `agent delete`:
```
vivi agent delete <handle> --expunge --confirm --execute
```
This likely has the same problem.

### Recommended fixes (pick one)

1. **Add `--execute` to agent subcommands** — have them replay the planned operations when `--execute` is passed. This preserves the plan-review-execute loop.

2. **Remove `--execute` from skill docs** and clarify that `agent` commands are read-only planning surfaces, while actual mutations go through the non-agent commands (`vivi archive`, `vivi delete`, etc.).

3. **Add a separate `vivi agent execute` command** that takes handles and executes their previously planned operations. This would be more useful for batch workflows like the one above where you plan 13 items at once.

### Notes

- The agent plan output is still valuable — it gives structured, machine-readable previews of what will happen (UIDs, mailbox paths, operation type).
- The non-agent `vivi archive` commands work fine and are safe to use directly after reviewing the agent plan.
- This was discovered during a real user session doing inbox triage, not in testing.

### Follow-up design decision

The larger issue is not just the missing `--execute` flag. The `agent` command
names the caller instead of the effect, and it is too easy to confuse
machine-readable planning with deferred execution.

The clean-break replacement is:

| Command | Meaning |
|---------|---------|
| `vivi exec ...` | perform the external write now |
| `vivi enqueue ...` | persist an intended write as pending queue work |
| `vivi queue list` / `show` | inspect pending or historical queue work |
| `vivi queue run <id>` | execute reviewed queued work |
| `vivi queue run --all` | execute all pending queued work in FIFO order |

Under this model, `agent` is removed from the active CLI. Agents and humans use
the same effect-oriented surfaces.

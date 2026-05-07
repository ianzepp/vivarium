# Vivarium Issues & Agent Observations

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

# Write Phase 06: Agent Approval And Audit Surface

## Interpreted Phase Problem

Phases 03 through 05 made remote mutation, draft creation, and SMTP sending
available, but the command surface is still human-oriented. Agents need a
plan-first interface that can describe intended external effects as JSON without
executing them, then execute only after an explicit approval flag.

## Normalized Phase Spec

### Goal

Make mailbox writes and outbound send/reply workflows usable by local agents
without silent external side effects.

### Inputs

- Phase 03 mutation CLI and audit records.
- Phase 05 draft-first reply/send workflow.
- Existing JSON search/show/thread outputs.
- Local account/config defaults.

### Expected Outputs

- `vivi agent` command surface for archive/delete/move/flag/send/reply.
- Agent commands plan by default and require `--execute` for external writes.
- JSON plan output for archive/delete/move/flag/send/reply.
- Audit records for planned, approved, executed, failed, and reconciled states.
- Agent defaults that keep hard delete disabled unless configured otherwise.
- Bounded draft/body previews in agent-facing JSON.
- Agent-safe workflow documentation and tests.

### Out Of Scope

- MCP server.
- Cloud-agent permissions.
- Automatic classification-driven mailbox mutation.
- Replacing the human CLI command behavior.

## Repo-Aware Phase Baseline

- Mutation commands already support `--dry-run`, `--json`, hard-expunge
  confirmation, and mutation audit JSONL.
- Draft/send commands are explicit, but `send` performs SMTP immediately.
- Reply creates a local draft and can optionally append remote Drafts.
- Existing search/show/thread commands already have JSON modes agents can use.

## Stage Graph

1. Agent CLI surface
   - Add `vivi agent <operation>`.
   - Plan by default; execute only with `--execute`.

2. Audit hardening
   - Preserve planned audit records.
   - Add approved and reconciled statuses around successful execution.
   - Add outbound agent audit for send/reply plans and execution.

3. Bounded JSON outputs
   - Emit operation, approval, external-write, and preview metadata.
   - Truncate body/draft previews for agent responses.

4. Agent defaults
   - Add config defaults for agent preview limits.
   - Keep agent hard delete disabled unless explicitly enabled.

5. Tests and gates
   - Parser tests for agent commands.
   - Unit tests for bounded output and audit records.
   - Run fmt, test, clippy, diff, and help checks.

## Checkpoint Target

A local agent can prepare mailbox changes or outbound replies as auditable JSON
plans. External writes happen only through explicit `--execute`, with hard
delete still disabled by default.

## Safety Stop

Do not execute live agent send or remote mutation commands during this phase.

## Agent-Safe Workflow

1. Discover candidate messages with bounded read commands:
   - `vivi search "terms" --json --limit 5`
   - `vivi show <handle> --json`
   - `vivi thread <handle> --json --limit 20`
2. Prepare an auditable plan:
   - `vivi agent archive <handle>`
   - `vivi agent move <handle> trash`
   - `vivi agent flag <handle> --read`
   - `vivi agent reply <handle> --body "Thanks"`
   - `vivi agent send <draft.eml>`
3. Review the JSON plan and audit record.
4. Execute only after approval:
   - `vivi agent archive <handle> --execute`
   - `vivi agent send <draft.eml> --execute`

## Delivered Outputs

- Added `vivi agent archive|delete|move|flag|send|reply`.
- Agent commands plan by default; execution requires `--execute`.
- Mutation agent commands reuse the existing dry-run JSON plan path by default.
- Send/reply agent commands emit JSON plans and write agent audit records.
- Mutation execution audit now records `approved`, `executed`, and
  `reconciled` in addition to `planned` and `failed`.
- Config defaults now include `agent_max_body_bytes`, `agent_max_results`, and
  `agent_allow_hard_delete`; hard delete is disabled by default for agent mode.
- Reply draft previews are bounded with truncation metadata.

## Correctness Pass

- Existing human commands remain behaviorally unchanged.
- Agent mutation execution delegates to the existing mutation runner, preserving
  remote-first then local-reconcile ordering.
- Agent send execution delegates to the existing explicit `.eml` send path.
- Agent reply execution creates a local draft only; it does not remote-append.
- No live SMTP send, remote APPEND, or remote IMAP mutation was executed.

## Verification

- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo run -- agent --help`
- `cargo run -- agent send --help`
- `cargo run -- agent reply --help`

## Poker Face Check

- Completion score: 90%.
- Largest gap: mutation agent plans still perform capability discovery through
  IMAP, inherited from the existing dry-run path; no live mutation was executed.
- Gate result: PASS.

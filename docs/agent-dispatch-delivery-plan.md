# Vivi Agent Dispatch Delivery Plan

## Interpreted Problem

### Claimed Problem

The current operational flow has two independent polling loops:

- `vivi sync` runs from cron and downloads new mail into local storage.
- A Hermes-side background process periodically scans Vivi-downloaded mail for trusted instructions, reads full thread context, runs an agent turn, and sends a reply.

The proposed change is to let Vivi own the trigger and mail I/O. Users should be able to register a local agent such as Hermes with a trusted sender and command, then have Vivi dispatch that agent after sync and optionally reply through the configured Vivi account.

### Inferred Actual Problem

The real problem is not just combining two commands. The goal is to move email transport, credential handling, dedupe, thread assembly, and reply delivery into Vivi while leaving task interpretation and execution in the external agent.

### Evidence And Rationale

- `src/sync_command.rs` already has a clean post-sync hook point through `run_post_sync_indexes`.
- `src/agent.rs` already implements trusted-sender filtering, full-thread context, a per-account lock, and an agent ledger.
- `src/agent.rs` is conceptually Codex-specific today (`codex_command`, `codex_args`, `codex_prompt`, `run_codex`), even though the command is configurable.
- `src/draft_runner.rs` and `src/smtp.rs` already provide the primitives Vivi needs to create replies and send mail.
- `src/cli.rs` exposes `agent poll` as a standalone command. `sync` does not currently dispatch agents.
- Local Hermes is installed at `/Users/ianzepp/.local/bin/hermes` and reports `Hermes Agent v0.13.0 (2026.5.7)`.
- Hermes supports programmatic single-turn execution through top-level `hermes --oneshot <prompt>` and `hermes chat --query <prompt> --quiet --source tool`, but the prompt is passed as an argument and the default useful output is plain final-response text, not a structured JSON-over-stdin contract.

### Confidence

High for the repo-aware shape. Medium-high for the current local Hermes CLI contract because it was checked against the installed local Hermes source and help output. Medium for the final cross-machine Hermes contract because it may need a Hermes-side adapter or new native mode.

### Ambiguities

- Whether Hermes should grow native `stdin`/JSON support, or whether Vivi should target a small local adapter command first.
- Whether automatic replies should default to `draft`, `send`, or `none`.
- Whether dispatch should run only for newly downloaded messages or also for previously unprocessed local messages after `--reset` or `--limit 0`.

## Normalized Spec

### Project Frame

Add durable, account-scoped local agent registrations to Vivi. A registration describes which trusted sender may instruct an agent, which local folder to scan, which command to run, and how Vivi should handle replies.

### Problem Statement

Vivi should become the mail-side dispatcher for trusted email-driven agents, removing the need for Hermes to know IMAP/SMTP credentials or call Vivi mail commands directly.

### Functional Requirements

1. Add agent registration commands:
   - `vivi agent add <name> --account <account> --trusted-from <email> --command <cmd> [--arg <arg> ...] [--folder inbox] [--reply-mode none|draft|send]`
   - `vivi agent list [--account <account>] [--json]`
   - `vivi agent show <name> [--json]`
   - `vivi agent enable <name>`
   - `vivi agent disable <name>`
   - `vivi agent remove <name>`
   - `vivi agent run <name> [--dry-run] [--json]`
2. Keep `vivi agent poll --from ...` as a compatibility/manual debugging path for at least one release.
3. Generalize `agent poll` internals away from Codex names into agent dispatch names.
4. Add `vivi sync --dispatch` to run enabled registrations after sync.
5. Support durable config-backed registrations.
6. Preserve trusted-sender filtering and full-thread context.
7. Add a structured worker input contract.
8. Add a structured worker output contract for optional replies.
9. If `reply-mode = send`, Vivi sends the reply through the account SMTP settings.
10. If `reply-mode = draft`, Vivi stores a local reply draft and does not send.
11. If `reply-mode = none`, Vivi records the run result without creating a reply.
12. Prevent duplicate processing through the existing ledger or a compatible migration of it.

### Non-Functional And Technical Constraints

- No shell-string command execution for registered agents. Store command plus an argument vector.
- Do not expose account secrets to the worker.
- Do not auto-send to arbitrary recipients. Replies should be constrained to the trusted sender and original thread.
- Dispatch failure must not make sync look like it failed to download mail unless the user explicitly asks for strict dispatch behavior.
- Preserve current `cargo test --quiet` health.
- Keep `accounts.toml` permission enforcement intact.

### Required Language And Runtime

- Rust 2024.
- Existing dependencies are likely enough for MVP: `serde`, `toml`, `rusqlite`, `mail-builder`, `mail-parser`, `lettre`, `clap`.
- Avoid adding async process plumbing unless the chosen worker protocol requires it.

## Repo-Aware Baseline

### Current Architecture

- CLI parsing is centralized in `src/cli.rs`.
- Runtime command dispatch is in `src/main.rs` plus command-specific runner modules.
- Account and general config types live in `src/config/types.rs`; parsing is currently `Deserialize`-only.
- Sync orchestration lives in `src/sync_command.rs`; actual account sync lives in `src/sync.rs`.
- Agent polling logic lives in `src/agent.rs`; CLI adapter lives in `src/agent_runner.rs`.
- Existing reply and send helpers live in `src/draft_runner.rs`, but many useful pieces are private runtime helpers rather than library-level APIs.
- Existing queue command can create reply drafts (`QueuedCommand::Reply`) or send `.eml` files, but registered agent auto-reply needs a more direct path than shelling through CLI.

### Hard Gates

- Registered agent commands must not execute via `sh -c`.
- Ledger idempotency must be tested before enabling post-sync dispatch.
- Auto-send must have a narrow recipient policy.
- The compatibility `agent poll` command must keep working or fail with a clear migration message.
- A failed agent command must leave a diagnosable ledger state.

### Constraint Decisions

| Decision | Why | Tradeoff |
|---|---|---|
| Store registrations in `accounts.toml` under each account for MVP. | Registrations are account-scoped and already depend on account credentials and mail root. | Later global multi-account agents may need a separate `agents.toml`. |
| Keep `agent poll` for now. | It is useful for manual testing and reduces migration risk. | Two paths exist temporarily. |
| Add `sync --dispatch` before implicit dispatch-after-sync. | Makes the operational change explicit and safer for cron rollout. | Cron line still changes once. |
| Default `reply-mode` to `none` or `draft`, not `send`. | Avoids accidental mail loops and premature auto-send. | The user must opt into full automation. |
| Use JSON output contract for registered agents. | Lets Vivi own reply and status handling cleanly. | Hermes may need an adapter or mode change. |
| Keep Vivi's registered process contract JSON-over-stdin. | Avoids argv-size limits for full thread context and keeps agent execution generic. | Current Hermes needs an adapter or native stdin mode before it can be the direct command. |

### Proposed Registration Shape

`accounts.toml`:

```toml
[[accounts]]
name = "agent-proton"
email = "agent@example.com"
provider = "protonmail"

[[accounts.agents]]
name = "hermes"
enabled = true
trusted_from = "owner@example.com"
folder = "inbox"
command = "hermes"
args = ["vivi-run"]
reply_mode = "draft" # none | draft | send
```

The exact Hermes command is intentionally provisional. Local Hermes currently exposes `hermes --oneshot <prompt>` and `hermes chat --query <prompt> --quiet --source tool`; neither is the ideal direct target for full Vivi thread JSON because both take prompt text as an argument. For the first implementation, either register a wrapper executable that reads Vivi's JSON stdin and calls Hermes, or add a native Hermes command such as `hermes vivi-run` / `hermes chat --stdin --json --source vivi`.

### Worker Input Contract

Vivi writes JSON to the registered command's stdin.

```json
{
  "version": 1,
  "agent": "hermes",
  "account": "agent-proton",
  "seed": "4f8c2d1",
  "trusted_from": "owner@example.com",
  "claimed_message_ids": ["..."],
  "messages": [
    {
      "handle": "...",
      "local_role": "inbox",
      "from": "...",
      "to": "...",
      "subject": "...",
      "body": "..."
    }
  ]
}
```

This can reuse most of `thread_context_json` from `src/agent.rs`.

### Worker Output Contract

MVP worker output should be JSON on stdout.

```json
{
  "status": "processed",
  "summary": "Checked the requested thing.",
  "reply": {
    "text": "Done. I checked the requested thing.",
    "html": null
  }
}
```

Supported statuses:

- `processed`
- `no_action`
- `needs_review`
- `failed`

For `reply_mode = none`, Vivi records summary/status only.
For `reply_mode = draft`, Vivi creates a local reply draft.
For `reply_mode = send`, Vivi sends an in-thread reply to the trusted sender.

### Local Hermes CLI Findings

Checked local Hermes install:

```sh
command -v hermes
hermes --version
hermes --help
hermes chat --help
```

Findings:

- Installed command: `/Users/ianzepp/.local/bin/hermes`.
- Version: `Hermes Agent v0.13.0 (2026.5.7)`.
- Top-level `-z, --oneshot PROMPT` prints only final response text to stdout and auto-bypasses approvals for scripts/pipes.
- `hermes chat -q, --query QUERY` supports single-query mode.
- `hermes chat -Q, --quiet` suppresses banner/spinner/tool previews and prints the final response to stdout; it prints `session_id` to stderr.
- `hermes chat --source tool` tags third-party sessions so they can be filtered out of normal user session lists.
- Source inspection confirms `--oneshot` calls `run_oneshot(prompt, ...)` with a prompt string, and quiet chat calls `run_conversation(user_message=effective_query, ...)`.

Implications for Vivi:

- Directly registering `command = "hermes"` is not enough unless Vivi can pass full thread context as argv text, which is brittle for large threads.
- Vivi should not make its generic agent contract match Hermes' current argv prompt shape; that would leak a Hermes limitation into the mail dispatcher.
- MVP registered execution should write JSON to stdin and parse JSON from stdout.
- Hermes support can be delivered either by a wrapper executable registered as the agent command, or by a small Hermes-side native command that reads stdin and emits Vivi's worker output JSON.
- Until structured Hermes output exists, Vivi can optionally support a `stdout = "text"` compatibility mode that treats stdout as the reply text, but JSON should remain the primary contract.

## Stage Graph

### Stage 1: Contract And Config Foundation

Inputs:

- Current `AgentPollOptions`, `AgentCommand::Poll`, account config types.

Outputs:

- `AgentRegistration` config type.
- `ReplyMode` enum.
- Worker I/O mode enum if needed, defaulting to `json_stdio`.
- CLI parse support for `agent add/list/show/enable/disable/remove/run`.
- Tests for parsing and TOML loading.

Dependencies:

- None.

Verification:

- `cargo test config::tests --quiet`
- `cargo test --test cli --quiet`

Approval Gate:

- Confirm registration TOML shape before downstream implementation locks it in.

### Stage 2: Generalize Agent Poll Internals

Inputs:

- `src/agent.rs` current ledger, lock, next batch, prompt, and subprocess code.

Outputs:

- Rename Codex-specific internals to generic agent names.
- Extract reusable:
  - batch selection
  - thread JSON rendering
  - ledger claim/finish
  - process execution with stdin/stdout
- Preserve `agent poll` behavior as compatibility.

Dependencies:

- Stage 1 contract decisions.

Verification:

- Existing `src/agent/tests.rs`.
- New tests proving compatibility `agent poll` still claims the same messages.

Approval Gate:

- No behavior regression for manual poll.

### Stage 3: Registered Agent Runner

Inputs:

- Registration config and generalized agent internals.

Outputs:

- `vivi agent run <name>`.
- Worker JSON input generation.
- Worker JSON output parsing.
- Optional text-output compatibility mode if we choose to support current Hermes without a wrapper.
- Ledger status records include registration name and worker output summary.
- Dry-run output shows selected batch and command without running the worker.

Dependencies:

- Stages 1 and 2.

Verification:

- Unit test with a fake command that echoes valid JSON.
- Unit test for invalid JSON output and nonzero exit.
- Unit test for disabled registration refusing to run unless `--force` is later added.

Approval Gate:

- Failed worker runs are diagnosable and do not mark messages processed.

### Stage 4: Reply Handling

Inputs:

- Worker output contract.
- Existing message reply builder, draft storage, and SMTP send code.

Outputs:

- Library-level helper to build a reply from a thread seed/original message and worker output.
- `reply_mode = none|draft|send` behavior.
- Sent replies are constrained to trusted sender and original thread.
- Optional local sent reconciliation equivalent to current `send_path`.

Dependencies:

- Stage 3.

Verification:

- Test draft creation from worker output.
- Test send path with mocked SMTP or isolated lower-level construction where possible.
- Test empty reply with `processed` status does not send.
- Test `reply_mode = send` rejects output that tries to introduce a different recipient.

Approval Gate:

- Auto-send is impossible without explicit `reply_mode = send`.

### Stage 5: Sync Dispatch Integration

Inputs:

- `src/sync_command.rs` post-sync flow.
- Registered agent runner.

Outputs:

- `vivi sync --dispatch`.
- Sync runs enabled registrations for the synced account after sync and optional indexing.
- Multi-account sync dispatches each account's enabled registrations after that account's sync.
- Dispatch failures are summarized after sync.

Dependencies:

- Stages 1 through 4.

Verification:

- CLI parse tests for `sync --dispatch`.
- Runtime tests around success/failure summarization if existing test structure allows.
- Manual local test against Proton Bridge with `reply_mode = none` or `draft`.

Approval Gate:

- Sync result remains visible even when dispatch finds no work or a worker fails.

### Stage 6: Docs, Migration, And Release

Inputs:

- Final CLI and config shape.

Outputs:

- README section for registered agents.
- Migration note: old `agent poll` remains manual/debug; new cron line is `vivi sync --account agent-proton --dispatch`.
- Example Hermes registration, using either a native Hermes `vivi-run` command if available or a wrapper executable that bridges JSON stdin to Hermes' current single-query CLI.
- Release notes and version bump.

Dependencies:

- All implementation stages.

Verification:

- `cargo fmt --check`
- `cargo test --quiet`
- `vivi agent --help`
- `vivi sync --help`
- Local dry-run with configured agent.

Approval Gate:

- User can install, register Hermes, dry-run, and enable dispatch without editing Hermes mail credentials.

## Epic Candidates And Scopable Issues

### Epic 1: Agent Registration Config And CLI

Issue 1.1: Add config types.

- Files: `src/config/types.rs`, `src/config/tests.rs`.
- Add `agents: Vec<AgentRegistration>` to `Account`.
- Add `ReplyMode`.
- Add `WorkerInputMode` / `WorkerOutputMode` only if the Hermes compatibility path requires more than `json_stdio`.
- Acceptance: TOML with `[[accounts.agents]]` parses; missing fields get safe defaults.

Issue 1.2: Add CLI subcommands.

- Files: `src/cli.rs`, `tests/cli.rs`.
- Add `AgentCommand::{Add,List,Show,Enable,Disable,Remove,Run,Poll}`.
- Acceptance: parse tests cover each command.

Issue 1.3: Add config mutation helpers.

- Files: likely new `src/config/agents.rs` or `src/agent/config.rs`.
- Current config parsing is deserialize-only, so mutation needs a TOML-preserving or minimal rewrite decision.
- Acceptance: `vivi agent add` writes a valid `accounts.toml` without exposing secrets or weakening permissions.

### Epic 2: Generalized Agent Execution

Issue 2.1: Rename Codex-specific types.

- Files: `src/agent.rs`, `src/agent_runner.rs`, `src/agent/tests.rs`.
- Acceptance: current poll tests pass; CLI compatibility remains.

Issue 2.2: Structured process runner.

- Files: `src/agent.rs` or new `src/agent/runner.rs`.
- Acceptance: fake command receives JSON stdin and stdout JSON is parsed.

Issue 2.4: Hermes adapter contract.

- Files: docs first; implementation location depends on whether the adapter lives in Vivi examples, Hermes, or user config.
- Decide between:
  - native Hermes command that reads stdin and writes Vivi worker JSON;
  - registered wrapper executable such as `hermes-vivi`;
  - temporary `stdout = "text"` compatibility mode.
- Acceptance: `vivi agent run hermes --dry-run` displays the exact command and I/O mode; a non-dry-run test can execute a fake Hermes-shaped command without argv-sized thread payloads.

Issue 2.3: Ledger schema evolution.

- Files: `src/agent.rs`.
- Add registration name, status details, output summary, maybe reply state.
- Acceptance: existing ledgers migrate or continue to read; duplicate processing remains impossible.

### Epic 3: Reply Ownership In Vivi

Issue 3.1: Extract reply construction helpers.

- Files: `src/draft_runner.rs`, `src/message/compose.rs`, possibly new library module.
- Acceptance: reply can be built without invoking editor or CLI runtime.

Issue 3.2: Implement reply modes.

- Files: `src/agent.rs`, `src/smtp.rs`, draft/message modules.
- Acceptance: `none`, `draft`, and `send` are tested.

Issue 3.3: Safety policy.

- Files: `src/agent.rs`, tests.
- Acceptance: auto-send is only to trusted sender, in-thread, from receiving account.

### Epic 4: Sync Dispatch

Issue 4.1: Add `sync --dispatch`.

- Files: `src/cli.rs`, `src/sync_command.rs`, tests.
- Acceptance: dispatch runs only after successful sync for the relevant account.

Issue 4.2: Dispatch summary output.

- Files: `src/sync_command.rs`, `src/agent.rs`.
- Acceptance: output distinguishes idle, processed, failed, disabled.

Issue 4.3: Failure semantics.

- Files: `src/sync_command.rs`, tests.
- Acceptance: failed dispatch can return nonzero for explicit strict mode later, but MVP should at least print/report without hiding sync result.

### Epic 5: Documentation And Operations

Issue 5.1: README agent registration docs.

- Files: `README.md`.
- Acceptance: user can copy/paste a Hermes registration and cron command.

Issue 5.2: Doctor enhancement.

- Files: `src/doctor_command.rs`.
- Optional but useful: include registered agent config validity in `vivi doctor`.
- Acceptance: doctor warns if registered command is missing from PATH.

## Checkpoints

### Checkpoint 1: Contract Freeze

Pass Criteria:

- Registration TOML shape accepted.
- Worker input/output JSON accepted.
- Hermes direct/wrapper/native contract decision accepted.
- Reply mode semantics accepted.
- No implementation beyond config/CLI parsing depends on unstable names.

Suggested command set:

```sh
cargo test config::tests --quiet
cargo test --test cli --quiet
```

### Checkpoint 2: Foundation Merge

Pass Criteria:

- `agent poll` still works.
- Generic agent internals exist.
- Ledger migration is covered.
- No auto-send behavior exists yet.

Suggested command set:

```sh
cargo test agent::tests --quiet
cargo test --quiet
```

### Checkpoint 3: Registered Runner

Pass Criteria:

- `vivi agent run hermes --dry-run` works.
- Fake worker JSON round trip works.
- Failed worker output is recorded and visible.

Suggested command set:

```sh
cargo test agent --quiet
vivi agent run hermes --dry-run
```

### Checkpoint 4: Reply Safety

Pass Criteria:

- `reply_mode = draft` creates a local draft only.
- `reply_mode = send` requires explicit config and sends only an in-thread trusted-sender reply.
- Invalid recipient attempts fail closed.

Suggested command set:

```sh
cargo test message --quiet
cargo test agent --quiet
```

### Checkpoint 5: Sync Dispatch Readiness

Pass Criteria:

- `vivi sync --dispatch --account agent-proton` runs sync then dispatches enabled registrations.
- Dispatch failures are visible.
- No duplicate processing on repeated runs.

Suggested command set:

```sh
cargo fmt --check
cargo test --quiet
vivi sync --account agent-proton --limit 0 --dispatch
```

## Companion Skill Plan

- Use `carmack-linus` before Checkpoint 1 to pressure-test the registration and worker contracts.
- Use `consequences` after Stage 4 to review auto-send safety and failure semantics.
- Use `poker-face` before release to compare implementation against this artifact.
- Use `zombie-docs` after implementation if README and CLI help diverge during iteration.

## Gate Plan

### Hard Correctness Gates

- Duplicate message processing remains prevented by ledger tests.
- Registered commands are executed without shell interpretation.
- Auto-send cannot target non-trusted recipients.
- Existing `agent poll` CLI remains compatible or has an explicit deprecation path.

### Operational Gates

- Existing cron `vivi sync` behavior remains unchanged unless `--dispatch` is supplied.
- `vivi doctor` remains green against Proton Bridge.
- `vivi sync --dispatch` has clear output for idle/no work.

### Release Gates

- All tests pass.
- README includes:
  - `vivi agent add hermes ...`
  - `vivi agent run hermes --dry-run`
  - `vivi sync --account agent-proton --dispatch`
- Release notes call out `agent poll` compatibility and the new registered dispatch path.

## Open Questions

1. Should `reply_mode` default to `none` or `draft`?
2. Should `vivi agent add` mutate `accounts.toml`, or should registrations live in a new `agents.toml` to avoid rewriting secret-bearing config?
3. Should `vivi sync --dispatch` return nonzero when dispatch fails, or should that require a later `--strict-dispatch` flag?
4. Should dispatch consider all unprocessed local messages, or only messages newly cataloged by the current sync?
5. What exact Hermes CLI contract should Vivi target first?
   - Current evidence favors a wrapper or native Hermes stdin/JSON mode. Direct `hermes --oneshot` / `hermes chat --query` is viable only for short prompt text and plain-text output.
6. Should Vivi support multiple trusted senders per registration, or require one registration per sender?
7. Should `vivi doctor` validate registered agent commands in MVP?

## Recommended First Slice

Implement a conservative MVP in this order:

1. Add registration config and CLI parsing.
2. Generalize current `agent poll` internals without changing behavior.
3. Add `vivi agent run <name> --dry-run` using registered config.
4. Add worker JSON input and output, with `reply_mode = none`.
5. Add `reply_mode = draft`.
6. Add `sync --dispatch`.
7. Add `reply_mode = send` only after the above is stable and tested.

This sequence gives immediate operational value without making auto-send the first risky milestone.

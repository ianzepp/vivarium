# Local Agent Mail And Task Delivery Plan

## Interpreted Problem

Vivi is already the local-first mail surface for agents and humans, but it does
not yet provide a local-only transport for arbitrary project addresses such as
`ceo@hanta-monitor.local` or `cto@hanta-monitor.local`.

That local transport is interesting because it can unify three communication
paths behind one mental model:

- agent to agent: local project addresses delivered entirely inside Vivi
- agent to human: normal drafts, queues, and external send paths
- human to agent: real inbound email, eventually routed or mirrored into an
  agent-visible local mailbox

Tasks should not become a separate coordination universe. A task is an email
message delivered to a task folder instead of the inbox. Completion can be
represented by moving the task message to `Done`, with normal replies and
threads carrying discussion, status, and handoffs.

## Normalized Spec

### Goal

Add a local-only Vivi delivery mode for project-scoped agent addresses, with
folder-based task handling.

The first useful version should let a project define local agents and then:

```sh
vivi mailspace init

vivi mail send --from ceo --to cto \
  --subject "review: local delivery" --body "Please review the API shape."

vivi task send --from ceo --to cto \
  --subject "Implement local delivery" --body @task.md

vivi mail list --for cto
vivi task list --for cto
vivi task done <handle> --for cto
```

The exact command spelling can change during implementation, but the behavior
should remain:

- local mail is email-shaped and stored as raw RFC 5322 `.eml` blobs
- local delivery has no SMTP, IMAP, Proton, or external side effects
- local agent identities are scoped to an explicitly initialized project
  mailspace
- Vivi auto-detects existing mailspaces by walking upward from the current
  directory, but never creates `.vivi/` implicitly
- mail messages land in the recipient's `Inbox`
- task messages land in the recipient's `Tasks`
- completing a task moves the message from `Tasks` to `Done`
- normal Vivi `show`, `thread`, `search`, and indexing work across local mail
  and task folders
- external recipients remain approval-gated through existing draft/queue/send
  flows

### Non-Goals

- No scheduler or background agent runtime in Vivi.
- No Orqa dependency.
- No SMTP server, IMAP server, or local network daemon in the first version.
- No remote synchronization of local project mail in the first version.
- No automatic external sending for mixed local and real recipients.
- No separate task database unless the folder model proves insufficient.
- No automatic `.vivi/` creation from arbitrary working directories.

## V1 Decisions

- `mailspace init` creates an empty explicit roster.
- Unknown local recipients are rejected.
- Local identities are added explicitly.
- The local domain defaults to a sanitized project directory name.
- `Bcc` is rejected for local delivery in v1.
- Task priority and due dates are out of v1.
- Mixed local/external sends are rejected in v1.
- Patch submission and application are a follow-up phase after local mail and
  task folders work.

## Repo-Aware Baseline

Vivi already has most of the storage primitives needed for this:

- raw message bytes are stored as immutable `.eml` blobs under each account's
  `blobs/` tree
- `.vivarium/storage.sqlite` stores messages, blob references, local folder
  role, read/star flags, metadata, and optional remote bindings
- `MessageIngestRequest` already supports local messages without remote
  bindings
- `local_role` already models user-facing folders such as `inbox`, `archive`,
  `trash`, `sent`, and `drafts`
- `vivi compose` and `vivi reply` already generate local drafts
- `vivi exec send` and `vivi enqueue send` already distinguish immediate
  external send from queued external send
- `vivi list`, `show`, `thread`, `search`, `index`, and `export` already read
  from local storage
- the implementation can reuse or adapt these primitives for a project
  mailspace, but should not force local mailspaces through `accounts.toml`

Important constraints:

- Provider config is currently account-shaped for upstream mail providers:
  `gmail`, `proton-api`, `protonmail`, or `standard`.
- Folder canonicalization currently knows the core mail roles but not `tasks`
  or `done`.
- Local project mail should not require new `accounts.toml` entries. A
  project-local address model needs a clear mapping from the current project
  root and identity strings such as `cto` to local mailboxes.
- Existing send commands are external-write oriented. Local delivery should not
  reuse a code path that can accidentally call SMTP or Proton APIs.

## Proposed Model

### Project-Local Mailspace

Add a project-local mailspace that is initialized explicitly and discovered
conservatively.

```sh
cd /path/to/project
vivi mailspace init
```

This creates:

```text
<project>/.vivi/
  mailspace.toml
  mail.sqlite
  blobs/
```

Root detection rules:

1. If `--project <path>` is passed, use that project root.
2. Else walk upward from cwd looking for an existing `.vivi/mailspace.toml`.
3. Else fail with a message such as:
   `No Vivi mailspace found. Run vivi mailspace init from the project root.`

Vivi must not use the nearest Git root as an automatic creation target. It may
mention the nearest Git root as a suggestion, but creation must be explicit.
This avoids scattered `.vivi/` directories when a user or agent runs a command
from the wrong place.

### Mailspace Status

`vivi mailspace status` is the safe preflight command for humans and agents.
It is read-only and must never create `.vivi/`.

```sh
vivi mailspace status
vivi mailspace status --json
vivi mailspace status --project /path/to/project
```

If no mailspace is found, it exits nonzero and prints a concrete diagnostic:

```text
No Vivi mailspace found.
cwd: /path/to/project/src/deep
nearest git root: /path/to/project
init: vivi mailspace init --project /path/to/project
```

If a mailspace is found, it prints the resolved root and current waiting work:

```text
mailspace hanta-monitor
root      /Users/ianzepp/work/hanta/hanta-monitor
store     /Users/ianzepp/work/hanta/hanta-monitor/.vivi/mail.sqlite

identity  inbox unread  tasks open  done
ceo       3             1           7
cto       0             2           4
cpo       1             0           2

total unread mail: 4
total open tasks: 3
```

For status, "new mail" means unread messages in `Inbox`. "Open tasks" means
messages in `Tasks`. `Done` is optional but useful for orientation.

The JSON form should expose the same data for startup checks:

```json
{
  "found": true,
  "name": "hanta-monitor",
  "root": "/Users/ianzepp/work/hanta/hanta-monitor",
  "store": "/Users/ianzepp/work/hanta/hanta-monitor/.vivi/mail.sqlite",
  "identities": [
    {
      "identity": "ceo",
      "address": "ceo@hanta-monitor.local",
      "inbox_unread": 3,
      "tasks_open": 1,
      "done": 7
    }
  ],
  "totals": {
    "inbox_unread": 4,
    "tasks_open": 3
  }
}
```

### Local Identities And Addresses

Local identities are lightweight project-scoped addresses, not configured
accounts.

```text
ceo        -> ceo@<mailspace>.local
cto        -> cto@<mailspace>.local
ceo@local  -> ceo@<mailspace>.local
```

The exact shorthand rules can be finalized during implementation, but the
principle is: local identities live inside the project mailspace, while
upstream human email still lives in configured external accounts.

The mailspace should maintain a small roster or identity table so Vivi can
validate local recipients and list known participants. In v1, identities are
added explicitly and unknown local recipients are rejected.

### Folder Roles

Extend local folder roles with:

```text
tasks  -> Tasks
done   -> Done
```

`Tasks` is the open task folder. `Done` is the completed task folder. Task
discussion can continue as ordinary mail replies in `Inbox`, but the task
message itself moves through `Tasks` and `Done`.

### Mail Commands

Prefer a mail-shaped surface instead of making users name the transport:

```sh
vivi mail send --from <addr-or-identity> --to <addr-or-identity> \
  [--cc <addr-or-identity>] [--bcc <addr-or-identity>] \
  --subject <subject> --body <body>

vivi mail deliver <path-to-eml> --folder inbox
vivi mail list --for <identity> [--folder inbox]
```

`mail send` should route by recipient and policy:

- all recipients resolve to local identities in the current mailspace: deliver
  locally and immediately
- any external recipient is present: use the external draft/queue/send safety
  model, not silent immediate delivery
- mixed local and external recipients: reject or queue for review in v1

`mail deliver` is the low-level/debug surface for an explicit `.eml`. It should
still obey project mailspace detection and reject external delivery in v1.

### Task Commands

Tasks are email messages delivered to `Tasks`.

```sh
vivi task send --from <addr-or-identity> --to <addr-or-identity> \
  --subject <title> --body <body|@file>
vivi task list --for <identity> [--status open|done]
vivi task show <handle>
vivi task done <handle> --for <identity> [--note <body>]
vivi task reopen <handle> --for <identity> [--note <body>]
```

Implementation:

- `task send` builds an RFC 5322 message and locally delivers it to each
  recipient's `Tasks` folder
- sender gets a copy in `Sent`
- `task list --status open` lists `local_role = "tasks"`
- `task list --status done` lists `local_role = "done"`
- `task done` moves a local task from `tasks` to `done`
- optional notes should be represented as normal reply messages in the thread,
  not by rewriting the task body

Headers such as `X-Vivi-Kind: task` may be added for clarity and future
projection, but folder placement is the source of open/done lifecycle.

### Task Identity And Threads

Tasks should have stable Git-style handles derived from the original task
message. Do not invent fixed semantic prefixes such as `tsk_`.

Use the root task message as the identity basis:

- full identity: the root task message's stable internal message id or content
  id
- display handle: shortest unambiguous hash prefix, with the same style as
  existing Vivi short handles
- command input: any unambiguous prefix
- ambiguity: fail with candidate matches rather than guessing

The handle must stay stable when the task moves from `Tasks` to `Done`, so the
hash basis must not include the current folder.

The email thread remains primary. Clarifying questions and status updates
should be replies to the root task message, not detached task comments in a
separate system:

```sh
vivi task show 9f3a8c2
vivi mail reply 9f3a8c2 --from cto --body "Do you want this in v1?"
vivi task done 9f3a8c2 --for cto --note "Implemented and tested."
```

`task show <handle>` should render the root task and relevant thread context.
`mail reply <handle>` should resolve task handles as thread roots, so agents can
clarify task scope through ordinary mail while still using stable task
references.

## Stage Graph

### Stage 1: Project Mailspace Initialization And Detection

Add an explicit project mailspace lifecycle.

Expected outputs:

- `vivi mailspace init` creates `.vivi/mailspace.toml`, `.vivi/mail.sqlite`,
  and `.vivi/blobs/` under the selected project root
- mailspace creation is explicit and never happens as a side effect of
  `mail send`, `task send`, list, search, or show commands
- commands that need a local mailspace walk upward from cwd to find an existing
  `.vivi/mailspace.toml`
- `--project <path>` overrides cwd-based detection
- missing mailspace errors explain how to initialize one and may suggest the
  nearest Git root without creating anything there
- `vivi mailspace status` reports whether a mailspace can be found and, when
  found, summarizes unread inbox mail and open tasks per identity
- `vivi mailspace status --json` returns the same information in a stable
  machine-readable shape for agents
- tests cover detection from project root, detection from subdirectories,
  explicit `--project`, missing-mailspace failure, read-only status behavior,
  and per-identity count summaries

Checkpoint:

```sh
cd /path/to/project
vivi mailspace init
mkdir -p src/deep
cd src/deep
vivi mailspace status
```

reports the original project root, does not create any nested `.vivi/`, and
shows waiting unread mail plus open task counts.

### Stage 2: Folder Role Expansion

Teach storage, listing, search filters, and folder canonicalization about
`tasks` and `done`.

Expected outputs:

- `vivi mail list --for <identity> --folder tasks` works
- `vivi mail list --for <identity> --folder done` works
- `vivi task list --for <identity> --status open` works
- `vivi task list --for <identity> --status done` works
- `vivi search <query> --folder tasks` works
- local role names remain lowercase in SQLite
- remote providers do not start assuming `Tasks` or `Done` exist upstream

Checkpoint:

Tests can ingest synthetic local messages into `tasks` and `done`, then list,
show, search, and thread them.

### Stage 3: Mailspace Delivery Primitive

Add an internal delivery function that takes raw `.eml` bytes and a target
local role, validates local recipients, and ingests one message per recipient
identity inside the project mailspace.

Expected outputs:

- delivery to one local recipient lands in that recipient's `inbox`
- delivery to multiple local recipients lands in each recipient's inbox
- sender gets a `sent` copy
- generated message IDs are stable enough for threading but unique enough for
  repeated sends
- external recipients are rejected with a clear error
- no remote binding rows are created

Checkpoint:

A unit test sends from `ceo@project.local` to `cto@project.local` and verifies
the CTO inbox plus CEO sent copy in the same project mailspace without network
mocks.

### Stage 4: Local Mail CLI

Expose a small command surface for local-only mail.

Candidate CLI:

```sh
vivi mail send --from ceo --to cto \
  --subject "review: API" --body "Please review."

vivi mail deliver draft.eml --folder inbox
```

Expected outputs:

- parser tests for `mail send` and `mail deliver`
- help text explicitly says local delivery has no external side effects
- delivery rejects non-local recipients
- delivery supports `To` and `Cc`
- `Bcc` is rejected for local delivery in v1

Checkpoint:

An end-to-end CLI test composes and delivers local mail, then uses `vivi list`
and `vivi show` to read it.

### Stage 5: Task CLI As Folder-Based Mail

Add task commands as semantic wrappers over local mail and folder moves.

Candidate CLI:

```sh
vivi task send --from ceo --to cto \
  --subject "Implement local delivery" --body @task.md
# created 9f3a8c2
vivi task list --for cto
vivi mail reply 9f3a8c2 --from cto --body "Clarifying question..."
vivi task done 9f3a8c2 --for cto --note "Implemented."
```

Expected outputs:

- `task send` delivers to `tasks`, not `inbox`
- task list shows Git-style abbreviated hash handles derived from root task
  messages
- task commands accept any unambiguous task handle prefix and reject ambiguous
  prefixes with candidate matches
- task handles remain stable after moving from `tasks` to `done`
- `task list` is a task-focused view over the `tasks` folder
- `task show <handle>` renders the root task plus thread context
- `mail reply <task-handle>` resolves the task handle as the root thread
- `task done` moves from `tasks` to `done`
- `task reopen` moves from `done` back to `tasks`
- optional done/reopen notes create reply messages in the thread when provided
- tasks remain readable through ordinary `show`, `thread`, `search`, and
  `export`

Checkpoint:

An end-to-end test creates a task, lists it, marks it done, verifies it leaves
`tasks`, verifies it appears in `done`, verifies the abbreviated task handle is
still accepted, and verifies clarification replies remain in the task thread.

### Stage 6: Human Boundary And Safety Rules

Define the bridge from local agent communication to external human email
without making it automatic.

Expected outputs:

- local delivery commands reject external recipients
- external sends continue to use `compose`, `enqueue send`, and `exec send`
- docs explain the boundary: internal local mail is immediate, external mail is
  draft/queue/approval oriented
- mixed local/external recipient handling is rejected in v1

Checkpoint:

Tests prove `vivi mail send --to human@example.com` fails or queues according
to the selected v1 policy and points callers to external draft/queue commands.

### Stage 7: Documentation And Example Executive Team Setup

Document a project-local executive team setup without requiring Orqa.

Expected outputs:

- README section for project mailspaces
- README section for local identities and addresses
- README section for tasks-as-folders
- example commands for agent-to-agent mail, task creation, task completion, and
  external human draft handoff

Checkpoint:

A user can follow the docs to initialize a project mailspace and run a
two-agent mail and task exchange without editing upstream account config and
without network access.

### Follow-Up Stage: Patch Mail And CTO Merge Gate

After local mail and task folders work, add a patch-mail workflow for agent
changes. This should not be part of v1.

The goal is to let agents propose repository changes without mutating the
canonical worktree directly. Agents can work in temporary sandboxes or scratch
copies, produce a patch, and send it to the CTO or another merge owner for
review and application.

Candidate CLI:

```sh
vivi patch submit --from cpo --to cto \
  --subject "clarify onboarding copy" --body @rationale.md --patch proposal.patch

vivi patch list --for cto
vivi patch show <handle>
vivi patch apply --check <handle>
vivi patch apply <handle>
vivi patch reject <handle> --reason "Needs tests."
```

Expected behavior:

- patch submissions are email-shaped messages, not a separate coordination
  database
- patch payloads are stored as attachments or mail-linked blobs
- patch review discussion happens as replies in the patch thread
- patch handles use the same abbreviated-hash style as task/message handles
- `patch apply --check` validates the patch against the selected project root
  without mutating files
- `patch apply` applies to the selected project root only after the merge owner
  chooses to do so
- applying a patch should record a reply/status message in the patch thread
- rejected patches remain searchable and auditable

This stage shifts default agent collaboration away from shared write access and
toward a Git-native review loop:

- most roles propose findings, tasks, and patches
- CTO owns normal code application and merge decisions
- COO verifies applied changes
- CSO reviews risky or security-sensitive patches
- CEO resolves priority conflicts when patch threads disagree

## Epic Candidates And Scopable Issues

### Epic: Project Mailspace Foundation

- Add `vivi mailspace init`.
- Add conservative upward mailspace detection.
- Add `--project <path>` override for local mailspace commands.
- Add `vivi mailspace status` and `vivi mailspace status --json`.
- Add unread inbox, open task, and optional done counts per identity.
- Add tests proving commands do not create `.vivi/` implicitly.

### Epic: Local Role And Folder Support

- Add canonical roles for `tasks` and `done`.
- Update list/search help text.
- Add storage tests for task/done roles.
- Ensure no remote mutation path assumes local-only roles exist upstream.

### Epic: Local Delivery

- Add local identity roster inside the project mailspace.
- Add delivery function for raw `.eml` bytes.
- Add sent-copy behavior.
- Add rejection for external recipients.
- Add threading and repeated-send tests.

### Epic: Local Mail CLI

- Add `vivi mail send`.
- Add `vivi mail deliver`.
- Add `vivi mail list --for <identity>`.
- Add parser and help tests.
- Add end-to-end CLI tests.

### Epic: Task Mail CLI

- Add `vivi task send`.
- Add `vivi task list`.
- Add Git-style abbreviated task handles.
- Add task-handle resolution for `task show`, `task done`, `task reopen`, and
  `mail reply`.
- Add `vivi task done`.
- Add `vivi task reopen`.
- Add optional note replies.
- Add task lifecycle tests.

### Epic: Safety And Docs

- Document local versus external delivery boundaries.
- Add examples for executive-team identities in a project mailspace.
- Add release smoke checks for local delivery and tasks.

### Future Epic: Patch Mail

- Add `vivi patch submit`.
- Add `vivi patch list` and `vivi patch show`.
- Add `vivi patch apply --check`.
- Add gated `vivi patch apply`.
- Add `vivi patch reject`.
- Preserve patch review as ordinary mail threads.

## Checkpoints

1. Project mailspace initialization and detection work without accidental
   `.vivi/` creation.
2. `vivi mailspace status` is a read-only preflight that reports resolved root,
   store path, unread inbox mail, open task counts, and missing-mailspace
   diagnostics.
3. `tasks` and `done` are first-class local roles for list/search/show/thread.
4. Local delivery can move an RFC 5322 message from one project-local identity
   to another with no remote side effects.
5. `vivi mail send` provides a friendly agent-to-agent mail command.
6. `vivi task send/list/done/reopen` provides folder-based task workflow.
7. External human email remains gated behind existing draft/queue/send
   semantics.
8. Docs let an executive-team skill use Vivi as its communication substrate.
9. Follow-up patch-mail design is captured without expanding v1 scope.

## Companion Skill Plan

- Use `mail` for implementation work touching Vivi commands, provider config,
  compose/reply/send, local storage, indexing, or Proton/IMAP boundaries.
- Use `delivery` again for any phase split that needs factory-ready issue
  decomposition.
- Use `clean-break` if old agent-specific queue or Orqa assumptions start
  leaking into the design.
- Use `security-scan` or a security review pass before enabling any mixed
  local/external recipient flow.

## Gate Plan

For each implementation phase:

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo run -- --help
cargo run -- mailspace --help
cargo run -- mail --help
cargo run -- task --help
```

For local delivery phases, add fixture-level checks that prove:

- commands from a project subdirectory use the parent mailspace
- commands outside a mailspace fail instead of creating `.vivi/`
- `mailspace status` is read-only in both found and not-found cases
- `mailspace status --json` includes per-identity `inbox_unread` and
  `tasks_open` counts
- no SMTP transport is constructed
- no Proton API send is called
- no IMAP mutation is attempted
- local storage rows have no remote binding
- recipient validation rejects external addresses

For task phases, add checks that prove:

- task creation lands in `tasks`
- task completion moves to `done`
- task handles are abbreviated unambiguous hash prefixes
- task handles stay stable across `tasks` to `done` moves
- ambiguous task prefixes produce a clear ambiguity error with candidates
- `mail reply <task-handle>` replies to the root task thread
- ordinary mail search/thread/show/export still work on task messages
- optional status notes are threaded replies, not destructive rewrites

For the follow-up patch-mail phase, add checks that prove:

- `patch apply --check` does not mutate the worktree
- `patch apply` applies only to the selected project root
- patch payloads remain inspectable through the patch thread
- rejected patches stay searchable and auditable

## Open Questions

1. Should `task done --note` send a reply to the task creator by default, or
   only record a local note when explicitly requested?
2. Should a future mixed local/external send split delivery automatically, or
   require a reviewed queue item every time?
3. Should a future task metadata layer use headers, body conventions, or a
   projection table for priority and due dates?
4. Should a future local delivery mode support `Bcc` by stripping
   recipient-specific headers per delivered copy?

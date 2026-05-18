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
validate local recipients and list known participants. Unknown local
recipients can either be rejected in v1 or auto-created only when an explicit
`--create-recipient` style flag is provided.

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
- tests cover detection from project root, detection from subdirectories,
  explicit `--project`, and missing-mailspace failure

Checkpoint:

```sh
cd /path/to/project
vivi mailspace init
mkdir -p src/deep
cd src/deep
vivi mailspace status
```

reports the original project root and does not create any nested `.vivi/`.

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
- delivery supports `To`, `Cc`, and `Bcc`
- Bcc should be handled deliberately: either preserve existing Vivi compose
  behavior or strip Bcc from recipient-visible blobs before delivery

Checkpoint:

An end-to-end CLI test composes and delivers local mail, then uses `vivi list`
and `vivi show` to read it.

### Stage 5: Task CLI As Folder-Based Mail

Add task commands as semantic wrappers over local mail and folder moves.

Candidate CLI:

```sh
vivi task send --from ceo --to cto \
  --subject "Implement local delivery" --body @task.md
vivi task list --for cto
vivi task done <handle> --for cto --note "Implemented."
```

Expected outputs:

- `task send` delivers to `tasks`, not `inbox`
- `task list` is a task-focused view over the `tasks` folder
- `task done` moves from `tasks` to `done`
- `task reopen` moves from `done` back to `tasks`
- optional done/reopen notes create reply messages in the thread when provided
- tasks remain readable through ordinary `show`, `thread`, `search`, and
  `export`

Checkpoint:

An end-to-end test creates a task, lists it, marks it done, verifies it leaves
`tasks`, verifies it appears in `done`, and verifies the thread remains intact.

### Stage 6: Human Boundary And Safety Rules

Define the bridge from local agent communication to external human email
without making it automatic.

Expected outputs:

- local delivery commands reject external recipients
- external sends continue to use `compose`, `enqueue send`, and `exec send`
- docs explain the boundary: internal local mail is immediate, external mail is
  draft/queue/approval oriented
- mixed local/external recipient handling is either rejected or queued with an
  explicit future design note

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

## Epic Candidates And Scopable Issues

### Epic: Project Mailspace Foundation

- Add `vivi mailspace init`.
- Add conservative upward mailspace detection.
- Add `--project <path>` override for local mailspace commands.
- Add `vivi mailspace status`.
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
- Add `vivi task done`.
- Add `vivi task reopen`.
- Add optional note replies.
- Add task lifecycle tests.

### Epic: Safety And Docs

- Document local versus external delivery boundaries.
- Add examples for executive-team identities in a project mailspace.
- Add release smoke checks for local delivery and tasks.

## Checkpoints

1. Project mailspace initialization and detection work without accidental
   `.vivi/` creation.
2. `tasks` and `done` are first-class local roles for list/search/show/thread.
3. Local delivery can move an RFC 5322 message from one project-local identity
   to another with no remote side effects.
4. `vivi mail send` provides a friendly agent-to-agent mail command.
5. `vivi task send/list/done/reopen` provides folder-based task workflow.
6. External human email remains gated behind existing draft/queue/send
   semantics.
7. Docs let an executive-team skill use Vivi as its communication substrate.

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
- no SMTP transport is constructed
- no Proton API send is called
- no IMAP mutation is attempted
- local storage rows have no remote binding
- recipient validation rejects external addresses

For task phases, add checks that prove:

- task creation lands in `tasks`
- task completion moves to `done`
- ordinary mail search/thread/show/export still work on task messages
- optional status notes are threaded replies, not destructive rewrites

## Open Questions

1. Should `mailspace init` create an explicit roster file, or should identities
   be created lazily by the first local send?
2. Should local mailspace identity shorthand require a known roster entry, or
   can `--to cto` create/resolve `cto@<mailspace>.local` automatically?
3. Should `task done --note` send a reply to the task creator by default, or
   only record a local note when explicitly requested?
4. Should `Bcc` be supported for local delivery in v1, or rejected until we can
   strip recipient-specific headers correctly?
5. Should task priority and due dates be headers, body conventions, or left out
   of v1?
6. Should local domains such as `hanta-monitor.local` be derived from
   `mailspace.toml`, the directory name, or an explicit init flag?
7. Should a future mixed local/external send split delivery automatically, or
   require a reviewed queue item every time?

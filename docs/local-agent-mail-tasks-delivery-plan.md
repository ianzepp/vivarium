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
vivi local send --from ceo@hanta-monitor.local --to cto@hanta-monitor.local \
  --subject "review: local delivery" --body "Please review the API shape."

vivi task create --from ceo@hanta-monitor.local --to cto@hanta-monitor.local \
  --title "Implement local delivery" --body task.md

vivi list inbox --account cto-hanta-monitor
vivi list tasks --account cto-hanta-monitor
vivi task done <handle> --account cto-hanta-monitor
```

The exact command spelling can change during implementation, but the behavior
should remain:

- local mail is email-shaped and stored as raw RFC 5322 `.eml` blobs
- local delivery has no SMTP, IMAP, Proton, or external side effects
- local agent addresses are validated against a project-local roster
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

Important constraints:

- Provider config is currently account-shaped and assumes a remote-capable
  provider enum: `gmail`, `proton-api`, `protonmail`, or `standard`.
- Folder canonicalization currently knows the core mail roles but not `tasks`
  or `done`.
- Local account storage is account-scoped. A project-local address model needs
  a clear mapping from an address like `cto@hanta-monitor.local` to an account
  or local mailbox root.
- Existing send commands are external-write oriented. Local delivery should not
  reuse a code path that can accidentally call SMTP or Proton APIs.

## Proposed Model

### Local Provider

Add a `provider = "local"` account type. A local account never resolves IMAP,
SMTP, OAuth, Proton sessions, or remote folder capabilities.

Example account shape:

```toml
[[accounts]]
name = "cto-hanta-monitor"
email = "cto@hanta-monitor.local"
provider = "local"
mail_dir = ".vivi/agents/cto"
```

This keeps the initial implementation compatible with Vivi's account-scoped
storage. A later project roster layer can generate these local accounts instead
of asking users to hand-edit them.

### Local Address Registry

Add a local address lookup helper over configured `provider = "local"`
accounts:

- address must match exactly one local account email
- recipient validation fails if no configured local account owns the address
- ambiguous ownership is a config error
- local domains are not special by themselves; configured accounts are the
  authority

This keeps delivery explicit and avoids accidentally treating arbitrary
`*.local` strings as valid recipients.

### Folder Roles

Extend local folder roles with:

```text
tasks  -> Tasks
done   -> Done
```

`Tasks` is the open task folder. `Done` is the completed task folder. Task
discussion can continue as ordinary mail replies in `Inbox`, but the task
message itself moves through `Tasks` and `Done`.

### Local Delivery Commands

Prefer a small, explicit local surface instead of overloading external send:

```sh
vivi local send --from <addr> --to <addr> [--cc <addr>] [--bcc <addr>] \
  --subject <subject> --body <body>

vivi local deliver <path-to-eml> --folder inbox
```

`local send` is a convenience around compose plus local delivery. `local
deliver` accepts an explicit `.eml` and delivers it locally only.

Both must reject external recipients by default. If mixed local/external
delivery is ever supported, it should be a later phase with explicit queueing
and review semantics.

### Task Commands

Tasks are email messages delivered to `Tasks`.

```sh
vivi task create --from <addr> --to <addr> --title <title> --body <body|@file>
vivi task list --account <local-account> [--status open|done]
vivi task show <handle>
vivi task done <handle> [--note <body>]
vivi task reopen <handle> [--note <body>]
```

Implementation:

- `task create` builds an RFC 5322 message and locally delivers it to each
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

### Stage 1: Local Provider And Config Validation

Add `Provider::Local` and make account validation understand local accounts.

Expected outputs:

- `provider = "local"` parses from `accounts.toml`
- local accounts do not require remote host, username, password, token, OAuth,
  IMAP, or SMTP fields
- `doctor` reports local accounts as local-only rather than trying remote
  connectivity
- tests cover config parsing and local account secret bypass behavior

Checkpoint:

```sh
vivi doctor --account cto-hanta-monitor
```

returns a clear local-only status without network access.

### Stage 2: Folder Role Expansion

Teach storage, listing, search filters, and folder canonicalization about
`tasks` and `done`.

Expected outputs:

- `vivi list tasks --account <local-account>` works
- `vivi list done --account <local-account>` works
- `vivi search <query> --folder tasks` works
- local role names remain lowercase in SQLite
- remote providers do not start assuming `Tasks` or `Done` exist upstream

Checkpoint:

Tests can ingest synthetic local messages into `tasks` and `done`, then list,
show, search, and thread them.

### Stage 3: Local Delivery Primitive

Add an internal delivery function that takes raw `.eml` bytes and a target
local role, validates local recipients, and ingests one message per recipient
account.

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
the CTO inbox plus CEO sent copy without network mocks.

### Stage 4: Local Mail CLI

Expose a small command surface for local-only mail.

Candidate CLI:

```sh
vivi local send --from ceo@project.local --to cto@project.local \
  --subject "review: API" --body "Please review."

vivi local deliver draft.eml --folder inbox
```

Expected outputs:

- parser tests for `local send` and `local deliver`
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
vivi task create --from ceo@project.local --to cto@project.local \
  --title "Implement local delivery" --body @task.md
vivi task list --account cto-project
vivi task done <handle> --account cto-project --note "Implemented."
```

Expected outputs:

- `task create` delivers to `tasks`, not `inbox`
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

Tests prove `vivi local send --to human@example.com` fails and points callers
to external draft/queue commands.

### Stage 7: Documentation And Example Executive Team Setup

Document a project-local executive team setup without requiring Orqa.

Expected outputs:

- README section for `provider = "local"`
- README section for local agent addresses
- README section for tasks-as-folders
- example `accounts.toml` for CEO/CPO/CTO/COO/CSO/CMO/CFO/CXO local accounts
- example commands for agent-to-agent mail, task creation, task completion, and
  external human draft handoff

Checkpoint:

A user can follow the docs to create local accounts and run a two-agent mail
and task exchange without network access.

## Epic Candidates And Scopable Issues

### Epic: Local Provider Foundation

- Add `Provider::Local`.
- Relax local account required fields.
- Make doctor report local-only accounts.
- Add tests for local config parsing and validation.

### Epic: Local Role And Folder Support

- Add canonical roles for `tasks` and `done`.
- Update list/search help text.
- Add storage tests for task/done roles.
- Ensure no remote mutation path assumes local-only roles exist upstream.

### Epic: Local Delivery

- Add address registry over configured local accounts.
- Add delivery function for raw `.eml` bytes.
- Add sent-copy behavior.
- Add rejection for external recipients.
- Add threading and repeated-send tests.

### Epic: Local Mail CLI

- Add `vivi local send`.
- Add `vivi local deliver`.
- Add parser and help tests.
- Add end-to-end CLI tests.

### Epic: Task Mail CLI

- Add `vivi task create`.
- Add `vivi task list`.
- Add `vivi task done`.
- Add `vivi task reopen`.
- Add optional note replies.
- Add task lifecycle tests.

### Epic: Safety And Docs

- Document local versus external delivery boundaries.
- Add examples for executive-team local accounts.
- Add release smoke checks for local delivery and tasks.

## Checkpoints

1. Local provider is parseable and inspectable without network access.
2. `tasks` and `done` are first-class local roles for list/search/show/thread.
3. Local delivery can move an RFC 5322 message from one configured local
   address to another with no remote side effects.
4. `vivi local send` provides a friendly agent-to-agent mail command.
5. `vivi task create/list/done/reopen` provides folder-based task workflow.
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
cargo run -- local --help
cargo run -- task --help
```

For local delivery phases, add fixture-level checks that prove:

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

1. Should local agent accounts be manually configured `[[accounts]]` entries in
   v1, or should Vivi generate them from a project roster file?
2. Should local mail live under the global Vivi mail root by default, or under a
   project-local path such as `.vivi/agents/<role>`?
3. Should `task done --note` send a reply to the task creator by default, or
   only record a local note when explicitly requested?
4. Should `Bcc` be supported for local delivery in v1, or rejected until we can
   strip recipient-specific headers correctly?
5. Should task priority and due dates be headers, body conventions, or left out
   of v1?
6. Should local domains such as `hanta-monitor.local` be reserved/validated, or
   should configured local account email addresses be the only source of truth?
7. Should a future mixed local/external send split delivery automatically, or
   require a reviewed queue item every time?


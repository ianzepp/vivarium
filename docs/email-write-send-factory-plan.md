# Vivi Factory Plan: Upstream Email Writes And Outbound Sending

## Factory Intake

### Phase Set Source

Pharos now exposes both IMAP and SMTP for the Proton Bridge account. Vivi can
therefore grow from a local read/search archive into a controlled write-capable
email assistant surface, but only after the archive foundation is stable.

This plan is intentionally staged after `docs/local-email-archive-factory-plan.md`.
The read-only archive remains the product foundation. Upstream modification and
outbound sending are later capabilities with stricter safety rules because they
can change external mailbox state or send mail to other people.

### Target Repo

`/Volumes/code/ianzepp/vivarium`

### Delivery Spec Directory

Write one delivery spec per phase under:

`docs/factory/`

Recommended names:

- `docs/factory/write-phase-00-remote-identity-foundation.md`
- `docs/factory/write-phase-01-remote-folder-capabilities.md`
- `docs/factory/write-phase-02-imap-mutation-primitives.md`
- `docs/factory/write-phase-03-mutation-cli-safety.md`
- `docs/factory/write-phase-04-smtp-send-baseline.md`
- `docs/factory/write-phase-05-compose-reply-and-sent-sync.md`
- `docs/factory/write-phase-06-agent-approval-and-audit.md`
- `docs/factory/write-phase-07-provider-labels.md`

### Checkpoint Policy

Each phase must end with:

- a saved phase delivery spec
- implementation complete for that phase only
- focused correctness review
- repo validation commands run, or skipped checks documented
- live Pharos/Bridge smoke checks when the phase touches remote IMAP or SMTP
- a phase checkpoint note in the delivery spec
- a local commit

### Commit Policy

Commit after every completed phase. Keep commits small and phase-scoped. Do not
remove the existing outbound code path; repair or replace it only when that phase
owns the outbound surface.

### Agent Policy

Use explorer agents for bounded codebase questions. Use implementation agents
only for narrow write scopes. Factory remains responsible for final integration,
correctness review, live validation, and commits.

### Correctness Policy

Preserve these invariants throughout:

- raw email bytes remain preserved and citeable
- derived data remains rebuildable
- upstream writes are remote-first, then mirrored locally only after success
- destructive commands default to Trash, not hard expunge
- hard expunge requires explicit user confirmation
- outbound mail is draft-first until a send command is explicitly approved
- every external write or send has an audit record
- cloud LLMs never receive broad corpus access by default
- agent-facing commands support JSON plans before execution

### Current Baseline Note

As of this plan, the default `vivi` binary is primarily read/retrieval oriented.
The current `archive` command moves local Maildir files only. SMTP send, compose,
reply, auth, token, and watch code exists behind the `outbox` feature and is not
part of the default Homebrew surface.

The current sync path stores raw `.eml` messages and a lightweight RFC
Message-ID index, but it does not persist enough remote identity for safe
upstream writes. Local handles such as `inbox-2039` are not sufficient because
IMAP UIDs are only valid inside a remote mailbox and UIDVALIDITY epoch.

## Irreducible Requirements

1. The read-only archive foundation remains first.
2. Upstream writes must use IMAP, not local Maildir moves alone.
3. Outbound sending must use SMTP and then preserve a Sent copy through either
   verified server behavior or IMAP APPEND.
4. Remote message identity must be durable before any mutation command becomes
   remote-writing.
5. Provider and folder semantics must be explicit in config.
6. Destructive mailbox changes must be reversible by default.
7. Agent use must be approval-oriented, auditable, and JSON-friendly.

## Phase Set

### Write Phase 00: Remote Identity Foundation

#### Goal

Persist enough remote state to map a local message handle back to the exact
remote IMAP object that should be modified.

#### Inputs

- existing sync path
- existing Maildir store
- future catalog from the read-only archive plan
- IMAP mailbox metadata, especially UIDVALIDITY

#### Expected Outputs

- durable remote identity table or catalog extension
- account, provider, remote mailbox, local folder, UID, UIDVALIDITY, RFC
  Message-ID, size, and content fingerprint per synced message
- sync updates that write remote identity alongside raw message storage
- lookup API from local handle to remote message reference
- reconciliation behavior for stale UIDVALIDITY or missing remote messages
- tests for stale, missing, and duplicate identity records

#### Out Of Scope

- IMAP mutation execution
- SMTP sending
- provider labels
- offline mutation queue

#### Checkpoint Target

Given a local message handle, Vivi can produce a remote reference containing
account, mailbox, UID, and UIDVALIDITY, or a clear stale-reference error.

### Write Phase 01: Remote Folder And Capability Discovery

#### Goal

Make Vivi understand the writable remote mailbox surface before issuing writes.

#### Inputs

- Pharos/Bridge IMAP account config
- existing provider defaults
- IMAP LIST, SELECT, STATUS, and CAPABILITY responses

#### Expected Outputs

- account config fields for inbox, archive, trash, sent, drafts, and optional
  label roots
- `vivi folders` or equivalent command to inspect remote folders
- capability probe for UIDPLUS, MOVE, SPECIAL-USE, APPEND, IDLE, and provider
  extensions when present
- folder resolution tests for Protonmail, Gmail, and standard IMAP
- docs for Pharos/Bridge IMAP and SMTP host/port expectations

#### Out Of Scope

- changing mailbox state
- sending mail
- Gmail label mutation

#### Checkpoint Target

Vivi can list and resolve the remote folders it will use for Archive, Trash,
Sent, and Drafts on the Pharos-backed Proton Bridge account.

### Write Phase 02: IMAP Mutation Primitives

#### Goal

Add a low-level IMAP mutation module that performs one remote write safely and
returns enough data for local reconciliation.

#### Inputs

- remote identity foundation
- folder/capability discovery
- existing IMAP transport

#### Expected Outputs

- `src/imap/mutate.rs` or equivalent module
- archive primitive: `UID MOVE` to Archive, with `UID COPY + STORE \Deleted +
  UID EXPUNGE` fallback when safe
- move-to-trash primitive
- hard-expunge primitive behind explicit call site
- flag primitives for read, unread, starred, and unstarred where supported
- remote-first result type describing old folder, new folder, UID changes, and
  reconciliation action
- unit tests for mutation planning and fallback selection

#### Out Of Scope

- CLI command UX
- outbound SMTP
- label-specific provider extensions

#### Checkpoint Target

Against a disposable message, Vivi can mark read/unread and move the message to
Archive or Trash remotely, then report the resulting state clearly.

### Write Phase 03: Mutation CLI And Safety

#### Goal

Expose safe, scriptable mutation commands without giving agents a foot-gun.

#### Inputs

- IMAP mutation primitives
- remote identity lookup
- existing CLI patterns

#### Expected Outputs

- `vivi archive <handle>` becomes a remote write plus local mirror update
- `vivi delete <handle> --trash` as the default delete behavior
- `vivi delete <handle> --expunge --confirm` for hard delete
- `vivi move <handle> <folder>`
- `vivi flag <handle> --read|--unread|--star|--unstar`
- shared `--dry-run`, `--json`, and confirmation behavior
- local Maildir/catalog update only after remote success
- mutation audit records under the local Vivi state directory
- tests for CLI parser, dry-run JSON, confirmation, and local mirror behavior

#### Out Of Scope

- offline retry queue
- SMTP sending
- arbitrary labels

#### Checkpoint Target

The mutation CLI can preview, execute, audit, and locally reconcile safe remote
writes on the Pharos-backed account.

### Write Phase 04: SMTP Send Baseline

#### Goal

Make outbound raw-message sending a supported default capability for the Vivi
binary.

#### Inputs

- existing `src/smtp.rs`
- existing `outbox` feature code
- Pharos/Bridge SMTP endpoint
- account auth and TLS config

#### Expected Outputs

- default release includes send support, either by removing the feature gate or
  building release artifacts with the required feature
- SMTP connection validation command or clear send-time diagnostics
- `vivi send <path>` for an explicit `.eml` file
- corrected TLS naming and behavior if needed
- envelope extraction for From, To, Cc, and Bcc as needed
- live test sending to a controlled/self address
- release workflow adjusted so Homebrew artifacts include the intended command
  surface

#### Out Of Scope

- compose/reply UX
- attachments beyond raw `.eml`
- automatic agent sending
- Sent folder reconciliation, except for recording whether the upstream already
  saved a copy

#### Checkpoint Target

The Homebrew-style `vivi` binary can send a controlled raw `.eml` through
Pharos SMTP, and the command surface is verified by `vivi --help`.

### Write Phase 05: Compose, Reply, Drafts, And Sent Sync

#### Goal

Turn outbound sending from raw-message plumbing into a safe draft-first workflow.

#### Inputs

- SMTP send baseline
- remote folder discovery
- existing compose/reply code
- message parser/rendering helpers

#### Expected Outputs

- `vivi compose` creates a local draft and, when requested, appends it to remote
  Drafts
- `vivi reply <handle>` builds RFC-compliant reply headers, including
  In-Reply-To and References
- generated messages include Date, Message-ID, From, To, Cc/Bcc where relevant,
  and safe plain-text body handling
- `vivi send <draft>` sends only explicit draft/raw paths
- sent messages are mirrored to Sent by verified upstream behavior or IMAP APPEND
- local Sent/Drafts state reconciles with remote state
- tests for generated headers and draft/send state transitions

#### Out Of Scope

- rich HTML composition
- attachment authoring
- scheduled sends
- autonomous send approval policy beyond explicit command confirmation

#### Checkpoint Target

Vivi can draft, reply, send, and verify a Sent copy for a controlled message
through Pharos IMAP/SMTP.

### Write Phase 06: Agent Approval And Audit Surface

#### Goal

Make mutation and outbound capabilities usable by local agent assistants without
silently changing external email state.

#### Inputs

- mutation CLI
- compose/reply/send workflow
- audit records
- JSON retrieval/search commands

#### Expected Outputs

- JSON plan output for archive/delete/move/flag/send/reply
- explicit execution command or `--execute` flow separated from planning
- audit log entries for planned, approved, executed, failed, and reconciled
  operations
- config defaults that disable hard delete and automatic send for agent mode
- bounded result and body sizes in agent-facing responses
- docs showing an agent-safe workflow: search, show, draft, preview, approve,
  send, verify
- tests for plan/execute separation and audit records

#### Out Of Scope

- MCP server
- cloud-agent permissions
- automatic classification-driven mailbox mutation

#### Checkpoint Target

A local agent can prepare mailbox changes or outbound replies as auditable JSON
plans, but external writes happen only through explicit approval.

### Write Phase 07: Provider Labels And Advanced Mailbox Semantics

#### Goal

Add label support after the safer folder and send paths are proven.

#### Inputs

- provider capability discovery
- mutation primitives
- account folder/label config

#### Expected Outputs

- Proton Bridge label/folder behavior documented from live probing
- Gmail label behavior scoped separately from standard IMAP folders
- provider-specific label operations where supported
- graceful unsupported errors where labels are not exposed by the provider
- tests for provider mapping and unsupported-label behavior

#### Out Of Scope

- provider private APIs
- mailbox rules engine
- autonomous labeling without explicit approval

#### Checkpoint Target

Vivi can either apply/remove labels through supported provider semantics or
explain clearly why the current account only supports folder moves.

## Series Ordering

Factory must run these phases strictly in order. Do not start sending before
remote identity and folder capability discovery exist. Do not expose agent
approval flows before the underlying mutation and outbound primitives are
validated directly.

The only acceptable reason to split a phase is discovery that the phase mixes
independent risks. If a split is needed, write the revised phase boundaries into
this plan before continuing.

## Live Validation Requirements

Use a disposable/self-addressed test message for remote checks. Never validate
destructive behavior against irreplaceable mail.

Required live checks across the full series:

- list remote folders and capabilities from Pharos IMAP
- sync a disposable message and record remote identity
- mark it read and unread
- archive it
- move it to Trash
- hard-expunge only if explicitly enabled for the test fixture
- send a self-addressed message through Pharos SMTP
- verify where the Sent copy is created
- append to Sent manually if SMTP does not do it

## Factory Stop Conditions

Pause the factory run if any of these become true:

- the read-only archive/catalog foundation is not ready enough to hold remote
  identity
- Pharos/Bridge does not expose stable writable folder semantics
- UIDVALIDITY cannot be captured or checked safely
- an operation would require Proton private APIs
- live mutation testing would risk real user mail
- release artifacts cannot be made to include the intended command surface
- agent approval semantics are ambiguous enough to allow accidental sending or
  hard deletion

## Suggested First Command For Execution

When ready to execute this write/send series, start with:

```sh
factory phase write-00 from docs/email-write-send-factory-plan.md
```

The factory should save the selected phase delivery spec to
`docs/factory/write-phase-00-remote-identity-foundation.md` before editing
implementation files.

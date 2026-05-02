# Write Phase 05: Compose, Reply, Drafts, And Sent Sync

## Interpreted Phase Problem

Phase 04 made raw `.eml` sending available in the default binary, but compose
and reply are still feature-gated and reply still sends directly in the old
path. This phase turns outbound work into a draft-first workflow: compose and
reply produce explicit draft files, send operates on explicit draft/raw files,
and successful sends reconcile local Drafts/Sent state.

## Normalized Phase Spec

### Goal

Turn outbound sending from raw-message plumbing into a safe draft-first workflow.

### Inputs

- Phase 04 SMTP send baseline.
- Remote folder discovery from Phase 01.
- Existing compose/reply helpers.
- Message parser/rendering helpers.

### Expected Outputs

- `vivi compose` creates a local draft.
- `vivi compose --append-remote` appends the draft to remote Drafts when
  requested.
- `vivi reply <handle>` creates a reply draft instead of sending immediately.
- Reply drafts include `In-Reply-To` and `References`.
- Generated messages include Date, Message-ID, From, To, Cc/Bcc where relevant,
  and safe plain-text body handling.
- `vivi send <draft>` sends only an explicit draft/raw `.eml` path.
- Successful sends mirror the raw message into local Sent.
- Successful sends remove the local Drafts copy when the input path is a local
  draft.
- Tests cover generated headers and draft/send state transitions without live
  SMTP or IMAP side effects.

### Out Of Scope

- Rich HTML composition.
- Attachment authoring.
- Scheduled sends.
- Autonomous send approval policy beyond explicit command invocation.
- Live SMTP/IMAP verification unless a controlled fixture is selected.

## Repo-Aware Phase Baseline

- `Command::Send` is default as of Phase 04.
- `Command::Compose`, `Command::Reply`, editor helpers, and reply builders are
  still feature-gated behind `outbox`.
- Existing compose drafts omit Date and Message-ID.
- Existing reply sends immediately when a body is provided, which violates the
  draft-first phase goal.
- `MailStore` can store messages in Drafts/Sent and can locate messages by local
  ID.

## Stage Graph

1. Default draft commands
   - Ungate compose/reply.
   - Add body/cc/bcc and optional remote-append arguments.

2. Message generation
   - Generate Date and Message-ID for compose and reply drafts.
   - Preserve reply threading headers.
   - Validate From and at least one recipient before storing drafts.

3. Send reconciliation
   - After SMTP success, store the raw sent message into local Sent.
   - If the source was a local Drafts message, remove that draft after Sent is
     stored.

4. Remote append option
   - Add an explicit path to append compose/reply drafts to remote Drafts.
   - Do not append automatically.

5. Tests and gates
   - Unit-test compose/reply headers.
   - Unit-test local draft-to-sent reconciliation.
   - Run `cargo fmt --check`, `cargo test`, clippy, and help checks.

## Checkpoint Target

Vivi can draft, reply, send, and locally verify a Sent copy for a controlled
message. Without a controlled live fixture, SMTP send and remote APPEND smoke
checks must be skipped and documented.

## Safety Stop

Do not send or remote-append real mail during this phase unless the target is a
controlled fixture explicitly selected for that purpose.

## Delivered Outputs

- `vivi compose` and `vivi reply` are available in the default binary.
- Compose drafts support To, Cc, Bcc, subject, body, and explicit
  `--append-remote`.
- Reply drafts are draft-first and include `In-Reply-To` and `References` when
  the original message has a Message-ID.
- Generated compose/reply messages include Date and Message-ID.
- `vivi send <path>` still requires an explicit `.eml` file and reconciles a
  successful send into local Sent.
- Sending a local Drafts message removes the local draft only after the Sent
  copy is stored.
- Remote Drafts APPEND is available only through explicit `--append-remote`.

## Correctness Pass

- Draft construction validates From and at least one To/Cc/Bcc recipient before
  local storage or remote APPEND.
- Send reconciliation is ordered as SMTP success, local Sent store, then local
  Drafts removal.
- Remote APPEND failures leave the local draft in place for retry.
- No live SMTP send or remote IMAP APPEND was attempted in this phase.
- Residual risk: remote Sent behavior is not live-verified and this phase does
  not automatically APPEND sent mail to the remote Sent folder.

## Verification

- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `git diff --check`
- `cargo run -- --help`
- `cargo run -- compose --help`
- `cargo run -- reply --help`
- `cargo run -- send --help`

## Poker Face Check

- Completion score: 89%.
- Largest gap: controlled live SMTP/IMAP fixture validation was intentionally
  skipped under the safety stop.
- Gate result: PASS for local implementation and tests; live provider behavior
  remains a documented follow-on.

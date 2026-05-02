# Write Phase 04: SMTP Send Baseline

## Interpreted Phase Problem

Vivi has existing raw SMTP send plumbing, but it is hidden behind the `outbox`
feature and therefore absent from the default/Homebrew-style binary. This phase
promotes explicit raw `.eml` sending into the default command surface while
leaving compose/reply and draft-first UX for Phase 05.

## Normalized Phase Spec

### Goal

Make outbound raw-message sending a supported default capability for the Vivi
binary.

### Inputs

- Existing `src/smtp.rs`.
- Existing feature-gated `vivi send <path>` command.
- Account auth, SMTP host/port/security config, and TLS behavior.
- Pharos/Bridge SMTP endpoint.

### Expected Outputs

- Default release includes `vivi send <path>`.
- Release/Homebrew workflow verifies the intended command surface.
- `vivi send <path>` sends only an explicit `.eml` file.
- SMTP send-time diagnostics include enough endpoint context to debug failures.
- Envelope extraction includes From, To, Cc, and Bcc recipients.
- Tests for envelope extraction and command-surface presence.

### Out Of Scope

- Compose/reply UX.
- Attachments beyond sending a complete raw `.eml`.
- Automatic agent sending.
- Sent folder reconciliation, except for existing local outbox behavior when the
  optional outbox watcher is enabled.
- Live send unless a controlled/self-addressed fixture is explicitly selected.

## Repo-Aware Phase Baseline

- `src/smtp.rs` and `Command::Send` are currently compiled only with the
  `outbox` feature.
- The release workflow builds with plain `cargo build --release`, so feature
  gated send support is not present in shipped artifacts.
- `src/smtp.rs` currently extracts From and To for the SMTP envelope, but Cc and
  Bcc recipients also need to be included.
- The existing `outbox` watcher can remain feature gated; Phase 04 only owns
  explicit raw send.

## Stage Graph

1. Default command surface
   - Remove the feature gate from `smtp` and `vivi send <path>`.
   - Keep auth/token/watch/compose/reply behind `outbox`.

2. SMTP behavior
   - Require an explicit `.eml` path.
   - Improve send failure diagnostics with host, port, and security context.
   - Include Cc/Bcc recipients in the envelope.

3. Release surface
   - Update the release/Homebrew test to verify `vivi --help` includes `send`.

4. Tests and gates
   - Unit-test envelope extraction for To/Cc/Bcc.
   - Unit-test CLI send command parsing.
   - Run `cargo fmt --check`, `cargo test`, clippy, and help checks.

## Checkpoint Target

The Homebrew-style `vivi` binary exposes `send`, validates explicit raw `.eml`
paths, and can send through SMTP when given a controlled message. Without a
controlled message fixture, live sending must be skipped and documented.

## Safety Stop

Do not send mail during this phase unless the target message is controlled and
self-addressed or otherwise explicitly approved.

## Phase Checkpoint

### Delivered Outputs

- Removed the default feature gate from `src/smtp.rs` and `vivi send <path>`.
- Kept auth/token/watch/compose/reply behind the optional `outbox` feature.
- Added explicit `.eml` path validation before `send` reads or transmits a file.
- Improved SMTP send failure diagnostics with host, port, and security context.
- Expanded SMTP envelope extraction to include To, Cc, and Bcc recipients.
- Updated the release/Homebrew formula test to assert that `vivi --help`
  includes `send`.
- Verified the default release build exposes `send`.

### Correctness Pass

- Checked that the default binary now exposes raw send without enabling
  `outbox`.
- Checked that Phase 05 surfaces remain gated: compose/reply/watch are still not
  part of the default command surface.
- Checked envelope behavior: To, Cc, and Bcc are included as SMTP recipients,
  and messages without any recipients fail before transport.
- Checked send safety: the command requires an explicit `.eml` file path before
  reading file bytes or sending.
- No live mail was sent during this phase.

### Verification Run

- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo run -- --help`
- `cargo run -- send --help`
- `cargo build --release`
- `target/release/vivi --help`

### Poker Face

- Self estimate: 90%.
- Evaluator mode: self-contained independent pass.
- Evaluator estimate: 88%.
- Largest remaining gap: no controlled/self-addressed live SMTP send was run.
- Verdict: cleared for Phase 04 completion.

### Gate Result

PASS. The default and release-style binary exposes explicit raw `.eml` sending,
SMTP envelope extraction covers To/Cc/Bcc, release workflow checks the command
surface, and no live mail was sent without an approved fixture.

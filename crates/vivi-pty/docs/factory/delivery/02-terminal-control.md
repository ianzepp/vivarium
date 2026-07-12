# Phase 2 delivery: complete raw terminal control

## Interpreted unit

Complete the harness-neutral terminal substrate beneath the existing Vivi PTY
daemon. Clients must be able to send literal UTF-8 text, arbitrary bytes, and
named key chords through distinct protocol operations; resize both the child
PTY and the terminal emulator; and obtain a bounded, revisioned terminal
snapshot suitable for operator recovery and later driver work.

The governing invariant is: **each terminal observation is an atomic view of
one daemon-owned session's process metadata and emulator state, while every
input path remains ordered through that session's single writer and every
terminal history remains bounded.**

## Normalized spec

### In scope

- A distinct raw-byte write request alongside the existing UTF-8 text write.
- Named key encoding with common navigation, editing, function, and control
  keys plus Shift, Alt, and Control modifiers.
- PTY resize propagation and emulator resize under the session registry lock.
- Visible text, formatted terminal bytes, cursor state, alternate-screen and
  input-mode flags, bounded scrollback metadata, screen revisions, and output
  sequence numbers in terminal snapshots.
- An atomic diagnostic snapshot containing daemon/protocol evidence, session
  process metadata, and the terminal snapshot.
- CLI commands for raw bytes, named keys, resize, and diagnostic snapshots.
- Deterministic fixture-TUI tests covering ANSI rendering, alternate screen,
  resize, Unicode, raw input, key encoding, and high-output bounds.
- Protocol tests for malformed parameters, limits, and the new method shapes.

### Explicit policy decisions

- `terminal.write` remains the text operation and writes UTF-8 bytes exactly as
  supplied. `terminal.write_bytes` accepts a JSON byte array and never
  performs text decoding or normalization.
- `terminal.key` accepts a key name or one Unicode scalar plus a list of
  `control`, `alt`, and `shift` modifiers. Unsupported combinations are typed
  invalid-parameter errors rather than guessed escape sequences.
- Supported named keys are Enter, Escape, Tab, Backspace, Space, arrows,
  Home, End, Insert, Delete, PageUp, PageDown, and F1 through F12. A single
  printable Unicode scalar is also a valid key without modifiers; Control
  chords are limited to ASCII control mappings.
- Resize validates against the existing 1..=500 column and 1..=200 row
  limits, updates the kernel PTY first, then updates the emulator. A resize of
  a terminal session is an invalid-state error.
- `output_sequence` increments once for each non-empty PTY read chunk and
  `screen_revision` increments once for each chunk processed by the emulator.
  Both start at zero and never decrease or reset during a session.
- Snapshots expose the current scrollback offset and the configured bounded
  scrollback limit. The emulator remains configured with the Phase 1 limit of
  2,000 rows; no unbounded raw-output buffer is added.
- Formatted contents are serialized as bytes so ANSI escape sequences and
  non-UTF-8 terminal data remain lossless.
- A diagnostic snapshot is captured while the registry and terminal locks are
  held in one request path. It is observational evidence only and does not
  create a second process or terminal ownership path.

### Out of scope

- Server notifications, waits, operation identifiers, replay, or leases
  (Phase 3).
- Harness-specific classifications or semantic submit/interrupt behavior
  (Phases 4-5).
- Human attachment, MCP, Fleet runtime bindings, authorization policy, and
  cross-project integration (later phases).
- Persistent terminal history or daemon recovery after process loss.

## Repo-aware baseline

- Target: the `vivi-pty` workspace member under `crates/vivi-pty`.
- Entrypoints: `src/main.rs` and `src/daemon.rs`; the client speaks the same
  length-prefixed JSON-RPC protocol over the project-scoped Unix socket.
- Current PTY ownership: `ManagedSession` owns the child, process group, PTY
  master, writer, VT100 parser, and stoppable output-drain thread.
- Current protocol: text-only `terminal.write` and a visible-text-only
  `terminal.snapshot`; resize and raw input are absent.
- Existing Phase 1 policy to preserve: process-group-only termination,
  bounded session/tombstone capacity, stale-socket protection, typed errors,
  and dedicated test files.
- Existing dependency support: `portable-pty::MasterPty::resize` and
  `vt100::Screen` already expose resize, formatted contents, cursor, mode, and
  alternate-screen state. No new dependency is required.

## Stage graph

```text
protocol input/snapshot types and key encoder
                  |
                  v
session terminal state + PTY resize/raw writes
                  |
                  v
daemon dispatch and CLI surfaces
                  |
                  v
fixture-TUI, protocol, bounds, and atomic-snapshot tests
```

The stages are sequential because the protocol shape, terminal-state locking,
and PTY ownership must agree before the client and tests can prove the
checkpoint. The key encoder is a local pure component but remains in the same
phase because its accepted operation shape is part of the raw terminal
contract.

## Implementation work

1. **Protocol and key contract**
   - Add raw-byte, key, resize, and diagnostic request/response types.
   - Add typed modifier/key encoding with explicit unsupported-input errors.
   - Extend terminal snapshots with revisions, modes, scrollback metadata, and
     lossless formatted bytes.
   - Add method dispatch names and stable invalid-parameter behavior.

2. **Terminal state and PTY control**
   - Replace the parser-only shared state with a locked terminal state carrying
     the parser and monotonic counters.
   - Increment counters in the output-drain path while processing each
     non-empty read chunk.
   - Add raw writes and validated resize; propagate resize to both PTY and
     emulator without changing process-group ownership.
   - Build snapshots from the locked state, including all visible mode and
     formatting evidence.

3. **Daemon and CLI surface**
   - Add registry methods for raw bytes, keys, resize, and diagnostics.
   - Keep all input operations serialized by the existing registry/session
     writer path.
   - Add CLI commands that expose the new operations without introducing a
     second socket or project-discovery model.

4. **Tests and documentation**
   - Add pure key-encoding and protocol round-trip tests.
   - Add PTY fixture tests for raw bytes, control keys, resize, alternate
     screen, Unicode, high output, revisions, and formatted snapshots.
   - Add diagnostic atomicity/shape coverage and update the crate README with
     the new operator commands.

## Checkpoints and gates

### Checkpoint target

A deterministic fixture TUI can be started under `vivi-pty`, driven through
raw bytes and named keys, resized, and inspected through a lossless formatted
snapshot. Snapshot revisions increase monotonically with PTY output; parser
history remains bounded; alternate-screen and mode evidence is visible; and a
diagnostic request reports process, protocol, and terminal evidence together.

### Gate

`PASS` requires all of the following:

- `cargo fmt --all --check` passes.
- `cargo clippy -p vivi-pty --all-targets -- -D warnings` passes.
- `cargo test --workspace` passes under an explicit timeout.
- Focused tests prove raw bytes are not UTF-8-normalized, key encodings are
  deterministic, resize reaches the child and emulator, and high output does
  not exceed the configured history bound.
- Process-group cleanup and project-scoping tests remain green.
- No new compatibility alias preserves the former standalone package names.

`FAIL` if input bytes are changed, resize updates only one side of the PTY
boundary, revisions can regress, snapshots mix sessions, output history grows
without a bound, or a raw operation bypasses the session writer and ownership
guards.

### Batching / split decision

Execute this unit as one sequential batch. Raw protocol operations, terminal
state, and snapshot evidence are one public substrate checkpoint; splitting
them would leave no honest way to validate the operator-visible contract.

### Release decision

`defer-release`: this milestone changes the experimental `vivi-pty` protocol
and CLI but does not meet the Codex Fleet canary gate for a product release.
Record the user-visible protocol change in the phase commit and revisit
versioning at the next release checkpoint.

## Validation

Run from the workspace root, each with an explicit timeout:

```sh
timeout 30s cargo fmt --all --check
timeout 60s cargo clippy -p vivi-pty --all-targets -- -D warnings
timeout 60s cargo test --workspace
```

Also run focused `vivi-pty` tests for key encoding, terminal control, and
diagnostic snapshots. If the host lacks `timeout`, use an equivalent bounded
process wrapper and record that substitution.

## Companion skill plan

- Correctness: audit raw-byte fidelity, lock ordering, revision monotonicity,
  resize propagation, and process-group scope before closeout.
- Read-only review and bonsai: inspect the protocol/daemon boundary for
  deferred risks without expanding into Phase 3 events or leases.
- Cleanliness: keep terminal state and key encoding out of the daemon
  dispatcher when natural module boundaries improve maintainability.
- Housekeeping: run formatting, clippy, workspace tests, test-boundary hygiene,
  and dependency checks on the changed tree.
- Polish: inspect each phase-modified primary source file serially and commit
  only directly related improvements before closing the phase.

## Open questions

None block this phase. Authorization, runtime-binding location, multi-session
role semantics, and release packaging remain deferred to the milestones that
own those decisions.

## Revision history

- 2026-07-12: Phase 2 delivery spec compiled from the rewritten project goal,
  Phase 1 delivery policy, and the current PTY/protocol implementation.

# Phase 1 delivery: runtime lifecycle hardening

## Interpreted unit

Make the existing daemon a bounded, explicit owner of long-running PTY
sessions. A session must have a monotonic lifecycle, terminate its complete
daemon-owned process group, release its output reader, and remain inspectable
as a bounded tombstone after exit. The daemon must clean up sessions during a
graceful shutdown and must never silently replace a live socket. Protocol
callers must be able to distinguish missing sessions, duplicate identifiers,
invalid state, invalid parameters, and resource-limit failures.

The governing invariant is: **every managed session has one daemon-owned PTY
process group, and every lifecycle path either keeps that ownership recorded or
terminates and reaps the whole group before releasing it.**

## Normalized spec

### In scope

- Explicit `running -> exited` and `running -> stopped` transitions.
- Idempotent inspection and stopping of terminal sessions.
- Process-group signaling, bounded graceful termination, forced cleanup, and
  child reaping.
- Bounded session capacity and bounded retained terminal tombstones.
- Session identifier validation and duplicate/tombstone behavior.
- Bounded VT100 history and start-request resource limits.
- Graceful daemon shutdown on SIGINT/SIGTERM, including owned-session cleanup.
- Stale Unix-socket replacement only when no listener is reachable.
- Typed JSON-RPC error codes for invalid parameters, missing sessions,
  conflicts, invalid state, resource limits, and internal failures.
- Concurrent multi-session lifecycle and process-cleanup tests.
- Moving Rust tests out of production files into dedicated test files while
  preserving private-module coverage.

### Explicit policy decisions

- Session identifiers are non-empty ASCII names of at most 128 bytes, using
  only letters, digits, `.`, `_`, and `-`.
- The registry admits at most 64 sessions total. Retained terminal sessions
  are tombstones; at most 32 tombstones are retained. Oldest tombstones are
  evicted when capacity is needed, so an evicted identifier may be reused.
  Live identifiers and retained tombstones reject duplicate starts.
- Terminal history is capped at 2,000 rows. Session starts accept at most
  500 columns by 200 rows, 128 argv entries, and 64 KiB of aggregate argv
  bytes.
- Stop sends SIGTERM to the session process group, waits up to 500 ms, then
  sends SIGKILL to that group and waits for the leader. A stop of an already
  terminal session does not signal it again.
- `SIGINT` and `SIGTERM` cause the daemon to stop accepting work, stop and reap
  all live sessions, remove its socket, and return. `Daemon` drop cleanup is a
  second safety net for tests and non-signal exits.
- The existing JSON-RPC transport remains unchanged. Error code values are
  stable constants in the protocol module; error messages remain diagnostic.

### Out of scope

- Raw byte/key APIs, resize propagation, scrollback APIs, or screen revisions
  beyond retaining a bounded current parser (Phase 2).
- Notifications, waits, operation IDs, idempotency records, or subscribers
  (Phase 3).
- Harness state, semantic drivers, leases, MCP, Fleet integration, packaging,
  and cross-repository changes.
- Persistent session storage or recovery after daemon process loss.

## Repo-aware baseline

- Historical target: the standalone `fleet-pty` repository. The completed
  implementation was subsequently recovered into Vivarium's `vivi-pty` crate.
- Stack: Rust 2024 binary/library crate using `portable-pty`, `serde_json`, and
  `vt100`; Unix-domain sockets are the current transport.
- Entrypoints: `src/main.rs` (`vivi-pty daemon`) and `src/daemon.rs`.
- Current ownership: `SessionRegistry` owns `ManagedSession` values in an
  unbounded `HashMap`; `ManagedSession` owns the child, PTY master, writer, and
  VT100 parser. `Child::kill` currently signals only the direct child.
- Current tests: inline unit modules in `src/daemon.rs` and
  `src/protocol.rs`; these will move to `src/daemon_test.rs` and
  `src/protocol_test.rs` using thin `#[path]` test wiring.
- Current validation surface: `cargo fmt`, `cargo clippy -- -D warnings`, and
  `cargo test`.

## Stage graph

```text
limits/errors and lifecycle model
        |
        v
process-group ownership and bounded reader cleanup
        |
        v
daemon signal shutdown and stale-socket behavior
        |
        v
dedicated tests, docs, formatting, and hygiene validation
```

Each stage is sequential because all later behavior depends on the session
registry and `ManagedSession` ownership model. No parallel write streams are
safe in this phase: `src/daemon.rs` and `src/protocol.rs` are shared contract
surfaces.

## Implementation work

1. **Protocol and registry policy**
   - Add named JSON-RPC error codes and a dispatch error type.
   - Map parameter, session, conflict, invalid-state, capacity, and internal
     failures without changing successful response shapes.
   - Add validation and capacity/tombstone bookkeeping to `SessionRegistry`.
   - Keep terminal states monotonic and refresh transitions explicit.

2. **Owned process cleanup**
   - Capture the PTY process-group leader after spawn.
   - Signal only the negative process-group ID for stop and shutdown.
   - Add bounded graceful/forced termination and guaranteed child reaping.
   - Make the PTY output reader stoppable and joinable so evicted sessions do
     not leave detached reader threads.

3. **Daemon shutdown**
   - Run the listener with signal-aware shutdown handling.
   - Stop accepting new sessions once shutdown begins, clean all live sessions,
     and remove the socket.
   - Preserve stale-socket protection and test both live and stale paths.

4. **Tests and documentation**
   - Move existing inline tests to dedicated files.
   - Add focused tests for limits, typed errors, tombstone eviction, repeated
     stop/inspect, concurrent sessions, descendant cleanup, stale sockets, and
     reader shutdown.
   - Update README only where lifecycle and resource behavior is user-visible.

## Checkpoints and gates

### Checkpoint target

Repeated tests can start, inspect, stop, and naturally exit multiple sessions;
stopping a session removes its descendant processes; duplicate and invalid
requests receive typed errors; tombstones and parser history stay bounded; and
daemon shutdown leaves no owned process groups or socket behind.

### Gate

`PASS` requires all of the following:

- The targeted lifecycle and cleanup tests pass.
- No test demonstrates cross-session state or input contamination.
- A stop/shutdown test proves the descendant process group is gone, not just
  the direct shell process.
- `cargo fmt --check`, `cargo clippy -- -D warnings`, and the full test suite
  pass under explicit timeouts.
- The changed production tree contains no inline test bodies or newly
  introduced test-only panic/unwrap patterns.

`FAIL` if any process remains, the socket is replaced while reachable, a
  terminal request can bypass the typed state/identity rules, or a limit is
  unbounded. `NEEDS FURTHER REVIEW` if platform behavior prevents proving
  process-group ownership on the supported Unix target.

### Batching / split decision

Execute this unit as one sequential batch. The lifecycle, process-group, and
shutdown changes share `ManagedSession` ownership and cannot be validated
independently without creating false confidence. Phase 2 is the named split
boundary because it expands the public terminal contract rather than hardening
the existing lifecycle.

## Validation

Run from the repository root, each with an explicit timeout:

```sh
timeout 30s cargo fmt --check
timeout 60s cargo clippy -- -D warnings
timeout 60s cargo test
```

Also perform a manual or integration-level daemon check using a temporary
`VIVI_PTY_SOCKET`: start a shell that launches a background child, stop the
session, verify both processes are gone, terminate the daemon, and verify the
socket path is removed. If the host lacks the `timeout` utility, use the
repository's equivalent bounded process wrapper and record that substitution.

## Companion skill plan

- Factory correctness mode: audit process ownership, signal scope, lifecycle
  monotonicity, lock scope, and race behavior before closeout.
- Read-only review and bonsai: inspect the changed registry/protocol surface
  for deferred risks without expanding this phase.
- Cleanliness: reshape dispatch and session cleanup only when behavior is
  preserved and the phase boundary remains intact.
- Housekeeping: enforce the Rust production/test boundary, format, lint, and
  documentation consistency on changed files.
- Polish: inspect each phase-modified primary source file serially and commit
  only directly related improvements.

## Open questions

No blocking questions for Phase 1. The exact declarative-driver format,
event-replay persistence, and release packaging targets remain deferred to the
phases that own those decisions.

## Revision history

- 2026-07-12: Initial Phase 1 delivery spec compiled from `GOAL.md`,
  `docs/BRIEF.md`, and the current MVP source.
- 2026-07-12: Implementation extracted PTY supervision into `src/session.rs`,
  moved tests to dedicated companions, and added `tests/hygiene.rs`.

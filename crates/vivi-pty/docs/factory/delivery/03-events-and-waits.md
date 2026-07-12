# Phase 3 delivery: events, waits, and operation correlation

## Interpreted unit

Give local clients a durable request/response and notification contract for
reacting to PTY changes without polling terminal snapshots or guessing whether
an input operation took effect. The daemon will retain a bounded ordered event
history per session, stream subscribed batches over long-lived Unix-socket
connections, report lag with a diagnostic snapshot, and replay completed
operations by explicit operation ID.

The governing invariant is: **each session's event sequence is monotonic and
bounded, each operation ID has one immutable outcome for one request
fingerprint, and reconnecting clients can recover from a bounded event gap by
using the daemon's current diagnostic snapshot.**

## Normalized spec

### In scope

- Session lifecycle, screen, and operation events with per-session sequence
  numbers and bounded replay history.
- Long-lived client connections with `session.subscribe` and
  `session.unsubscribe`; server `session.event` notifications carry ordered
  batches rather than requiring client polling.
- Bounded event history with explicit lag detection and a diagnostic snapshot
  attached to lagged notifications.
- `session.wait` predicates for lifecycle state, screen revision, and event
  sequence with bounded timeouts.
- Request-level operation IDs, echoed response correlation, bounded completed
  operation replay, and conflict rejection when an ID is reused for a different
  request.
- Disconnect/reconnect behavior: subscriptions are connection-local, while
  event history and operation records remain daemon-owned and bounded.
- Deterministic tests for ordering, lag, waits, idempotent retry, conflicts,
  disconnects, and concurrent event producers.

### Explicit policy decisions

- Event history retains the newest 256 events per session. A subscriber whose
  cursor is older than the retained window receives `lagged: true` and a
  current diagnostic snapshot; the batch's latest sequence becomes the new
  cursor.
- A connection may hold one subscription at a time. Subscription state is
  connection-local and disappears on disconnect; the session history does not.
- Notifications use framed JSON-RPC-shaped messages with method
  `session.event`; normal request responses remain correlated by JSON-RPC ID.
- `session.wait` accepts one predicate (`state`, `screen_revision`, or
  `event_sequence`) and a timeout capped at 30 seconds. Completion returns a
  diagnostic snapshot; expiry is a typed timeout error.
- Operation IDs are non-empty ASCII identifiers capped at 128 bytes. The
  daemon retains 256 completed outcomes with the request fingerprint and
  response. Reusing an ID with the same fingerprint replays the response with
  the new JSON-RPC request ID; reuse with a different fingerprint is a conflict.
- Operation outcomes are emitted once as events for new operations; replaying a
  completed operation does not duplicate the event.
- Event publication never owns or signals a process. PTY output, lifecycle
  transitions, and operation completion publish into the bounded event hub;
  process-group ownership remains in `ManagedSession`.

### Out of scope

- Harness drivers, semantic state classification, leases, MCP, Fleet
  integration, authorization, and persistent event storage.
- Cross-daemon event federation or public TCP transport.
- Exactly-once delivery across daemon restart; reconnect recovery is bounded
  to the live daemon's retained history and current diagnostic snapshot.

## Repo-aware baseline

- Phase 2 supplies raw writes, key encoding, resize, bounded VT100 state,
  screen/output revisions, and diagnostic snapshots.
- `serve_client` currently handles one request at a time on a blocking framed
  stream; `client::call` opens one short-lived connection per request.
- `SessionRegistry` serializes session operations under a mutex, while the PTY
  output drain runs independently and updates `TerminalState` under its own
  lock.
- The protocol already has typed session errors and a version constant. No
  external compatibility alias or new dependency is required.

## Stage graph

```text
event protocol + bounded event hub
              |
              v
PTY/lifecycle/operation publication
              |
              v
long-lived subscriptions + lag recovery
              |
              v
wait predicates + operation replay/conflicts
              |
              v
ordering, timeout, reconnect, and concurrency tests
```

The stages are sequential because notification payloads, event cursors, and
operation records must share one sequence and locking model before waits and
reconnect behavior can be tested honestly.

## Implementation work

1. **Protocol and event hub**
   - Add event kinds, event batches, subscription, wait, operation-ID, and
     notification types.
   - Add a bounded per-session event hub with monotonic sequence assignment,
     cursor reads, and lag detection.
   - Add bounded operation records keyed by validated ID and request
     fingerprint.

2. **Publication and registry integration**
   - Publish screen events from the PTY drain and lifecycle events from session
     refresh/start/stop paths.
   - Publish one operation outcome after a new correlated request completes.
   - Add registry helpers for event batches, waits, and operation replay while
     preserving Phase 1/2 process and terminal locks.

3. **Connection service**
   - Keep client streams alive with bounded read timeouts while a subscription
     is active.
   - Send ordered `session.event` notifications and advance cursors only after
     a batch is written.
   - Return a typed lagged batch with current diagnostic evidence and handle
     subscribe/unsubscribe/wait requests without a second transport.

4. **Tests and documentation**
   - Test sequence ordering, bounded history, lag snapshots, wait completion and
     expiry, operation replay/conflicts, disconnect/reconnect, and concurrent
     screen/lifecycle publication.
   - Update README and protocol tests with the long-lived event examples.

## Checkpoints and gates

### Checkpoint target

A client can subscribe over one persistent socket, receive ordered lifecycle
and screen notifications, detect a bounded-history gap and recover from the
attached diagnostic snapshot, wait for a state/revision predicate with a
bounded timeout, and retry a completed operation without duplicating its effect
or event.

### Gate

`PASS` requires:

- `cargo fmt --all --check` passes.
- `cargo clippy -p vivi-pty --all-targets -- -D warnings` passes.
- `cargo test --workspace` passes under an explicit timeout.
- Tests prove event ordering, bounded replay and explicit lag, wait timeout,
  operation replay/conflict, and disconnect/reconnect behavior.
- Existing process-group, raw-terminal, project-scoping, and hygiene tests
  remain green.

`FAIL` if events reorder or grow without a bound, a wait hangs beyond its cap,
an operation retry repeats a side effect, a subscriber silently misses a gap,
or notification handling changes process ownership.

### Batching / split decision

Execute as one sequential batch. Events, waits, and operation records form one
correlation contract; splitting them would permit a client to observe events
without a trustworthy operation outcome or recovery path.

### Release decision

`defer-release`: this changes the experimental local protocol and adds a
long-lived client behavior, but the Codex Fleet canary gate remains pending.

## Validation

```sh
timeout 30s cargo fmt --all --check
timeout 90s cargo clippy -p vivi-pty --all-targets -- -D warnings
timeout 120s cargo test --workspace
```

Also run focused `vivi-pty` event, wait, and operation tests with explicit
timeouts and inspect the framed notification stream in a temporary socket
fixture.

## Companion skill plan

- Correctness: audit event ordering, cursor advancement, timeout cancellation,
  operation replay, and lock ordering.
- Read-only review and bonsai: inspect notification and registry boundaries for
  deferred risks without entering semantic driver work.
- Cleanliness: keep event storage, connection protocol, and wait predicates in
  named modules rather than growing the dispatcher.
- Housekeeping: enforce test placement, formatting, Clippy, workspace tests,
  and bounded-resource checks.
- Polish: inspect each Phase 3 primary source file serially before closeout.

## Open questions

None block this phase. Authorization, leases, runtime binding, and durable
event persistence remain deferred to later milestones.

## Revision history

- 2026-07-12: Phase 3 delivery spec compiled from the rewritten project goal,
  completed Phase 2 contract, and the current daemon/client architecture.

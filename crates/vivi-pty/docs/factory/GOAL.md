# Vivi PTY product factory goal

## Run status

- Status: active factory execution
- Current phase: Phase 2 — Complete raw terminal control
- Completed phases: MVP baseline, Phase 1
- Repository migration: complete; implementation now lives in the Vivarium
  workspace as the separate `vivi-pty` crate and binary
- Pending phases: 2-10
- Delivery specs: create under `docs/factory/delivery/` before each phase
- Checkpoint policy: verify, review, polish, and commit every phase independently
- Release policy: evaluate a version checkpoint after every user-visible protocol
  or CLI phase; target the first product release after Phase 8

## Goal boundary

Build `vivi-pty` into the process-control layer for Fleet-managed terminal agent
harnesses. The product must own agent processes and PTYs, translate normalized
operations into harness-specific terminal interactions, expose reliable local
automation and operator surfaces, and replace tmux for selected Fleet Hands
without requiring a native API from the underlying harness.

The first product-completion target is a Codex Hand operating through Vivi PTY
for repeated real Fleet cycles. Pi and OpenCode then validate that the common
contract is genuinely harness-neutral.

## Ground truth

- `docs/BRIEF.md` defines the product thesis, runtime layers, submission
  contract, and adoption signal.
- `README.md` describes the working MVP surface.
- Commits `57e27db` and `fb3daa6` establish the Unix-socket daemon, PTY process
  ownership, terminal writes, and rendered VT100 snapshots.
- Source commit `c72e123` in the former `fleet-pty` repository completed Phase 1
  before the implementation moved into this workspace.
- The current protocol supports daemon discovery, session lifecycle, literal
  terminal writes, and terminal snapshots.

## Architectural invariants

1. Vivi PTY exclusively owns every managed agent process and PTY.
2. tmux-backed and Vivi PTY-backed Hands may coexist during migration, but
   they never own the same process.
3. Vivi remains work truth; Vivi PTY reports process and interaction truth.
4. The terminal is the universal harness contract. Native harness APIs are
   optional optimizations, never requirements.
5. Semantic operations are guarded, serialized, correlated, and evidence-backed.
6. Ambiguous terminal state is reported as unknown rather than converted into
   false certainty.
7. Automated input and human input cannot race; interactive human control
   requires an exclusive expiring lease.
8. MCP is a facade over the daemon protocol, not the process owner or internal
   protocol.
9. Process termination and recovery affect only the daemon-owned process group.
10. Compatibility shims are not a goal. Fleet configuration moves cleanly to a
    canonical runtime binding when the adoption gate is met.
11. One Vivi mailspace owns one Vivi PTY daemon endpoint; runtime sessions use
    the same canonical role identities as project-local mail.
12. `vivi` and `vivi-pty` are separate binaries and processes even though they
    share this repository and project-discovery code.

## Non-goals

- Reimplementing all tmux features.
- Owning Fleet work assignment, task lifecycle, or Vivi state.
- Browser UI, general-purpose terminal multiplexing, or arbitrary shell hosting.
- Direct unauthenticated network exposure.
- Cloud scheduling or distributed orchestration inside the daemon.
- Preserving `tmux_target` as a second active control path for migrated Hands.
- Requiring Codex, Pi, or OpenCode native APIs.

## Product acceptance

The factory goal is complete when:

- A Fleet Mind can create, inspect, wake, observe, interrupt, restart, and stop a
  Codex Hand exclusively through Vivi PTY's normalized surface.
- Repeated Fleet cycles distinguish waiting, submitting, running, approval,
  completion, failure, stopped, and unknown states with recorded evidence.
- Submission retries cannot silently duplicate a turn, and concurrent writers
  cannot corrupt terminal input.
- An operator can attach, observe, acquire control, interact, and detach without
  racing automation.
- The MCP facade exposes the supported normalized operations without owning
  session lifetime.
- Fleet supports a canonical Vivi PTY runtime binding and no tmux interaction
  for migrated Hands.
- Codex, Pi, and OpenCode drivers pass the shared conformance suite, with
  capability differences reported explicitly.
- Restart, daemon recovery, stale sockets, process-group cleanup, bounded
  history, and diagnostic snapshots have automated coverage and operator docs.

## Stop conditions

Pause the factory when:

- A phase would require weakening an architectural invariant to pass validation.
- A harness cannot be operated through its terminal without credentials,
  destructive actions, or a human-only trust decision not represented by the
  protocol.
- Fleet integration requires changing a live external contract that has not
  been authorized.
- A dependency cannot preserve process-group ownership, terminal fidelity, or
  bounded resource behavior.
- Validation shows lost input, duplicated submissions, cross-session writes, or
  orphaned process groups.

Record ordinary harness ambiguity, unsupported capabilities, and recoverable
terminal quirks as driver evidence or deferred findings; they are not automatic
stop conditions.

## Phase set

Each numbered item is one factory phase and one delivery-sized unit. Factory
must save a delivery spec before implementing it and close the phase with its own
verification, review, polish pass, checkpoint decision, and cohesive commit.

### Phase 1 — Runtime lifecycle hardening

**Outcome:** The daemon is a trustworthy owner of many long-running PTY process
groups rather than a successful prototype.

**Scope:**

- Explicit session lifecycle and transition rules.
- Process-group termination, wait, and orphan prevention.
- Daemon shutdown behavior and stale-socket handling.
- Session identifier validation and duplicate/tombstone policy.
- Bounded output/history memory and resource limits.
- Typed protocol errors for conflicts, invalid state, and missing sessions.
- Concurrency and multi-session lifecycle tests.

**Checkpoint:** Stress tests repeatedly start, exit, stop, and concurrently
inspect multiple sessions without leaked processes, unbounded memory, or
cross-session state.

**Depends on:** MVP baseline.

### Phase 2 — Complete raw terminal control

**Outcome:** Vivi PTY provides the full harness-neutral terminal substrate
needed by higher-level drivers and operator recovery.

**Scope:**

- Raw byte writes distinct from UTF-8 text writes.
- Named key encoding and key chords.
- Terminal resize propagated to both PTY and emulator.
- Visible screen, bounded scrollback, cursor, modes, and formatted snapshot.
- Monotonic screen revision and output sequence numbers.
- Atomic diagnostic snapshot containing process, protocol, and terminal evidence.
- ANSI, alternate-screen, resize, Unicode, and high-output tests.

**Checkpoint:** A fixture TUI can be driven using raw keys, resized, and read
back deterministically without harness-specific code.

**Depends on:** Phase 1.

### Phase 3 — Events, waits, and operation correlation

**Outcome:** Clients can react to state and screen changes without polling or
guessing whether an operation took effect.

**Scope:**

- Server notifications over long-lived framed-protocol connections.
- Per-session ordered event stream and sequence numbers.
- Subscribe/unsubscribe with bounded subscriber queues and lag recovery.
- Screen, process, operation, and lifecycle event types.
- `session.wait` predicates, timeouts, and cancellation.
- Operation identifiers, outcomes, and bounded replay/idempotency records.
- Reconnect and missed-event snapshot behavior.

**Checkpoint:** Tests prove ordered events, explicit lag detection, correlated
operation results, and deterministic wait completion across concurrent clients.

**Depends on:** Phases 1-2.

### Phase 4 — Driver framework and generic semantic runtime

**Outcome:** Harness policy is isolated behind a capability-aware driver
contract, and the generic driver proves normalized semantics without being
Codex-shaped.

**Scope:**

- Driver registry, lifecycle, capabilities, and configuration.
- Evidence and confidence schema for classifications.
- Normalized semantic states from the project brief.
- Serialized per-session action queue and expected-state guards.
- Semantic submit, interrupt, approve/reject, restart, and raw fallback actions.
- Generic terminal driver with conservative classifications.
- Reusable fake-harness fixtures and driver conformance suite.

**Checkpoint:** The generic fixture completes guarded submit, interrupt,
approval, restart, unknown-state, and error flows through normalized operations.

**Depends on:** Phase 3.

### Phase 5 — Codex driver vertical slice

**Outcome:** Codex can be operated reliably through normalized Vivi PTY
semantics with its terminal quirks contained inside one driver.

**Scope:**

- Codex startup, trust, waiting, submitting, running, completed, error, and
  unknown recognition with bounded evidence.
- Acknowledged submission state machine with composer receipt and settle policy.
- Codex-specific Enter, interrupt, approval, and restart behavior.
- Duplicate-submission prevention and expected-revision guards.
- Fixture recordings plus live opt-in Codex integration tests.
- Comparison against Fleet's current pane classification and doorbell scenarios.

**Checkpoint:** Repeated live Codex turns submit once, transition correctly,
return to waiting, survive approval/error/restart paths, and require no
Codex-specific decisions from the caller.

**Depends on:** Phase 4.

### Phase 6 — Operator attachment and exclusive control leases

**Outcome:** A human can observe and recover a managed terminal without tmux and
without racing automation.

**Scope:**

- Read-only attach client with initial snapshot and incremental updates.
- Terminal resize propagation from the active attachment.
- Exclusive expiring control lease with acquire, renew, release, and revocation.
- Automation guards while a human holds the lease.
- Detach/reconnect behavior and lag recovery.
- Operator-visible session, driver, state, and lease status.

**Checkpoint:** Concurrent-client tests prove that read-only observers cannot
write, one controller can interact, automation is blocked predictably, and
control recovers after disconnect or lease expiry.

**Depends on:** Phases 3-4. May follow Phase 5 to exercise a real Codex TUI.

### Phase 7 — MCP facade and automation client contract

**Outcome:** Top-level LLMs can operate Vivi PTY through convenience tools
without coupling daemon lifetime or protocol evolution to MCP.

**Scope:**

- Separate stdio MCP server process backed by the Unix-socket client library.
- Tools for daemon/session inspection, lifecycle, semantic actions, waits,
  snapshots, diagnostics, and lease-aware recovery.
- Compact outputs with optional bounded screen evidence.
- Protocol version and capability negotiation.
- MCP disconnect/restart tests proving managed sessions survive.
- Tool documentation and example Fleet calls.

**Checkpoint:** An MCP client can discover and control an existing Codex
session, restart its MCP server, reconnect, and continue with no session loss.

**Depends on:** Phases 3-6.

### Phase 8 — Fleet integration and tmux replacement gate

**Outcome:** Fleet can run selected Hands entirely through Vivi PTY while
tmux-backed Hands coexist only as a migration backend.

**Scope:**

- Canonical Fleet runtime endpoint and session binding.
- Fleet sensor integration using normalized state and evidence.
- Doorbell, wake-on-mail, recovery, reinit, and process-liveness operations
  routed through Vivi PTY.
- Clean runtime selection between tmux and Vivi PTY at the Hand boundary.
- Removal of `tmux_target` use for migrated Hands; no shared ownership path.
- End-to-end Fleet cycle fixtures and live canary instructions.
- Migration, rollback, diagnostics, and operator documentation.

**Checkpoint:** A canary Codex Hand completes repeated real task/wake/idle,
approval, interruption, restart, and completion cycles with no tmux commands or
pane scraping. Evaluate and prepare the first product release.

**Depends on:** Phases 5-7. Cross-repo work with the canonical Fleet repository
must receive its own delivery spec and explicit repo boundary.

### Phase 9 — Pi and OpenCode drivers

**Outcome:** Two additional harnesses validate that the product contract is
terminal-native and not accidentally Codex-specific.

**Scope:**

- Pi driver and capabilities.
- OpenCode driver and capabilities.
- Harness-specific state evidence and terminal action policies.
- Shared conformance suite across generic, Codex, Pi, and OpenCode.
- Explicit unsupported-capability behavior rather than compatibility shims.
- Live opt-in integration tests and fixture recordings.

**Checkpoint:** All drivers pass shared lifecycle and safety conformance; each
harness's differences remain inside its driver and capability report.

**Depends on:** Phases 4-7. May run after the Phase 8 Codex adoption checkpoint.

### Phase 10 — Operationalization and product release

**Outcome:** Vivi PTY is installable, observable, recoverable, and maintainable
as a normal local service.

**Scope:**

- Package the separate `vivi` and `vivi-pty` binaries together; keep MCP as a
  thin companion process without introducing a second PTY-owning daemon.
- Service installation and lifecycle for supported local platforms.
- Structured logs, health checks, diagnostic bundles, and version reporting.
- Socket permissions, local authorization assumptions, and SSH remote guidance.
- Compatibility/version policy for daemon protocol and drivers.
- Performance, soak, crash-recovery, and resource-bound testing.
- Complete operator, driver-author, protocol, migration, and troubleshooting docs.
- Release notes and versioned product release.

**Checkpoint:** A clean host can install, start, exercise, diagnose, restart, and
remove Vivi PTY using documented procedures; the release passes soak and
recovery gates.

**Depends on:** Phases 8-9.

## Production ledger

| Unit | Status | Delivery spec | Dependency note |
| --- | --- | --- | --- |
| Runtime lifecycle hardening | Complete | `delivery/01-runtime-lifecycle.md` | MVP |
| Complete raw terminal control | Pending | `delivery/02-terminal-control.md` | Phase 1 |
| Events, waits, and correlation | Pending | `delivery/03-events-and-waits.md` | Phases 1-2 |
| Driver framework and generic runtime | Pending | `delivery/04-driver-framework.md` | Phase 3 |
| Codex driver vertical slice | Pending | `delivery/05-codex-driver.md` | Phase 4 |
| Operator attachment and leases | Pending | `delivery/06-attachment-and-leases.md` | Phases 3-5 |
| MCP facade | Pending | `delivery/07-mcp-facade.md` | Phases 3-6 |
| Fleet integration | Pending | `delivery/08-fleet-integration.md` | Phases 5-7 |
| Pi and OpenCode drivers | Pending | `delivery/09-harness-expansion.md` | Phases 4-7 |
| Operationalization and release | Pending | `delivery/10-product-release.md` | Phases 8-9 |

## Factory execution policy

For every phase:

1. Confirm dependencies and repo boundaries.
2. Compile and save the named delivery spec.
3. Implement only that delivery-sized unit.
4. Run targeted tests plus repository-wide formatting, lint, and tests.
5. Run correctness review against the phase checkpoint and global invariants.
6. Run cleanliness and bounded housekeeping on changed implementation surfaces.
7. Run the required per-file polish loop over primary modified source files.
8. Record deferred findings without silently expanding the phase.
9. Evaluate the checkpoint and release/version significance.
10. Commit the coherent phase and update this ledger before continuing.

The factory may continue autonomously from one passing phase to the next. It
must stop on a listed stop condition, a failed checkpoint, an authorization
boundary, or a required product decision that cannot be inferred from this goal
and repository evidence.

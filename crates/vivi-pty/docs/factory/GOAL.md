# Goal: Vivi PTY project runtime

## Summary

Make `vivi-pty` the project-scoped live-runtime companion to `vivi`.
Vivarium provides the canonical project, mailspace, and role identities;
`vivi-pty` separately owns the processes and pseudo-terminals bound to those
identities. Fleet is the first consumer and must be able to replace tmux pane
control with a reliable local RPC surface without merging durable work truth
and ephemeral terminal truth into one process.

The standalone prototype, runtime-lifecycle phase, and raw-terminal phase are
complete. Active work continues in this Vivarium workspace with ordered events
and operation correlation as the next useful milestone.

## Problem

- Fleet currently uses two correlated but independently addressed surfaces:
  Vivi identities for durable work and tmux panes for live agent interaction.
  The duplicated addressing and pane scraping make wake, observation,
  submission, and recovery fragile.
- A machine controller needs explicit process lifecycle, terminal state,
  keystroke, resize, wait, and diagnostic operations while preserving terminal
  compatibility across Codex, Pi, OpenCode, and future harnesses.
- A global PTY daemon would expose unrelated Fleets to one shared control
  plane. Runtime ownership must instead follow Vivi's project boundary.
- Combining mail storage and PTY supervision in one process would couple
  durable coordination availability to a more failure-prone and privileged
  runtime subsystem.

## Goals

- Ship `vivi` and `vivi-pty` from one Vivarium workspace as separate
  binaries and processes with a deliberately narrow shared project model.
- Run one Vivi PTY daemon endpoint per discovered Vivi mailspace.
- Use Vivi's canonical role identity, such as `hand-1` or `head-cto`, as
  the default runtime session identity rather than maintaining an independent
  pane-address namespace.
- Own each managed harness process, process group, PTY, terminal emulator,
  ordered input stream, and bounded runtime history.
- Expose raw recovery operations and normalized, evidence-backed harness
  operations over a versioned local protocol.
- Give Fleet and an MCP facade stable client APIs without making either one
  responsible for daemon or agent-process lifetime.
- Provide safe human observation and exclusive interactive control without
  racing automation.
- Replace tmux completely for explicitly migrated Fleet roles while allowing
  unmigrated roles to remain tmux-backed during rollout.
- Validate the common contract against Codex first, then Pi and OpenCode.

## Non-goals

- Move mail, task, need, want, or assignment lifecycle into `vivi-pty`.
- Let terminal observations implicitly complete, reopen, or reroute Vivi work.
- Run `vivi` mail operations inside the PTY daemon.
- Reimplement tmux as a general terminal multiplexer.
- Provide a browser UI, cloud scheduler, or unauthenticated network service.
- Require harness-native APIs; the terminal remains the universal contract.
- Preserve `tmux_target` as an alternate control path for a role after that
  role has migrated.
- Preserve the standalone `fleet-pty` package or protocol names as
  compatibility aliases.

## Ground Truth Researched

- `Cargo.toml`: Vivarium is a Rust workspace containing the original
  `vivarium` package and the new `crates/vivi-pty` member.
- `src/mailspace.rs`: `Mailspace::discover` owns canonical project-root
  discovery, `.vivi` location, mailspace name, and configured identities.
- `crates/vivi-pty/src/main.rs`: the companion already reuses
  `Mailspace::discover`, accepts `--project`, and defaults to
  `.vivi/vivi-pty.sock`.
- `crates/vivi-pty/src/daemon.rs` and `src/session.rs`: the recovered
  baseline already implements a framed JSON-RPC Unix-socket daemon, bounded
  sessions and tombstones, process-group ownership, graceful shutdown, literal
  terminal writes, and rendered VT100 snapshots.
- `crates/vivi-pty/docs/factory/delivery/01-runtime-lifecycle.md`: Phase 1
  records the completed lifecycle, limits, cleanup, and typed-error contract.
- `crates/vivi-pty/docs/factory/delivery/02-terminal-control.md`: Phase 2
  records the completed raw-terminal protocol, emulator, and diagnostic
  contract.
- Standalone source commit `c72e123`: the last complete implementation point
  before migration into Vivarium.
- Vivarium commit `eb9da73`: the recovered implementation and project-scoped
  endpoint entered this workspace.
- User decision in this session: Vivi PTY belongs in the Vivarium project but
  remains a separate `vivi-pty` binary.

## Reference Packet

Before changing the runtime, inspect:

- `crates/vivi-pty/docs/BRIEF.md`: product thesis, terminal contract,
  normalized states, and submission model.
- `crates/vivi-pty/docs/factory/delivery/01-runtime-lifecycle.md`: completed
  lifecycle policy that later work must preserve.
- `crates/vivi-pty/src/protocol.rs`: current public wire types, framing,
  limits, and error codes.
- `crates/vivi-pty/src/daemon.rs`: request dispatch and session registry.
- `crates/vivi-pty/src/session.rs`: PTY, process-group, reader, and terminal
  ownership.
- `crates/vivi-pty/src/main.rs`: CLI and project/socket discovery.
- `src/mailspace.rs`: the shared project and identity authority.
- `../fleet/references/runtime-config.md`,
  `../fleet/scripts/fleet-sensors.py`, and
  `../fleet/scripts/fleet-doorbell.sh`: current Fleet bindings, observation,
  and wake behavior that the eventual integration must replace.

## Constraints And Invariants

1. `vivi` owns durable communication and work truth. `vivi-pty` owns
   ephemeral process and interaction truth.
2. The binaries share repository code and project identity, not process
   lifetime, mutable runtime state, or failure fate.
3. One canonical Vivi mailspace maps to one daemon endpoint. Every session is
   scoped to that mailspace.
4. Project scoping is not authorization. Before Fleet adoption, the control
   surface must define and test who may inspect, write, lease, start, and stop
   sessions; a discoverable socket path alone is insufficient.
5. Every managed session has one daemon-owned process group and PTY. Stop,
   restart, shutdown, and failure paths must terminate and reap only that owned
   group.
6. One session has one ordered input queue. Semantic submissions are
   correlated and cannot silently duplicate on retry.
7. Terminal state is bounded and monotonic where ordered: screen revisions,
   output/event sequences, operation outcomes, tombstones, and replay windows.
8. Ambiguous harness state is `unknown` with evidence and confidence, never a
   fabricated definitive state.
9. Automated input and human input cannot race. Human writes require an
   exclusive, expiring control lease.
10. Raw terminal operations remain available as guarded recovery tools, but
    harness-specific choices belong in drivers.
11. MCP and Fleet are protocol clients. They never own the managed child
    process or bypass daemon guards.
12. A role is controlled by tmux or Vivi PTY, never both. Migration is a clean
    runtime-binding switch per role.
13. Existing `vivi` mail and provider behavior must remain usable when
    `vivi-pty` is absent, stopped, broken, or not installed.
14. No compatibility shim is added for the former `fleet-pty` names unless a
    real external contract is identified and explicitly approved.

## Architecture Direction

### Workspace and ownership

- `vivarium` remains the reusable library and `vivi` CLI for durable mail
  and project work.
- `crates/vivi-pty` remains an independently testable library and
  `vivi-pty` binary.
- Shared APIs should expose only project discovery, canonical mailspace
  identity, configured role lookup, and runtime binding data. PTY code must not
  reach through that seam into mail storage or provider internals.
- If the shared seam grows beyond a narrow module, extract a small workspace
  crate rather than making the two binaries mutually aware of their internals.

### Project runtime

- `vivi-pty --project <root> daemon` owns the runtime for exactly that
  mailspace.
- The default local endpoint is derived from that mailspace. Explicit
  `--socket` and `VIVI_PTY_SOCKET` overrides remain operational escape
  hatches, not a second identity model.
- Runtime metadata may be recorded beneath `.vivi`, but durable mail and work
  records remain independent of daemon availability.
- A role identity is the canonical lookup key. Additional opaque session IDs
  are allowed only for a demonstrated multi-session-per-role requirement.

### Runtime layers

1. PTY supervisor: process groups, PTY I/O, resize, exit, and cleanup.
2. Terminal model: current screen, cursor, modes, bounded scrollback, revisions,
   and diagnostic evidence.
3. Operation/event core: ordered writes, operation IDs, waits, subscriptions,
   replay bounds, and leases.
4. Harness drivers: capabilities, classifications, evidence, and guarded
   semantic actions.
5. Service clients: CLI, attachment client, MCP facade, and Fleet integration.

The framed local protocol remains transport-independent. Unix sockets are the
supported local transport; remote operation initially uses SSH forwarding or
remote command execution rather than a public TCP listener.

## Supporting Skills

- `delivery`: lower each milestone into a repo-aware delivery specification
  before implementation.
- `factory`: execute the milestone sequence with independent validation and
  commits when autonomous continuation is authorized.
- `correctness`: review process ownership, concurrency, ordering, retries,
  cleanup, and state-transition invariants.
- `cleanliness`: preserve the narrow Vivi/Vivi PTY boundary as the runtime
  gains drivers and client surfaces.
- `housekeeping`: enforce formatting, test placement, dependency hygiene, and
  repository validation.
- `poker-face`: audit each milestone against its promised operator-visible
  behavior before advancing.

## Implementation Shape

### Completed baseline

- MVP: Unix-socket RPC, session lifecycle, literal writes, and VT100 snapshots.
- Runtime lifecycle hardening: process-group cleanup, bounded resources,
  tombstones, graceful daemon shutdown, typed errors, and concurrency tests.
- Vivarium migration: workspace member, renamed binary/protocol surface,
  shared mailspace discovery, and per-project default socket.

### Completed milestone: complete raw terminal substrate

Raw bytes, named keys and chords, resize propagation, bounded scrollback,
screen and output revisions, cursor/mode reporting, and atomic diagnostic
snapshots are implemented and tested against deterministic PTY fixtures.

### Following milestones

1. Add ordered events, waits, operation correlation, bounded replay, and lag
   recovery.
2. Define the driver contract and conservative generic driver with serialized,
   guarded semantic actions.
3. Implement the Codex driver and acknowledged submission state machine.
4. Add read-only attachment plus exclusive expiring control leases.
5. Add the thin MCP facade and version/capability negotiation.
6. Integrate a canary Fleet role using a canonical Vivi PTY runtime binding;
   remove tmux control for that role and exercise repeated real Fleet cycles.
7. Validate the abstraction with Pi and OpenCode drivers.
8. Operationalize installation, daemon lifecycle, authorization, diagnostics,
   soak testing, migration guidance, and release packaging.

Each milestone receives its own delivery spec, validation checkpoint, and
cohesive commit. Detailed file edits and task graphs belong in those delivery
specs, not in this goal.

## Release Posture

Decision: release checkpoints are required, but publication remains separately
authorized.

- `vivi-pty` initially versions independently inside the workspace while it
  is experimental.
- Evaluate version and release notes after each user-visible protocol or CLI
  milestone.
- The first product release is gated on a real Codex Fleet canary completing
  repeated wake, work, idle, approval, interruption, restart, and recovery
  cycles without tmux.
- Packaging should install `vivi` and `vivi-pty` together while preserving
  their separate processes.
- Tagging, registry publication, Homebrew publication, deployment, and changes
  to active Fleet installations require explicit authorization.

## Exit Strategy

Decision: included as a per-role rollout boundary.

- Unmigrated Fleet roles remain tmux-backed during validation.
- A role may switch back to tmux only by stopping its Vivi PTY-owned process
  group and changing the canonical runtime binding; no live dual ownership is
  permitted.
- Stopping or uninstalling `vivi-pty` must leave Vivi mailspaces and durable
  work records intact and usable.
- The standalone `fleet-pty` repository remains historical evidence, not a
  supported fallback package.

## Acceptance Criteria

- A clean Vivarium build produces independently runnable `vivi` and
  `vivi-pty` binaries.
- Two different Vivi projects resolve to different daemon endpoints and cannot
  list, inspect, write to, or stop each other's sessions through normal client
  discovery.
- Runtime sessions bind canonically to configured Vivi role identities without
  requiring tmux pane addresses.
- Session lifecycle, process-group cleanup, terminal state, event ordering,
  operation correlation, bounded history, and authorization policies have
  automated coverage.
- Codex, Pi, and OpenCode pass a shared driver conformance suite, with
  unsupported capabilities reported explicitly.
- A Fleet Mind can create, inspect, wake, submit to, observe, interrupt,
  restart, and stop a migrated Codex role through Vivi PTY alone.
- Repeated canary cycles demonstrate no pane scraping, tmux keystrokes, lost
  input, duplicate submission, cross-session writes, or orphaned processes.
- An operator can attach read-only, acquire exclusive control, interact, and
  detach without racing Fleet automation.
- Restarting the CLI, MCP facade, or Fleet client does not terminate daemon-owned
  sessions; daemon shutdown does terminate and reap them.
- Vivi mail and project work commands continue to function with the PTY daemon
  unavailable.
- Installation, migration, diagnostics, rollback, and removal procedures are
  documented and exercised on a clean host before release.

## Validation

- `cargo fmt --all --check` must pass.
- `cargo clippy -p vivi-pty --all-targets -- -D warnings` must pass for every
  milestone; workspace-wide Clippy must pass before product release.
- `cargo test --workspace` must pass under an explicit timeout.
- Protocol tests must cover version negotiation, malformed input, limits,
  ordering, retries, lag, disconnects, and cross-project isolation.
- Process tests must prove descendant cleanup and absence of unrelated-process
  signaling.
- Driver fixtures must cover normal, ambiguous, approval, interruption, error,
  restart, alternate-screen, resize, Unicode, and high-output behavior.
- Live opt-in tests must exercise each supported harness.
- The release gate requires a documented multi-cycle Fleet canary and a clean
  install/start/diagnose/restart/remove smoke test.

## Open Questions

- What is the canonical local authorization mechanism: filesystem mode and peer
  credentials, per-fleet capabilities, or a combination?
- Should runtime binding live in `.vivi/fleet.json`, a dedicated
  `.vivi/runtime.json`, or Vivi-owned mailspace configuration?
- Does any real Fleet role require multiple simultaneous harness sessions, or
  can role identity remain the sole session key?
- Should release packaging use one shared Vivarium version or independently
  version `vivi-pty` until the first stable runtime release?
- Which service manager targets are required for the first supported release?

These questions do not block the raw-terminal milestone. Authorization and
runtime-binding location must be decided before Fleet integration.

## Stop Conditions

- Stop if a milestone requires weakening process ownership, input ordering,
  project isolation, authorization, or evidence requirements to make tests pass.
- Stop if implementation would make Vivi mail availability depend on the PTY
  daemon.
- Stop if a facade or shared module preserves a forbidden dependency behind a
  new name rather than maintaining the durable/ephemeral boundary.
- Stop if a harness cannot be controlled through its terminal without
  credentials, destructive actions, or a human-only trust decision not
  represented by the protocol.
- Stop before changing a live Fleet configuration, publishing a package,
  installing a service, or exposing a network listener without explicit
  authorization.
- Stop and resolve the relevant open question before authorization or
  runtime-binding work begins.
- Leave failures visible if validation finds lost input, duplicate
  submissions, cross-project access, cross-session writes, orphaned process
  groups, or ambiguous state presented as certain.

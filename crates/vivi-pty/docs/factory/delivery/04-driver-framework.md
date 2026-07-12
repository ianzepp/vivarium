# Phase 4 delivery: driver framework and generic semantic runtime

## Interpreted unit

Define the harness-driver boundary above the raw terminal and event layers.
Drivers classify terminal evidence into normalized states and translate guarded
semantic actions into terminal actions; the generic driver provides a
conservative implementation that never guesses ambiguous state. Per-session
semantic actions are serialized and guarded by the current classification.

The governing invariant is: **driver code owns harness policy, while the
daemon remains the sole process/PTY owner; every semantic action is serialized,
capability-checked, evidence-backed, and rejected when its expected state is
not satisfied.**

## Normalized spec

### In scope

- Normalized states: `starting`, `waiting_for_input`, `submitting`, `running`,
  `approval_required`, `completed`, `failed`, `stopped`, and `unknown`.
- Evidence and confidence attached to every classification.
- Explicit driver capabilities and a registry for named drivers.
- Generic terminal driver with conservative `waiting_for_input`, `running`,
  and `unknown` classifications plus raw fallback capabilities.
- Semantic actions: submit, interrupt, approve, reject, restart, and raw
  terminal fallback actions.
- Per-session serialized action queues with expected-state guards and action
  outcomes carrying operation IDs and evidence.
- Deterministic fake-harness fixtures and a driver conformance test surface.

### Explicit policy decisions

- A classification may always return `unknown`; empty or contradictory terminal
  evidence never becomes a definitive state.
- Generic-driver heuristics are intentionally conservative: an explicit shell
  prompt is `waiting_for_input`, visible output with no prompt is `running`,
  and all other screens are `unknown`.
- Semantic actions require a capability and expected-state guard. `interrupt`
  is allowed while running; raw writes are the only fallback that may bypass a
  semantic capability, and they remain serialized.
- Each session has one FIFO action queue owned by the daemon registry. A busy
  session rejects a second semantic action rather than interleaving input.
- Restart is represented as stop-then-start policy in the driver action plan;
  this phase does not change process ownership or add a second child.

### Out of scope

- Codex, Pi, or OpenCode-specific drivers and live harness tests.
- Submission acknowledgement/settle state machines beyond generic action
  planning (Phase 5).
- Human leases, MCP, Fleet runtime bindings, authorization, and persistence.

## Repo-aware baseline

- `src/driver.rs` currently contains only a small `HarnessDriver` trait and
  terminal actions; the protocol has normalized process states only.
- Phase 3 provides operation IDs, event publication, waits, and diagnostic
  snapshots, so semantic action outcomes can reuse those contracts.
- The current daemon registry is the process/PTY ownership boundary. Driver
  state must not reach into `vivarium` mail or provider modules.

## Stage graph

```text
normalized protocol state + evidence/capabilities
                  |
                  v
generic driver classification/action plans
                  |
                  v
per-session guarded action queue and outcomes
                  |
                  v
fake-harness conformance and concurrency tests
```

## Implementation work

1. **Driver contract**
   - Add normalized semantic states, confidence, evidence, capabilities,
     semantic actions, and action outcomes.
   - Keep raw terminal actions available without making them driver policy.

2. **Generic driver and registry**
   - Implement conservative prompt/output classification and generic action
     plans for submit, interrupt, approve, reject, restart, and raw fallback.
   - Add named-driver registration with explicit unknown-driver errors.

3. **Guarded action queue**
   - Add a per-session FIFO queue that checks capability and expected state
     before execution, assigns/propagates operation IDs, and emits outcomes.
   - Keep execution as plans over the existing terminal writer; do not spawn
     processes or bypass session ownership.

4. **Tests and documentation**
   - Add pure classification/action tests, fake-harness conformance cases,
     guard failures, queue ordering, busy rejection, unknown-state behavior,
     and operation evidence checks.
   - Update the crate README with the generic driver boundary and current
     limitations.

## Checkpoints and gates

### Checkpoint target

A fake terminal can be classified conservatively, a generic semantic submit or
interrupt becomes a deterministic terminal-action plan, unsupported or
ambiguous actions are rejected explicitly, and concurrent actions on one
session cannot interleave or bypass expected-state guards.

### Gate

`PASS` requires:

- `cargo fmt --all --check` passes.
- `cargo clippy -p vivi-pty --all-targets -- -D warnings` passes.
- `cargo test --workspace` passes under an explicit timeout.
- Conformance tests cover every normalized state, unknown evidence, capability
  rejection, expected-state mismatch, queue ordering, and busy behavior.
- Existing process, terminal, event, wait, operation, isolation, and hygiene
  tests remain green.

### Release decision

`defer-release`: this adds an internal semantic driver API but does not yet
operate a real Codex Fleet role.

## Validation

```sh
timeout 30s cargo fmt --all --check
timeout 90s cargo clippy -p vivi-pty --all-targets -- -D warnings
timeout 120s cargo test --workspace
```

## Companion skill plan

- Correctness: audit state guards, queue ordering, action capability checks,
  and the process/driver ownership boundary.
- Cleanliness: keep protocol models, generic policy, and queue execution in
  separate modules.
- Housekeeping: enforce dedicated test files, formatting, Clippy, and bounded
  validation.
- Polish: inspect each Phase 4 primary source file serially before closeout.

## Open questions

None block the generic framework. Exact Codex submission settle policy and
harness-specific state evidence remain Phase 5 decisions.

## Revision history

- 2026-07-12: Phase 4 delivery spec compiled from the rewritten goal and the
  completed raw terminal/event contracts.

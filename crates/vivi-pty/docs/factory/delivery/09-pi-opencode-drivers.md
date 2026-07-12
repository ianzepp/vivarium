# Phase 9 delivery: Pi/OpenCode drivers and conformance

## Interpreted unit

Validate that the Phase 4 driver boundary is not Codex-shaped by adding
conservative Pi and OpenCode drivers. Their marker sets come from the current
Fleet sensors, but the drivers remain pure terminal classifiers and action
planners with no Fleet or harness process dependency.

The governing invariant is: **each harness-specific driver may recognize only
its own stable evidence and must report unsupported or unknown behavior rather
than borrowing another harness’s assumptions.**

## In scope

- Named `pi` and `opencode` drivers in the built-in registry.
- Fleet-grounded prompt, running, approval, completion, failure, and unknown
  fixtures for both drivers.
- Explicit capability sets and deterministic submit/interrupt/approval/reject
  plans.
- Shared conformance assertions for normalized classification shape,
  evidence, unknown handling, capability rejection, and operation plans.
- Capability discovery updated with the new drivers.

## Explicit policy decisions

- Pi recognizes its `❯`/input prompt, activity markers, and completion/approval
  phrases only; generic shell or Codex markers do not make Pi certain.
- OpenCode recognizes its status-bar/input markers and trust prompts only;
  unknown chrome remains unknown.
- Restart remains daemon-owned and unsupported by these pure terminal drivers.
- Live opt-in harness tests remain separate from deterministic fixtures.

## Out of scope

- Launching Pi/OpenCode, changing Fleet config, or replacing tmux doorbells.
- Harness-specific acknowledgement state machines beyond Codex.
- MCP transport, authorization, leases, and release packaging.

## Stage graph

```text
Fleet-grounded screen fixtures -> driver classification -> shared conformance
                                                  -> guarded action plans
```

## Implementation work

1. Add Pi and OpenCode driver modules with stable marker evidence.
2. Register both as built-ins and advertise them through capabilities.
3. Add shared fixture/conformance coverage and explicit unsupported paths.
4. Update README and goal checkpoint.

## Gates

`PASS` requires:

- `cargo fmt --all --check` passes.
- `cargo clippy -p vivi-pty --all-targets -- -D warnings` passes.
- `cargo test --workspace` passes under an explicit timeout.
- Both drivers classify known, ambiguous, approval, completion, failure, and
  unknown fixtures with evidence.
- Unsupported capabilities remain explicit and no driver launches a process.

## Release decision

`defer-release`: deterministic cross-harness conformance is stronger, but live
Pi/OpenCode harness tests and the Fleet canary remain pending.

## Validation

```sh
timeout 30s cargo fmt --all --check
timeout 90s cargo clippy -p vivi-pty --all-targets -- -D warnings
timeout 120s cargo test --workspace
```

## Revision history

- 2026-07-12: Phase 9 delivery spec compiled from Fleet sensor markers and the
  normalized driver contract.

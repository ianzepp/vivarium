# Phase 5 delivery: Codex driver and acknowledged submission

## Interpreted unit

Implement the first harness-specific driver above the Phase 4 contract. The
Codex driver classifies recognizable terminal evidence and exposes a submission
workflow that waits for observable composer receipt before sending the submit
key. It must preserve `unknown` when the screen does not provide enough
evidence; a fixed keystroke recipe is not an acknowledgement.

The governing invariant is: **a Codex submission is successful only after
driver-observable evidence of composer receipt and a subsequent terminal
outcome; missing or contradictory evidence becomes an explicit uncertain
result rather than a fabricated success.**

## In scope

- A named `codex` driver implementing `HarnessDriver`.
- Codex capabilities for submit, interrupt, approve, reject, and raw input as
  represented by deterministic terminal plans; process restart remains a
  daemon-owned operation for a later phase.
- Conservative classification for input-ready, approval-required, running,
  completed, failed, and unknown evidence.
- A submission state machine keyed by an operation ID and baseline screen
  revision.
- Literal composer write, observable composer receipt, Codex submit key, and
  running/terminal outcome transitions.
- Deterministic fake-screen fixtures for positive, negative, and ambiguous
  observations.

## Explicit policy decisions

- Codex prompt markers and approval/error phrases are evidence, not durable
  work truth.
- Composer receipt requires a changed screen revision and the submitted text
  appearing in the visible screen. A timeout/unchanged screen is uncertain.
- The submit key is emitted only after receipt is observed.
- A transition to a recognized running, completed, or failed state is an
  acknowledged post-submit outcome; any other screen is uncertain.
- The driver does not launch processes, write sockets, mutate Vivi mail, or
  infer task completion from terminal output alone.

## Out of scope

- Real Codex process launching or live Codex integration tests.
- Daemon RPC methods, leases, attachment, MCP, and Fleet bindings.
- Persistent submission records or cross-restart replay beyond the existing
  operation store.
- Codex version-specific UI scraping beyond stable prompt/approval/error
  markers covered by fixtures.

## Stage graph

```text
Codex terminal snapshot
          |
          v
conservative classification
          |
          v
write message + await visible receipt
          |
          v
submit key + await running/terminal outcome
```

## Implementation work

1. Add the Codex driver and stable evidence markers.
2. Add a submission record/progress API with explicit uncertain outcomes.
3. Keep all terminal actions deterministic and compatible with `ActionQueue`.
4. Add fake-screen tests for every state and every submission transition.
5. Update README and the goal checkpoint.

## Gates

`PASS` requires:

- `cargo fmt --all --check` passes.
- `cargo clippy -p vivi-pty --all-targets -- -D warnings` passes.
- `cargo test --workspace` passes under an explicit timeout.
- The submit key cannot be planned before composer receipt.
- Missing, stale, or contradictory evidence yields an explicit uncertain
  result.
- Existing lifecycle, terminal, event, wait, operation, driver, and hygiene
  tests remain green.

## Release decision

`defer-release`: the state machine is testable but not yet attached to a live
Codex process or Fleet role.

## Validation

```sh
timeout 30s cargo fmt --all --check
timeout 90s cargo clippy -p vivi-pty --all-targets -- -D warnings
timeout 120s cargo test --workspace
```

## Revision history

- 2026-07-12: Phase 5 delivery spec compiled from the Codex vertical-slice
  contract and Phase 4 driver framework.

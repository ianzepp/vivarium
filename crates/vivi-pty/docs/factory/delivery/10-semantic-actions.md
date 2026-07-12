# Phase 10 delivery: semantic daemon actions

## Interpreted unit

Connect the driver boundary to daemon-owned session operations needed by a
canary: acknowledged Codex submission, interrupt, and process-group restart.
Semantic actions are serialized per session and retain the existing operation
ID replay contract. The daemon remains the only writer and process owner.

The governing invariant is: **a semantic action is either the sole active
action for its session or is rejected; submit never emits its final key before
driver evidence, and restart replaces only the daemon-owned process group.**

## In scope

- `session.submit` with required operation ID and Codex acknowledgement flow.
- `session.interrupt` through the selected driver’s terminal plan.
- `session.restart` with stop/reap/spawn lifecycle events and the same session
  identity.
- Per-session semantic busy guards and typed unsupported/state errors.
- Operation-correlated semantic outcomes with normalized state/evidence.
- PTY-backed shell fixtures for restart and concurrency guard behavior.

## Explicit policy decisions

- Codex submission waits for visible composer receipt and returns an explicit
  uncertain phase when the bounded observation window expires.
- Generic/Pi/OpenCode submit plans are available through the driver API, but
  daemon acknowledgement is currently Codex-only until their workflows gain
  equivalent receipt state machines.
- Restart is stop-then-spawn under the same role/session identity; no second
  process group may overlap the old one.
- The existing operation store remains the retry/idempotence boundary.

## Out of scope

- Live Codex Fleet cycles, tmux migration, scheduler integration, or remote
  hosts.
- Durable semantic outcomes beyond the existing bounded operation/event store.
- Automatic model fallback or harness selection.

## Stage graph

```text
daemon RPC -> per-session semantic guard -> driver plan/state machine
                                      -> daemon-owned PTY/process actions
```

## Gates

`PASS` requires formatting, strict Clippy, the full workspace test suite under
an explicit timeout, operation IDs on submit, busy rejection without PTY
input, restart process-group cleanup, and retained lifecycle/event evidence.

## Release decision

`defer-release`: the local semantic surface is ready for canary wiring, but no
live Fleet role is changed by this phase.

## Validation

```sh
timeout 30s cargo fmt --all --check
timeout 90s cargo clippy -p vivi-pty --all-targets -- -D warnings
timeout 120s cargo test --workspace
```

## Revision history

- 2026-07-12: Phase 10 delivery spec compiled from the canary acceptance gap
  and the Codex submission contract.

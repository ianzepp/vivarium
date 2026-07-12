# Phase 8 delivery: canonical Fleet runtime binding

## Interpreted unit

Define the migration seam between Fleet role configuration and Vivi PTY
sessions. A binding resolves one configured role to either the existing tmux
runtime or a canonical Vivi PTY session, with role identity as the durable
lookup key. Vivi PTY validates the shape but does not mutate Fleet files or
take ownership of Fleet scheduling.

The governing invariant is: **one role has one selected runtime owner at a
time; a Vivi PTY binding has a canonical session identity and cannot carry a
live tmux target.**

## In scope

- Fleet role binding model with `tmux` and `vivi_pty` runtime kinds.
- Resolution from a `fleet.json`-shaped JSON value for a named role.
- Canonical session identity defaulting to `mail_identity`/role.
- Project-scoped default socket and explicit socket override.
- Driver, cwd, and command fields for a Vivi PTY launch plan.
- Rejection of missing roles, invalid identities, unsupported runtime kinds,
  missing tmux targets, and tmux/Vivi PTY dual ownership.
- Deterministic fixture tests and migration documentation.

## Explicit policy decisions

- Existing Fleet entries without a `runtime` block resolve to tmux and remain
  untouched by this library.
- A Vivi PTY role uses `runtime: {"kind":"vivi_pty", ...}` and must omit
  `tmux_target`, `tmux_session`, and `tmux_window`.
- The binding model is descriptive; starting/stopping the process remains a
  daemon operation and Fleet file writes remain Fleet-owned.
- No compatibility alias preserves `fleet-pty` names or a dual runtime path.

## Out of scope

- Editing sibling Fleet files or changing an active fleet.
- A live Codex canary, scheduler loop, or tmux doorbell replacement.
- Pi/OpenCode drivers, authorization, service installation, and release
  packaging.

## Stage graph

```text
fleet.json role -> binding validation -> canonical Vivi PTY session plan
                                  \-> tmux binding during migration
```

## Implementation work

1. Add binding types and runtime-kind parsing.
2. Resolve role entries from the current Fleet-shaped configuration.
3. Enforce canonical identity and exclusive runtime ownership invariants.
4. Add fixture tests for legacy tmux, Vivi PTY, invalid, and dual-owner cases.
5. Document the seam in the Vivi PTY README and goal checkpoint.

## Gates

`PASS` requires:

- `cargo fmt --all --check` passes.
- `cargo clippy -p vivi-pty --all-targets -- -D warnings` passes.
- `cargo test --workspace` passes under an explicit timeout.
- Legacy tmux config parses without mutation.
- Vivi PTY config produces a canonical role/session/socket binding.
- Dual ownership and malformed identities are rejected before runtime calls.

## Release decision

`defer-release`: this is a validated migration contract, not evidence of a live
Fleet canary.

## Validation

```sh
timeout 30s cargo fmt --all --check
timeout 90s cargo clippy -p vivi-pty --all-targets -- -D warnings
timeout 120s cargo test --workspace
```

## Revision history

- 2026-07-12: Phase 8 delivery spec compiled from the warmed Fleet runtime
  configuration and the canonical identity invariants.

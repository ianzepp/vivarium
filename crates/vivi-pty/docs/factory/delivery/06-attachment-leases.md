# Phase 6 delivery: read-only attachment and control leases

## Interpreted unit

Expose a daemon-owned read-only attachment stream and a separate exclusive
control capability for human interaction. Attachment reuses ordered session
events and diagnostic snapshots. Control leases are short-lived, per-session,
and required by the controlled terminal write/key/resize methods.

The governing invariant is: **observation never grants input authority, and a
controlled terminal action is accepted only with the current unexpired lease
for that session.**

## In scope

- `session.attach` read-only attachment acknowledgement and persistent ordered
  event notifications.
- Exclusive per-session leases with bounded TTL, holder identity, capability
  token, expiry, and explicit release.
- Lease-required controlled terminal write, raw-byte write, key, and resize
  methods.
- Typed lease conflict, missing/expired lease, and invalid-input errors.
- Lease release when an explicitly stopped or observed-exited session ends.
- Unit and daemon tests for observation/control separation, exclusivity,
  expiry, wrong-token rejection, and successful controlled input.

## Explicit policy decisions

- The existing raw terminal methods remain recovery/automation primitives. The
  human attachment path uses the new `terminal.control_*` methods, which always
  require a lease token.
- A lease is scoped to one session and one holder string; a second holder must
  wait for release or expiry.
- TTLs are bounded to 1 millisecond through 5 minutes. No lease is durable or
  persisted across daemon restart.
- A lease does not authorize session creation, stopping, inspection, or event
  access. It only gates controlled terminal interaction.
- The daemon remains the sole owner of PTY writes; the lease manager only
  authorizes entry to that existing registry boundary.

## Out of scope

- Authentication or authorization beyond the local socket and lease token.
- Human UI, terminal rendering client, MCP, Fleet integration, or remote
  transport.
- Automatic lease renewal, durable lease recovery, or cross-session leases.

## Stage graph

```text
session.attach -> snapshot/events (read only)

session.lease.acquire -> exclusive token -> terminal.control_* -> release/expiry
```

## Implementation work

1. Add wire types and error codes for attachment and leases.
2. Add a bounded in-memory lease manager with expiry and token checks.
3. Route controlled terminal methods through the existing session registry.
4. Keep attachment streaming compatible with the Phase 3 subscription loop.
5. Add deterministic manager and PTY-backed daemon tests and document the
   operator boundary.

## Gates

`PASS` requires:

- `cargo fmt --all --check` passes.
- `cargo clippy -p vivi-pty --all-targets -- -D warnings` passes.
- `cargo test --workspace` passes under an explicit timeout.
- Read-only attachment cannot write.
- Two holders cannot control one session concurrently.
- Wrong, missing, and expired tokens are rejected without touching the PTY.
- Existing raw terminal, driver, event, wait, operation, lifecycle, and
  hygiene tests remain green.

## Release decision

`defer-release`: the local control boundary exists, but it is not yet
authenticated or exposed through MCP/Fleet.

## Validation

```sh
timeout 30s cargo fmt --all --check
timeout 90s cargo clippy -p vivi-pty --all-targets -- -D warnings
timeout 120s cargo test --workspace
```

## Revision history

- 2026-07-12: Phase 6 delivery spec compiled from the attachment, lease, and
  local-control invariants in the rewritten goal.

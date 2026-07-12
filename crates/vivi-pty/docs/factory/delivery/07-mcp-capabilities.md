# Phase 7 delivery: MCP facade and capability negotiation

## Interpreted unit

Provide a thin, allowlisted client facade for MCP tools and a daemon capability
document that lets clients discover the supported protocol, drivers, and
control boundaries. The facade translates tool calls to the existing local
JSON-RPC socket; it does not own sessions, PTYs, leases, or durable work.

The governing invariant is: **MCP is a protocol client of the daemon, and its
allowlisted tools cannot bypass daemon operation IDs, attachment boundaries,
lease checks, or process ownership.**

## In scope

- Versioned daemon capability discovery with supported methods and built-in
  driver names.
- An allowlisted `McpBridge` tool registry with read-only and lease-required
  metadata.
- Translation from stable tool names to existing daemon JSON-RPC methods.
- A client call path that delegates to the existing Unix-socket client.
- Unknown-tool and malformed-call errors before any socket request.
- Tests for capability shape, tool metadata, translation, and rejection of
  arbitrary daemon methods.

## Explicit policy decisions

- Tool names are stable `vivi_pty.*` names; daemon method names remain the
  internal transport contract.
- The bridge exposes observation, attachment, lease, controlled input, and
  diagnostic tools. It does not expose arbitrary method passthrough.
- Capability discovery is descriptive, not authorization. The daemon still
  validates every request and lease token.
- MCP transport/version negotiation outside this local bridge is deferred until
  the consumer integration phase.

## Out of scope

- A standalone MCP server process or network listener.
- MCP resource/prompt subscriptions beyond the existing event attachment.
- Fleet runtime binding, authentication, authorization, or remote transport.
- Changes to Vivi mail or task semantics.

## Stage graph

```text
daemon.capabilities -> capability document

MCP tool name -> allowlisted daemon method -> existing RPC/lease guards
```

## Implementation work

1. Add capability and tool descriptor wire types.
2. Implement the built-in tool registry and safe method translation.
3. Add `daemon.capabilities` and expose the registry’s capabilities.
4. Route bridge calls through `client::call` and preserve daemon errors.
5. Add unit tests and update README/goal checkpoint.

## Gates

`PASS` requires:

- `cargo fmt --all --check` passes.
- `cargo clippy -p vivi-pty --all-targets -- -D warnings` passes.
- `cargo test --workspace` passes under an explicit timeout.
- Unknown tools and arbitrary methods are rejected locally.
- Capability output identifies the protocol, drivers, attachment, and lease
  boundaries without claiming authorization.
- Existing daemon, lease, driver, event, wait, operation, lifecycle, and
  hygiene tests remain green.

## Release decision

`defer-release`: the bridge is a local client library; consumer-specific MCP
transport and Fleet integration remain pending.

## Validation

```sh
timeout 30s cargo fmt --all --check
timeout 90s cargo clippy -p vivi-pty --all-targets -- -D warnings
timeout 120s cargo test --workspace
```

## Revision history

- 2026-07-12: Phase 7 delivery spec compiled from the capability and MCP
  boundary in the rewritten goal.

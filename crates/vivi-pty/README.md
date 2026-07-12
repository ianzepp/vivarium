# vivi-pty

`vivi-pty` is a harness-neutral runtime adapter for terminal-based coding
agents. It owns each agent's pseudo-terminal, handles harness-specific terminal
behavior, and presents normalized session operations and state to Fleet.

The project deliberately keeps the terminal as its universal compatibility
contract. Codex, Pi, OpenCode, and future harnesses do not need to expose an API;
they only need to run in a terminal.

See [the project brief](docs/BRIEF.md) for the intended architecture, first
milestone, and design constraints.

## MVP

The MVP runs a persistent multi-session daemon with a length-prefixed JSON-RPC
2.0 protocol over a Unix domain socket. It can launch commands under owned PTYs
and list, inspect, and stop those sessions.

```sh
cargo build

# Terminal 1
target/debug/vivi-pty daemon

# Terminal 2
target/debug/vivi-pty info
target/debug/vivi-pty session list
target/debug/vivi-pty session start demo \
  --cwd /tmp -- /bin/sh
target/debug/vivi-pty session inspect demo
target/debug/vivi-pty terminal write demo "printf 'hello from PTY\\n'" --enter
target/debug/vivi-pty terminal write-bytes demo 1b5b324a
target/debug/vivi-pty terminal key demo c --modifiers control
target/debug/vivi-pty terminal resize demo 160 50
target/debug/vivi-pty terminal snapshot demo
target/debug/vivi-pty session diagnostic demo
target/debug/vivi-pty session stop demo
```

The daemon is project-scoped. It discovers the nearest Vivi mailspace and uses
`.vivi/vivi-pty.sock`, so separate Fleets do not share a runtime. Select a
mailspace with `--project`; override discovery with `VIVI_PTY_SOCKET` or
`--socket`.

Terminal output is continuously parsed into an in-memory VT100 screen.
`terminal.snapshot` returns rendered visible contents, lossless formatted bytes,
dimensions, cursor position, mode flags, bounded scrollback metadata, and
monotonic screen/output revisions; it does not merely return raw stdout.
`terminal.write` sends literal UTF-8 text, `terminal.write-bytes` accepts raw
bytes as hexadecimal, and `terminal.key` encodes named keys and modifiers.
`terminal.resize` updates both the child PTY and emulator. `session diagnostic`
returns process, protocol, and terminal evidence in one snapshot.

Long-lived JSON-RPC clients can subscribe to ordered `session.event`
notifications, wait for a state or screen revision, and recover from a lagged
subscription using the diagnostic snapshot included in the event batch. Set
`operation_id` on a request to correlate and safely retry a completed session
or terminal operation; reusing that ID for different parameters is rejected.

The driver layer now classifies terminal evidence into normalized harness
states and turns guarded submit, interrupt, and raw-input requests into
deterministic terminal-action plans. The generic driver is deliberately
conservative: it recognizes explicit shell prompts and otherwise reports
visible output as running. Additional harness drivers, leases, attachment, and
the MCP facade are subsequent vertical slices.

The built-in Codex driver adds evidence-backed state classification and an
acknowledged submission workflow: it writes the composer literally, waits for
the submitted text to appear on a newer screen revision, and only then plans
the Codex submit key. Stale, contradictory, or unrecognized evidence becomes
an explicit uncertain result.

`session.attach` provides a read-only ordered event stream. Human interaction
uses a short-lived exclusive lease acquired with `session.lease.acquire`; the
lease token is required by `terminal.control_write`,
`terminal.control_write_bytes`, `terminal.control_key`, and
`terminal.control_resize`. Observation never grants input authority.

The `mcp::McpBridge` is a narrow client facade over the same socket protocol.
It advertises the built-in drivers, attachment, lease, event, and replay
capabilities through `daemon.capabilities`, and rejects methods outside its
allowlist before connecting.

The daemon owns each session's Unix process group, not only its direct child.
Stopping a session or shutting down the daemon terminates and reaps that group;
SIGINT and SIGTERM perform the same cleanup before the socket is removed.
Session identifiers are bounded names, and completed sessions are retained only
as a bounded set of inspectable tombstones.

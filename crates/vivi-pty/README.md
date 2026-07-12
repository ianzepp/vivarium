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
target/debug/vivi-pty terminal snapshot demo
target/debug/vivi-pty session stop demo
```

The daemon is project-scoped. It discovers the nearest Vivi mailspace and uses
`.vivi/vivi-pty.sock`, so separate Fleets do not share a runtime. Select a
mailspace with `--project`; override discovery with `VIVI_PTY_SOCKET` or
`--socket`.

Terminal output is continuously parsed into an in-memory VT100 screen.
`terminal.snapshot` returns the rendered visible contents, dimensions, and cursor
position; it does not merely return raw stdout. `terminal.write` sends literal
text, with the CLI's `--enter` option appending a carriage return.

The current session states still describe child-process lifecycle only.
Semantic harness states, message submission, events, attachment, and the MCP
facade are subsequent vertical slices.

The daemon owns each session's Unix process group, not only its direct child.
Stopping a session or shutting down the daemon terminates and reaps that group;
SIGINT and SIGTERM perform the same cleanup before the socket is removed.
Session identifiers are bounded names, and completed sessions are retained only
as a bounded set of inspectable tombstones.

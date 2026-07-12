# Vivarium 5.0.0

Vivarium 5.0.0 adds the `vivi-pty` companion binary — a project-scoped PTY
runtime adapter for Fleet-managed terminal-based coding agents — alongside the
core `vivi` mail layer. This is a major-version release reflecting the
project-level shift from a single-binary mail tool to a multi-binary
agent-runtime platform.

## Highlights

### `vivi-pty` companion binary (new)

`vivi-pty` is a separate binary that owns Fleet-managed agent processes and
pseudo-terminals. It ships alongside `vivi` and ships as a first-class release
artifact. See [`crates/vivi-pty`](crates/vivi-pty/README.md) for the full
specification.

Surfaces shipped since the initial project-scoped addition:

- **Persistent multi-session daemon** with a length-prefixed JSON-RPC 2.0
  protocol over a Unix domain socket. The socket is project-scoped, living
  under `.vivi/` in the nearest Vivi mailspace.
- **Raw terminal control**: `terminal.write` (UTF-8), `terminal.write-bytes`
  (raw hex), `terminal.key` (named keys with modifiers), `terminal.resize`, and
  `terminal.snapshot` (rendered visible contents, formatted bytes, dimensions,
  cursor, scrollback, and monotonic revision counters).
- **Session lifecycle**: `session.start`, `session.stop`, `session.inspect`,
  `session.diagnostic`, and `session.restart`. The daemon owns the Unix process
  group, not just the direct child. Completed sessions are retained as bounded
  tombstones.
- **Events and operation replay**: `session.event` subscriptions and wait
  operations keyed to state or screen revision. `operation_id` on requests
  enables idempotent retry. Reusing an operation ID with different parameters
  is rejected.
- **Generic driver framework**: Evidence-backed terminal state classification
  with guarded submit, interrupt, and raw-input actions. Recognizes explicit
  shell prompts; otherwise reports visible output as running.
- **Codex driver**: Evidence-backed state classification with an acknowledged
  submission workflow — writes the composer literally, waits for the submitted
  text to appear on a newer screen revision, and only then plans the submit
  key. Stale, contradictory, or unrecognized evidence becomes an explicit
  uncertain result.
- **Pi and OpenCode drivers**: Independent Fleet-grounded marker sets for
  prompts, activity, approvals, completion, and failures. Unfamiliar terminal
  chrome remains unknown rather than borrowing Codex or shell assumptions.
- **Grok driver**: Full Grok terminal session support with the same normalized
  state and evidence boundaries.
- **Attachment and control leases**: `session.attach` for read-only event
  streams. Short-lived exclusive `session.lease.acquire` tokens required for
  `terminal.control_write`, `terminal.control_write_bytes`,
  `terminal.control_key`, and `terminal.control_resize`. Observation never
  grants input authority.
- **MCP bridge capabilities**: `mcp::McpBridge` — a narrow client facade over
  the same socket protocol that advertises built-in drivers, attachment, lease,
  event, and replay capabilities, and rejects methods outside its allowlist.
- **Fleet runtime binding**: `binding::resolve_role` validates a role's
  selected runtime. Legacy entries resolve to tmux; `runtime.kind = "vivi_pty"`
  produces a canonical role/session/socket/driver/command plan and rejects any
  remaining tmux target, preventing dual ownership.
- **Semantic daemon actions**: `semantic.submit`, `semantic.interrupt`, and
  `semantic.restart` for daemon-hosted semantics.
- **Functional LLM tests**: End-to-end tests against Codex, Pi, and Grok
  drivers.
- **Size-budget hygiene**: File-line, function-line, and total-function budgets
  enforced by crate-level hygiene tests.

### Mailspace improvements

- `vivi task show` and `vivi need show` avoid a full-scan on handle lookup
  (fix from 858c53b).

## Breaking Framing

The major-version bump reflects:

- **Multi-binary release**: `vivi-pty` ships as a first-class release artifact
  alongside `vivi`, bundled in the same release tarball. The install surface
  expands from one binary to two.
- **Agent-runtime scope**: Vivarium is no longer only a mail tool. The project
  now owns a PTY runtime adapter that Fleet and similar orchestrators depend on
  for terminal-based agent management.
- **`vivi-pty` graduated from 0.x to 1.0**: The companion binary is stable
  enough for production use alongside `vivi` 5.0.

Existing `vivi` mail commands, config, schema, and account files are unchanged
from 4.8.0. No migration is needed for mail storage.

## Installation

```sh
# Homebrew (updates both binaries)
brew upgrade ianzepp/tap/vivarium

# curl installer (installs both vivi and vivi-pty)
curl -fsSL https://raw.githubusercontent.com/ianzepp/vivarium/main/install.sh | bash

# From source
cargo install --path .
cargo install --path crates/vivi-pty
```

`vivi-pty` is installed to the same prefix as `vivi`. The Homebrew formula and
the `install.sh` script both install both binaries by default.

## Release Checks

Before publishing the tag, run:

```sh
cargo fmt --check
cargo test --test hygiene
cargo test --workspace
cargo build --release
target/release/vivi --version
target/release/vivi-pty --version
```

This release changes no provider routing, sync, or send paths relative to 4.8.0,
so live provider smoke checks from `docs/release-smoke-checks.md` are optional
rather than required.

## Publication

GitHub release assets and the Homebrew tap (`ianzepp/homebrew-tap` formula
`vivarium`) are part of this release contract. Each platform tarball
(`vivi-{target}.tar.gz`) contains both the `vivi` and `vivi-pty` binaries.

Crates.io is not part of the release contract.

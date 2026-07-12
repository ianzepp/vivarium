# Vivi PTY project brief

## Problem

Fleet successfully operates arbitrary agent harnesses through tmux because a
terminal is the one interface they all share. That universality comes with a
fragile control seam: Fleet sends keystrokes through `tmux send-keys`, samples a
rendered pane, and infers agent state from terminal text. Failures usually occur
when the terminal is not in the expected shape, submission timing differs, or
old screen content is mistaken for current state.

Fleet needs the same harness neutrality with a faster, stateful, and explicit
machine interface.

## Product thesis

`vivi-pty` owns the agent process and its pseudo-terminal. It continuously
interprets terminal output, applies harness-specific interaction policy, and
exposes common session operations and normalized state to Fleet.

The terminal remains the compatibility layer. Native harness APIs are optional
optimizations, not requirements.

## Architectural invariant

> Vivi PTY exclusively owns the agent PTY and translates semantic operations
> into harness-specific terminal interactions; Vivi remains work truth, and
> tmux is not part of the managed agent runtime.

This implies:

- Fleet does not need to know Codex, Pi, or OpenCode keystrokes.
- Harness drivers do not own work assignment or task lifecycle.
- Rendered screen text is observable evidence, not the durable work record.
- Automated input and human input cannot race; interactive control requires an
  explicit lease.
- Raw terminal operations remain available as a recovery escape hatch.

During migration, tmux-backed and Vivi PTY-backed Hands may coexist, but they
never own the same agent process. Human observation and recovery will use a
dedicated Vivi PTY attachment client.

## Daemon protocol

The persistent daemon listens on a Unix domain socket. Clients exchange JSON-RPC
2.0 messages framed by a four-byte, unsigned, big-endian payload length followed
by UTF-8 JSON. The protocol is independent of its transport even though Unix
sockets are the only MVP transport.

The CLI speaks this protocol directly. A later MCP process will translate MCP
tools into daemon calls over the same socket; it will not own agent processes.
Remote access initially uses SSH rather than exposing an unauthenticated TCP
listener.

The MVP protocol implements:

- `daemon.info`
- `session.list`
- `session.start`
- `session.inspect`
- `session.stop`
- `terminal.write`
- `terminal.snapshot`

Each daemon belongs to one Vivi mailspace. The default socket is
`.vivi/vivi-pty.sock` beneath the discovered project root. Clients may select
the project with `--project` or override discovery with `VIVI_PTY_SOCKET` or
`--socket`.

## Runtime model

The runtime is divided into four layers:

1. **PTY supervisor** — starts the harness, owns its process group, sends bytes,
   captures output, resizes the terminal, and reports exits.
2. **Terminal emulator** — maintains the current cell grid, cursor, modes,
   scrollback, and screen-change stream from ANSI output.
3. **Harness driver** — recognizes harness states and implements semantic
   actions such as submit, approve, interrupt, and compact.
4. **Service surface** — exposes normalized sessions and events through a local
   API, with MCP as a thin client-facing facade rather than the internal core.

## Normalized session states

- `starting`
- `waiting_for_input`
- `submitting`
- `running`
- `approval_required`
- `completed`
- `failed`
- `stopped`
- `unknown`

State reports must include evidence and confidence. A driver must be able to say
`unknown`; it must not convert ambiguous terminal output into false certainty.

## Common operations

Semantic operations:

- start, inspect, list, and stop a session
- submit a message
- approve or reject a prompt
- interrupt and restart a harness
- wait for a state transition
- subscribe to normalized events
- acquire and release an interactive operator lease

Recovery operations:

- read the current screen and bounded scrollback
- send named keys
- write literal bytes
- capture a diagnostic snapshot

## Submission contract

Message submission is an acknowledged state machine rather than a fixed
keystroke recipe:

1. Verify that the harness accepts input.
2. Focus or clear the composer when required.
3. Write the message literally.
4. Observe that the composer received it or wait for a driver-defined settle
   condition.
5. Send the harness-specific submission key.
6. Observe transition to `running`, an error, or an explicit uncertain result.

Fixed delays are permitted as driver fallbacks, but observed state transitions
are preferred.

## Driver contract

Drivers receive terminal snapshots and changes and return classifications with
evidence. They translate semantic actions into terminal commands. Capabilities
are explicit because not every harness supports every operation.

The first drivers are:

1. Codex, as the reference implementation.
2. Pi, to validate that the abstraction is not Codex-shaped.
3. OpenCode, after the driver boundary survives two distinct harnesses.
4. Generic terminal, providing raw input and conservative state reporting.

Simple recognizers and action recipes should eventually be declarative. Driver
code remains available for interactions that require richer state machines.

## First milestone: Codex vertical slice

The first usable release should:

- launch one Codex session under a real PTY
- maintain an in-memory terminal screen
- expose screen, state, submit, interrupt, restart, and raw-key operations
- serialize submissions and correlate them with turn identifiers
- verify composer receipt before sending Enter when observable
- detect successful transition into and out of a running turn
- preserve a raw event stream and diagnostic snapshot
- support a read-only human attachment surface
- demonstrate replacement of Fleet's Codex pane classification and
  submit-settle doorbell behavior

It does not need multi-host orchestration, a browser UI, persistent scheduling,
or full MCP coverage.

## Safety and concurrency

- One session has one ordered input queue.
- Every semantic request has an operation identifier and terminal outcome.
- Retries must be idempotent or explicitly rejected as unsafe.
- Human control requires an exclusive, expiring lease.
- A driver cannot inject input while state is `running` unless the operation is
  an allowed interrupt or the caller explicitly overrides the guard.
- Process termination targets the owned process group, never unrelated terminal
  sessions.

## Relationship to Fleet

Vivi continues to answer: *What work exists, who owns it, and is it done?*

Vivi PTY answers: *Is the harness alive, what is it doing, what terminal state
supports that conclusion, and did the requested interaction take effect?*

Fleet initially integrates through an optional runtime mode so tmux-backed Hands
remain available during comparison. Once the Codex vertical slice is reliable,
Fleet can replace `tmux_target` with a runtime endpoint/session binding while
retaining an attach command for operators.

## Open decisions

- Event persistence and replay boundaries.
- Whether driver definitions begin in Rust or use a declarative format from the
  first milestone.
- Exact normalized evidence and confidence schema.
- Packaging and service-management targets for the first product release.

## Acceptance signal

The project earns adoption when a Fleet Codex Hand can complete repeated wake,
work, idle, approval, interruption, and restart cycles without Fleet inspecting
terminal chrome or choosing Codex-specific keystrokes—and an operator can still
observe and recover the live terminal.

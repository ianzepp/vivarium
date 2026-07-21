# Goal: Vivi Role Process Binding + Status

## Summary

Add a durable **process binding** (`pid` + `host`) to Vivi roles and a read-only
**`vivi role status`** command that resolves a role by name, reads its binding,
and reports live process state (alive / zombie / dead / not-set / remote) plus
context (CPU, memory, uptime, command name). A Hand self-registers its own pid
after it boots — like writing a PID file — so the Mind (or any agent) can ask
"is `hand-1` still running, and how busy is it?" by role name alone, without
knowing the pid, the host, or the backend-specific liveness probe.

This is the process-side companion to the role charter: charter answers "who is
this seat," status answers "is this seat's process alive right now."

## Problem

Fleet is the primary consumer of Vivi roles. The role record already owns seat
essence and capacity (`kind`, `harness`, `provider`, `model`, `thinking`,
`charter`). What it does **not** own is a pointer from the durable role name to
the live process occupying that seat this run. Today the Mind finds process
truth by drilling into backend-specific state (tmux panes, vivi-pty sessions,
sub-agent harness handles), each with its own probe. That works per-backend but
breaks the uniform seam the role model promised: the Mind knows the role name,
not the pid.

Concrete pain the operator named:

1. **Pid discovery is a scavenger hunt.** To check whether `hand-1` is alive,
   the Mind must know which backend it used, then query that backend's process
   table. After a reinit or a compact, the pid it remembered is stale.
2. **No uniform liveness entry point.** `vivi role` is the durable seam, but it
   stops at capacity/charter. There is no `vivi role status` that works
   regardless of `harness`.
3. **Rich context is re-derived everywhere.** "Is it making progress, or hung?"
   today means the Mind itself parsing CPU / runtime captures. The role's host
   can answer that cheaply; the Mind should not have to.
4. **Missed exit notifications.** When a role finishes, reports, and exits, a
   Mind that missed the completion notification has no way to notice the
   process is gone except by failing to reach the backend. A name-keyed status
   poll closes that gap.

The fix is the PID-file pattern lifted into the role record: the role's own
process writes its pid (and host) onto its role row at boot; any querier reads
the binding and probes fresh liveness. Stored value = binding (role-authored);
reported liveness = observation (computed at query time, never stored).

## Relationship To The Prior Role Goal

The first-class-roles goal (`docs/mailspace-role-goal.md`) locked two relevant
decisions:

| Prior decision | This goal's stance |
| --- | --- |
| "Observation (last-ran, pane state) — **not Mind-edited role fields**" | Honored. Liveness, CPU, memory are **computed at query time**, never stored on the role. Only the **binding** (`pid`, `host`) is stored, and it is **self-set by the role's own process**, not Mind-edited. |
| "Runtime geometry (tmux target, host, cwd, steward, posture) stays Fleet/overlay or observation" | `host` here is the **process binding host** (where the pid lives), distinct from fleet runtime geometry. It exists only to make the pid probe honest across hosts. |

A `pid` is not seat essence and not capacity — it is a **runtime binding**. It
belongs on the role row because the role name is the durable key the Mind
already queries, and because the binding is self-authored by the seat it
describes. This does not move runtime geometry or posture into Vivi.

## Design Decisions (locked)

| Decision | Choice |
| --- | --- |
| Noun | **`pid`** (process id) + **`host`** (where that pid lives) on the role record |
| Who writes it | The role's **own process** self-registers at boot (PID-file semantics) |
| Storage shape | Two optional scalars on `LocalIdentity`, serialized only when set |
| Set surface | `vivi role set <name> --pid <PID> [--host <HOST>]` + `--clear-pid` / `--clear-host` (consistent with existing scalar fields) |
| Host default | When `--pid` is set without `--host`, default `host` to the **local hostname** (the role reports from where it runs) |
| Status surface | New subcommand `vivi role status <name> [--project …] [--json]` |
| Liveness source | Computed fresh per query from the OS via the `sysinfo` crate; **never stored** |
| Cross-host honesty | If stored `host` != local host, do **not** probe the local table; report `state = remote` with the stored host |
| "Not responding" | **Out of scope for v1.** Distinguishing alive-but-hung from alive-and-working needs an application heartbeat, not OS process state |
| GPU load | **Out of scope for v1.** Cross-platform GPU monitoring has no clean crate (nvidia-smi is Linux/NVIDIA-only; `powermetrics` needs root on macOS). Structured as a follow-on with a documented extension point |
| Backend neutrality | Works for any `harness`; the role self-reports whatever pid its process has |

### Why a new dependency (`sysinfo`) is justified

Process introspection by pid (state, CPU, memory, uptime, name) is **not**
covered by the Rust standard library or any existing Vivarium dependency.
`sysinfo` is the canonical, maintained, cross-platform crate for exactly this
(makes one set of fields work on both macOS dev and Linux `pharos` server).
Dependency policy permits adding a crate when stdlib/existing crates do not
cover the need. Use will be narrow: one probe module, default features.

### What status reports (v1)

| Field | Source | Reliable? |
| --- | --- | --- |
| `pid`, `host` | role binding | yes (stored) |
| `state` (`alive` / `zombie` / `dead` / `sleep` / `not_set` / `remote` / `unknown`) | `sysinfo` `Process::status()` | yes |
| `running` (bool convenience: state is a live state) | derived from `state` | yes |
| `name` (process name, e.g. `node`) | `sysinfo` | yes |
| `memory_bytes` (resident set) | `sysinfo` | yes |
| `uptime_seconds` | `sysinfo` `start_time` vs now | yes |
| `cpu_percent` | `sysinfo` (two-sample probe) | approximate; see open choice |

CPU% needs two refresh samples to be meaningful (sysinfo computes usage as a
delta). The status command will do a short two-sample probe (bounded interval)
so `cpu_percent` is not stuck at zero. See Open Implementation Choices.

## Goals

1. **Process binding fields** `pid: Option<u32>` and `host: Option<String>` on
   `LocalIdentity`, with serde optional serialization, validation (`pid` > 0),
   and round-trip through `RoleUpdate` / `RoleView`.
2. **Set/clear surface** via `vivi role set <name> --pid … [--host …]`,
   `--clear-pid`, `--clear-host`. Setting `--pid` without `--host` defaults
   `host` to local hostname.
3. **`vivi role status <name>`** that:
   - resolves the role by name or alias,
   - reads the binding,
   - if no pid: reports `state = not_set`,
   - if stored host != local host: reports `state = remote` (does not probe
     local table),
   - otherwise probes via `sysinfo` and reports `state`, `running`, `name`,
     `memory_bytes`, `uptime_seconds`, `cpu_percent`.
4. **JSON output** (`--json`) machine-parseable for the Mind, plus a short text
   rendering.
5. **Backend-neutral**: works regardless of `harness`; the role self-reports.
6. **`role show` / `role list --json`** surface `pid` and `host` alongside
   existing fields.
7. **README** documents the new fields, the boot-time self-registration pattern,
   and the status command.
8. **Tests** cover: set/clear pid+host, host defaulting, status for alive /
   dead / not-set / remote-host cases, JSON shape, CLI parse.

## Non-goals

- Storing liveness, CPU, memory, last-ran, or any **observed** value on the role
  row. Observation is computed at query time only.
- Application-level heartbeat / "not responding" detection.
- GPU load in v1 (follow-on; needs platform-specific tooling).
- Killing, signaling, or otherwise controlling the process from Vivi. Vivi
  reports; the Mind or operator acts.
- Auto-clearing the pid when a process exits. The binding is durable; stale
  bindings surface honestly as `dead` / `zombie`. (Optional auto-clear is a
  later decision — see Open Questions.)
- Moving fleet runtime geometry (tmux target, cwd, steward, posture) into Vivi.
- Bulk status across roles. One role per call; loops live outside the binary,
  consistent with the prior no-bulk decision.
- Cross-host remote probing (ssh-ing to `pharos` to check its pid). v1 reports
  `remote` and lets the caller decide; remote probing is a fleet concern.

## Ground Truth (repo)

| Signal | Location |
| --- | --- |
| Role storage record | `src/mailspace/identity.rs` — `LocalIdentity` (add `pid`, `host`); `RoleUpdate`; `RoleView` |
| Role CLI tree | `src/cli/role_command.rs` — `RoleCommand` enum (`Set` gains `--pid`/`--host`/`--clear-*`; new `Status` variant) |
| Role command handler | `src/local_role_command.rs` — `handle_role_command`, `build_role_update`, `update_has_fields`, `print_role_text` |
| Dispatch (sync, no Runtime) | `src/local_mailspace_command.rs::run_mailspace_command` → `Command::Role`; `src/main.rs::run` calls it before async dispatch |
| Field sanitizers | `identity.rs` — `sanitize_identity`, `sanitize_freeform_field`, `optional_field` (model the `pid`/`host` validators on these) |
| Error type | `src/error.rs` — `VivariumError` (`Message`, `Other`); no `anyhow` |
| CLI parse tests | `tests/cli.rs` — `parses_role_add_set_and_charter` (extend) |
| Role behavior tests | `tests/local_mailspace_cli.rs` (extend) |
| Hygiene ceilings | `tests/hygiene.rs` — 1000-line file, 60-line function on `src/**/*.rs` |
| Prior role goal | `docs/mailspace-role-goal.md` — invariants this goal must respect |
| Consumer | `~/.agents/skills/fleet/SKILL.md` — Mind owns liveness; role name is the durable key |

## Constraints and Invariants

1. **Binding is self-authored.** Only the role's own process (or an agent acting
   on its behalf) writes its `pid`. The Mind does not edit another role's pid.
2. **Observation is never stored.** `state`, `cpu_percent`, `memory_bytes`,
   `uptime_seconds` are computed at query time and never persisted.
3. **Cross-host honesty.** A status query never reports `dead` for a pid whose
   stored host differs from local; it reports `remote`.
4. **Host defaults to local on pid-set.** So a binding is always host-complete
   and self-consistent (the PID-file invariant).
5. **One role per mutation; one role per status.** No bulk in the binary.
6. **Backend-neutral.** Status does not branch on `harness`.
7. **`pid` > 0.** Zero/negative rejected at sanitization.
8. **Existing fields untouched.** Charter, capacity, aliases, rename semantics
   unchanged. `host` here does not collide with any existing field.
9. **Hygiene ceilings hold.** New code stays under file/function ceilings; new
   probe logic lives in its own small module.
10. **No `anyhow`.** Errors stay in `VivariumError`.

## Architecture Direction

- **Storage**: extend `LocalIdentity` with `pid: Option<u32>` and
  `host: Option<String>`, both `#[serde(default, skip_serializing_if = …)]` so
  legacy `mailspace.toml` rows load unchanged. Extend `RoleUpdate` with
  `Option<Option<…>>` slots mirroring the existing optional-field pattern, and
  `RoleView` with plain `Option<…>`.
- **CLI**: add `--pid`/`--clear-pid`/`--host`/`--clear-host` to the existing
  `RoleCommand::Set`; add a new `RoleCommand::Status { name, project, json }`
  variant. Keep the flat subcommand shape — no new `role pid` sub-tree (one
  direct path, consistent with how `harness`/`provider`/`model` are scalars on
  `set`).
- **Probe module**: new `src/role_status.rs` (or `src/process_probe.rs`) exposing
  one function that takes `(pid, stored_host) -> ProcessStatus` and encapsulates
  all `sysinfo` use + host comparison + state classification. Keeps the sysinfo
  dependency boundary narrow and the handler thin. Likely also a small
  `RoleStatusView` serde struct for JSON.
- **Hostname**: use `sysinfo::System::host_name()` (already pulling sysinfo) for
  the local-host default and the cross-host comparison.
- **Dispatch**: `Status` flows through the existing sync
  `run_mailspace_command` path (no IMAP/Runtime needed), matching `List`/`Show`.

## Implementation Shape (first pass)

One factory phase; the work is tightly cohesive. Ordered slices:

1. **Storage**: `pid` + `host` on `LocalIdentity`/`RoleUpdate`/`RoleView`;
   sanitizers; `apply_role_update` wiring; show/list rendering.
2. **Set surface**: `--pid`/`--host`/`--clear-*` on `RoleCommand::Set`; host
   defaulting on pid-set; `update_has_fields` / `build_role_update` wiring.
3. **Probe module**: `sysinfo`-based `probe_process(pid, stored_host)` →
   `ProcessStatus` + `RoleStatusView`; two-sample CPU read.
4. **Status command**: `RoleCommand::Status` variant + handler + text/JSON
   rendering; `not_set` / `remote` / alive / dead / zombie paths.
5. **Dependency**: add `sysinfo` to `Cargo.toml` `[dependencies]`.
6. **Tests + docs**: extend CLI-parse and role-behavior tests; README role
   section gains `--pid`/`--host`, the boot self-registration snippet, and
   `role status`.

## Acceptance Criteria

- `vivi role set hand-1 --pid 12345` persists `pid=12345` and `host=<local
  hostname>`; `role show hand-1 --json` includes both.
- `vivi role set hand-1 --pid 12345 --host pharos` stores `host=pharos` exactly.
- `vivi role set hand-1 --clear-pid` clears pid (and leaves host, or clears host
  too — pick one consistent rule in delivery and document it).
- `vivi role status hand-1` with the current process's own pid reports
  `state=alive`, `running=true`, and non-null `name`/`memory_bytes`/
  `uptime_seconds`.
- `vivi role status hand-1` with a pid that does not exist reports
  `state=dead`, `running=false`.
- `vivi role status hand-1` with no pid set reports `state=not_set`.
- `vivi role status hand-1` with stored `host` != local reports `state=remote`
  and does **not** probe the local process table.
- `vivi role status hand-1 --json` emits valid JSON with the documented fields.
- `role list --json` and `role show --json` include `pid` and `host`.
- `pid` of `0` or non-numeric is rejected with a clear error.
- Existing role fields, charter, rename, and alias semantics unchanged.
- `cargo fmt --check`, `cargo test --test hygiene`, `cargo test` pass.
- README documents the new fields, status command, and boot self-registration.

## Validation

- `cargo fmt --check`
- `cargo test --test hygiene`
- `cargo test`
- Manual: set a role's pid to `std::process::id()` from a throwaway process,
  then `vivi role status <name>` and confirm `alive`; set a bogus high pid and
  confirm `dead`; set `--host <other>` and confirm `remote`.
- `vivi role status <name> --json | jq .` parses.

## Open Questions / Implementation Choices (delivery may decide)

| Topic | Default bias |
| --- | --- |
| `--clear-pid` also clears `host`? | Bias: clearing pid also clears host (a binding with no pid has no meaningful host). Document the chosen rule. |
| CPU sample interval | Small fixed two-sample interval (e.g. 100–200 ms) inside the status call so `cpu_percent` is meaningful. Not a CLI flag (no options the request did not require). |
| `cpu_percent` when pid is the vivi process itself / unavailable | Report `null` rather than a misleading 0; document. |
| Auto-clear pid on exit | **Not in v1.** Stale bindings surface as `dead`. Revisit if Minds churn on stale rows. |
| `started_at` ISO timestamp | Optional in JSON; derive from `start_time` if cheap, else omit. |
| GPU follow-on | Separate later goal: best-effort `nvidia-smi` probe on Linux behind an extension point; macOS deferred (no rootless GPU API). |

## Stop Conditions

- Pause if `sysinfo`'s `Process::status()` classification does not cleanly map to
  the `alive`/`zombie`/`dead`/`sleep` states on one of the target platforms
  (macOS or Linux) — confirm the enum mapping before shipping a false state.
- Pause if adding `sysinfo` materially bloats compile time or pulls an unwanted
  native dependency on `pharos`; report and reconsider a `/proc`-only fallback
  for Linux.
- Pause if cross-host `remote` semantics are ambiguous to the operator (e.g.
  they expect remote probing) — confirm v1 is local-probe + `remote` label only.

## Success Picture

A Hand boots, loads its charter, then runs one line:

```bash
vivi role set hand-1 --pid $$ --project "$ROOT"
```

The Mind, knowing only the role name, runs:

```bash
vivi role status hand-1 --project "$ROOT" --json
```

and learns whether the seat's process is alive, busy, dead, zombie, or running
on a remote host — without knowing the pid, the backend, or how to probe either.
When the Hand finishes and exits, the next status poll reports `dead`, closing
the missed-notification gap. The role record remains the one durable seam for
both who the seat is and whether its process is live.

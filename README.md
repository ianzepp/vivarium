# Vivarium

Local-first email archive, retrieval, and write layer for private agents. Vivi
now supports a direct-to-Proton integration through `provider = "proton-api"`:
it can log in non-interactively, refresh sessions, sync headers or decrypted
bodies, build local indexes and embeddings, and send mail through Proton's API
without running Proton Bridge. Bridge-backed IMAP/SMTP remains supported for
users who prefer Proton's officially packaged local mail gateway, and standard
IMAP providers continue to work through the same local storage model.

Raw RFC 5322 bytes stay on disk as `.eml` blobs, while mutable mailbox state
and derived indexes live in SQLite.

The repository also ships `vivi-pty`, a companion binary that owns agent
pseudo-terminals. `vivi` remains the durable mail and work interface;
`vivi-pty` exposes ephemeral terminal runtime state over a local Unix socket.
See [`crates/vivi-pty`](crates/vivi-pty/README.md).

## Why

Local agents need access to email. Existing tools (offlineimap, mbsync, mutt) are built for humans and carry decades of assumptions. Vivarium keeps the important part simple: the raw message bytes stay local, stable, and directly readable as `.eml` files, while Vivi owns mailbox placement, flags, bindings, and indexes.

Vivarium is especially useful for isolated agent containers. A container can be
initialized with a Proton username plus a password or `password_cmd`, run
`vivi proton login`, and then sync or send mail directly through Proton without
manual Bridge setup, generated Bridge passwords, or shared Bridge state. This
direct path uses Proton's internal API shape rather than a stable public Proton
API contract, so Bridge remains the conservative compatibility option.

## Install

Current release: **9.0.0**
([GitHub releases](https://github.com/ianzepp/vivarium/releases),
[notes](docs/release-v9.0.0.md)).
Each published archive and the Homebrew formula install both `vivi` and
`vivi-pty`.

With Homebrew on macOS:

```sh
brew install ianzepp/tap/vivarium
```

The tap formula is macOS-only (`aarch64` and `x86_64`).

With curl on macOS or Linux:

```sh
curl -fsSL https://raw.githubusercontent.com/ianzepp/vivarium/main/install.sh | bash
```

Published binary archives for 9.0.0:

- `vivi-aarch64-apple-darwin.tar.gz`
- `vivi-x86_64-apple-darwin.tar.gz`
- `vivi-x86_64-unknown-linux-gnu.tar.gz`

Linux `aarch64` has no binary archive yet. On that platform the installer
falls back to `cargo install` from the release tag.

From source, requires Rust 1.93+:

```sh
git clone https://github.com/ianzepp/vivarium.git
cd vivarium
cargo install --path .
cargo install --path crates/vivi-pty
```

## Agent skill

Agent-facing CLI guidance lives at [`skills/vivi/SKILL.md`](skills/vivi/SKILL.md).
Symlink that folder into a client skill directory:

```sh
ln -s "$(pwd)/skills/vivi" ~/.agents/skills/vivi
```

## Quick Start

```
vivi init
```

This creates `~/.vivarium/` with:

- `config.toml` - general settings such as mail root and TLS policy
- `accounts.toml` - account credentials, created with mode `600`
- `agent/prompt.md` - default prompt for `vivi agent poll`

Semantic embedding settings are intentionally not guessed. If you want
`storage_mode = "semantic"`, `vivi sync --embed`, or semantic search, configure
an embedding service in `config.toml` or pass all embedding options on the
explicit index command:

```toml
[defaults]
embedding_provider = "ollama"
embedding_model = "your-embedding-model"
embedding_endpoint = "http://your-embedding-host/api/embed"
```

Edit `accounts.toml` to add a Proton Bridge account:

```toml
[[accounts]]
name = "proton"
email = "you@proton.me"
username = "you@proton.me"
auth = "password"
password = "your-bridge-app-password"
imap_host = "127.0.0.1"
imap_port = 1143
imap_security = "ssl"
smtp_host = "127.0.0.1"
smtp_port = 1025
smtp_security = "starttls"
provider = "protonmail"
storage_mode = "headers" # proxy | headers | bodies | semantic
```

For direct Proton API sync and send without Bridge:

```toml
[[accounts]]
name = "agent-proton"
email = "agent@proton.me"
username = "agent@proton.me"
auth = "password"
password_cmd = "printenv PROTON_PASSWORD"
provider = "proton-api"
storage_mode = "semantic" # headers | bodies | semantic
```

Then verify the direct API path:

```
vivi proton auth-info --account agent-proton --json
vivi proton login-check --account agent-proton --json
vivi proton login --account agent-proton --json
vivi proton session-check --account agent-proton --json
vivi proton identity --account agent-proton --json
vivi sync --account agent-proton --limit 25 --index --json
vivi sync --account agent-proton --limit 0 --index --embed --json
```

`login-check` verifies credentials and discards returned tokens. `login` stores
the direct Proton session under the account's Vivi state directory, and
`session-check` refreshes that stored session without using the account
password. `identity` uses the stored session to report non-secret user, address,
and key-state metadata.

Direct Proton accounts support local-first reads and draft-first sends.
`storage_mode = "headers"` stores metadata-only local messages.
`storage_mode = "bodies"` fetches encrypted Proton payloads, caches them
privately under the account state directory, decrypts them locally, and stores
reconstructed RFC-like message blobs in the normal Vivi store. `storage_mode =
"semantic"` uses the same body fetch/decrypt/cache path, then allows `--embed`
or `vivi index embeddings` to run as local post-processing over
already-decrypted local bodies.

To send through the direct Proton API, create or provide a local `.eml` draft
and execute it with the direct account:

```
vivi compose --account agent-proton \
  --from agent@proton.me \
  --to you@example.com \
  --subject "Hello" \
  --body "Plain text" \
  --html-body-auto
vivi exec send --account agent-proton --from agent@proton.me path/to/draft.eml
```

Direct Proton send creates the Proton draft, builds Proton encrypted send
packages, and submits the message through Proton's API. Clear external
recipients, Proton/internal recipients, and text/plain external PGP recipients
are supported. HTML or multipart external PGP recipients still require future
PGP/MIME package support.

Vivi sends a Bridge-style Proton app version by default because Proton scopes
key access by client family. If Proton reports that the client is out of date,
set `VIVI_PROTON_APP_VERSION` to a current Proton client app-version string
before rerunning the command.

For `provider = "protonmail"`, Vivi defaults to IMAP implicit TLS on
`127.0.0.1:1143` and SMTP STARTTLS on `127.0.0.1:1025` when host, port, or
security fields are omitted. Set `imap_security` or `smtp_security` explicitly
to override those defaults for a different bridge or mail server.

Then sync:

```
vivi sync
vivi sync --account proton --reset
```

`vivi sync --account <name> --reset` is the clean bootstrap path. It removes the
local cache for that account and rebuilds it from the remote mailbox.

Plain `vivi sync` is incremental. It downloads only missing messages from each
account's configured provider, then updates storage-backed metadata and local
indexes for new messages.

Storage modes control how much mail Vivi keeps locally:

- `headers` is the default. Sync stores provider metadata, folder or label
  identity, and thread/search metadata, but not message bodies.
- `bodies` stores full RFC 5322 messages locally for fast `show`, `thread`,
  export, and offline body access. It does not enable semantic indexing by
  itself.
- `semantic` stores full messages and allows `vivi sync --embed` or
  `vivi index embeddings` to build body-derived embeddings. Semantic embedding
  requires `embedding_provider`, `embedding_model`, and `embedding_endpoint` in
  `config.toml`, or explicit `--provider`, `--model`, and `--endpoint` flags
  for `vivi index embeddings`.
- `proxy` is reserved for live IMAP proxy workflows and does not maintain a
  sync cache.

Header-only sync keeps deterministic search local because Vivi's lexical index
uses headers and metadata: sender, recipients, subject, date, folder, message
IDs, and thread references. Semantic search is body-derived and requires
`storage_mode = "semantic"`.

## Project Mailspaces

Project mailspaces are local-only mailboxes for project-scoped agent addresses.
They are explicit: Vivi discovers an existing `.vivi/mailspace.toml` by walking
upward from the current directory, but it never creates `.vivi/` as a side
effect of send, list, search, or show commands.

```sh
cd /path/to/project
vivi mailspace init
vivi role add ceo --kind head
vivi role add cto --kind head --harness subagent
# legacy alias still works:
# vivi mailspace identity add hand-1
vivi mailspace status
vivi boot
vivi board
```

### Boot (one-read project frame)

`vivi boot` renders the whole project frame in a single bounded read: seat
bindings against observed process state, declared cadences and their silence,
unabsorbed mail, open handles with verdicts, registered goals with register
tallies, memos, charter heads, and the backlog sliced into seat-sized groups.
Use it first when orienting, or after a compaction resets the working picture.

```sh
vivi boot --project /path/to/project
```

It is read-only, stateless, and idempotent: two runs are comparable, and boot
never absorbs, closes, promotes, dispatches, or writes. Act on the frame with
the ordinary verbs.

Every section is capped, and the digest closes with a truncation manifest that
names each cap and how much it omitted. Handles carry a closed verdict
vocabulary (`open`, `blocked`, `stale`, `live`, `unbound`, `unverified`,
`dead`, `zombie`, `remote`, `unknown`), and a registered goal whose Status line
claims a different completion count than its own register is reported as
`MISMATCH`.

Facts Vivi cannot own — git ancestry, lane state, the live seat count of a
harness — arrive through project-declared probes:

```toml
[[probes]]
name = "example"
command = "scripta/boot-probe"   # executable, resolved against the mailspace root
```

A probe prints one JSON object on stdout with `facts` (flat preamble lines),
`sections` (named blocks of lines), and `verdicts` (`handle` plus a verdict
slug and an optional detail). A probe that is missing, exits non-zero, or
prints unparseable JSON is reported under `probes skipped` and never fails
boot, so a project with no probes still gets every native section. See
[`skills/vivi/SKILL.md`](skills/vivi/SKILL.md) for the full contract.

### Roles (agent seats)

Roles are first-class mailspace seats. Each role owns a local mailbox name plus
durable metadata used by multi-agent hosts (especially sub-agent spawns):

| Field | Meaning |
| --- | --- |
| `name` | Local-part / mailbox key (`head-ceo`, `hand-1`) |
| `kind` | Process class (`hand`, `head`, `mind`, `operator`, `steward`, or freeform) |
| `status` | Lifecycle (`active`, `parked`, `retired`, or freeform); default `active` |
| `labels` | Freeform slugs (`auditor`, `floater`, …) |
| `harness` | Execution home (`subagent`, `tmux`, `vivi_pty`, …) |
| `provider` / `model` / `thinking` | Desired capacity (not process liveness) |
| `pid` / `host` | Live process binding; self-set by the role at boot (PID-file semantics). `host` defaults to the local hostname when `pid` is set |
| `cadence` | Optional maximum silence between outbound signals (`15m`, `1h`, …). General-purpose; often used for heads/stewards |
| `charter` | Standing prompt body for the seat (stored under `.vivi/charters/`) |
| `address` | Derived: `{name}@{mailspace}.local` |

```sh
vivi role list --json
vivi role show head-ceo --json
vivi role set hand-1 --provider zai --model glm-5.2 --thinking low
vivi role set head-ceo --harness subagent --cadence 15m
vivi role set head-ceo --clear-cadence
vivi role charter set head-ceo --file personas/ceo.md
vivi role charter show head-ceo
```

### Process status and schedule (by role name)

A role self-registers its live process at boot so any agent can ask "is this
seat's process alive?" by role name alone, without knowing the pid or the
backend. Liveness is computed fresh on each call; nothing observed is stored.

```sh
# At boot, the role's own process writes its pid (host defaults to local):
vivi role set hand-1 --pid $$ --project <root>

# Any agent checks by role name:
vivi role status hand-1 --project <root> [--json]
```

`role status` reports process `state` (`alive`, `zombie`, `dead`, `not_set`,
`remote`, `unknown`), `running`, and — for a live local pid — `name`,
`cpu_percent`, `memory_bytes`, and `uptime_seconds`. If the stored `host`
differs from the local host, it reports `remote` rather than probing the local
table (so a pid on `pharos` is not falsely read as dead from another host). CPU
is sampled with a short two-read interval, so a live pid costs ~200 ms. Clearing
the pid also clears the host.

When a role has `cadence`, `role status` also reports **schedule** health from
the age of that role's latest outbound mailspace message (memos excluded):

| Schedule state | Meaning |
| --- | --- |
| `none` | No cadence configured |
| `never` | Cadence set, no outbound signal yet |
| `ok` | Last signal younger than one cadence (+10% grace) |
| `due` | Silence between one and two cadences |
| `overdue` | Silence at or beyond two cadences |

`due` is advisory visibility for the Mind. `overdue` is an action-required
signal after two full cadence intervals. Board JSON includes `model`,
`thinking`, and a `schedule` block per identity; `schedule.action_required` is
true only for `overdue`. Text output prints configured model capacity, prints a
schedule line when state is not `none`, and marks overdue roles `ACTION REQUIRED`.

```sh
# Bulk capacity flips stay outside vivi (one role per mutation):
for r in head-ceo head-cto hand-1; do
  vivi role set "$r" --provider zai --model glm-5.2 --thinking high
done
```

Parent agents should pass **pointers**, not paste charters:

```text
You are fleet role head-ceo.
Load charter: vivi role charter show head-ceo --project <root>
Load task:    vivi task show <handle> --project <root>
```

`vivi mailspace identity add|list|rename` remains as a thin roster path; prefer
`vivi role` for new work. Existing `[[identities]]` entries in
`mailspace.toml` load as roles with empty optional fields.

The default local domain is derived from the project directory name. In a
project named `hanta-monitor`, `cto` resolves to
`cto@hanta-monitor.local`. Unknown local roles are rejected, external
recipients are rejected by the local delivery commands, and mixed
local/external sends are not sent automatically. Use the existing
`compose`, `enqueue send`, and `exec send` flows for human or external mail.

Local agent mail is stored as raw RFC 5322 `.eml` blobs under `.vivi/blobs/`
with mailbox state in `.vivi/mail.sqlite`:

```sh
vivi mail send --from ceo --to cto \
  --subject "review: local delivery" \
  --body "Please review the API shape."

vivi mail list --for cto
# handle  date  from  subject  (add --json for structured output)
vivi mail list --for cto --json
vivi mail list --from ceo
vivi mail list --to cto
vivi mail list --for cto --from ceo
```

Tasks are ordinary local messages delivered to the recipient's `Tasks` folder.
Completing a task moves the same message to `Done`, so the handle remains
stable across the lifecycle.

```sh
vivi task send --from ceo --to cto \
  --subject "Implement local delivery" \
  --body @task.md

vivi task list --for cto
vivi task list --for cto --json
vivi task list --from ceo
vivi task list --for cto --status all
vivi task done <handle> --for cto
vivi task list --for cto --status done
```

Replies are first-class local mailspace messages. Reply targets are
kind-agnostic, so a mail can answer a need and a task can continue that same
conversation. The parent link is captured by stable content identity and
survives a task or want moving to `done`:

```sh
vivi mail reply <handle> --from cto --body "Reviewed and approved."
vivi mail send --from cto --to ceo --subject "Follow-up" \
  --body "Implement the next step." --reply-to <handle>
vivi mail thread <handle> --json
# Bound a large conversation walk when needed
vivi mail thread <handle> --json --limit 100 --max-depth 20
# Trace the cross-role communication tree around a task, want, or mail
vivi trace <handle>
vivi trace <handle> --json --max-depth 5 --limit 100
# Import an executable Mermaid work graph (project-local topology)
vivi graph import --code mir-swarm-wave-2 --file wave.mmd --check --json
vivi graph import --code mir-swarm-wave-2 --file wave.mmd --json
# Topology is always Mermaid (add --include-state for readiness classes)
vivi graph show mir-swarm-wave-2
vivi graph show mir-swarm-wave-2 --include-state
# Status loops: compact frontier, not a topology dump
vivi graph ready mir-swarm-wave-2
vivi graph ready mir-swarm-wave-2 --json
vivi graph apply mir-swarm-wave-2 --file wave-v2.mmd --json
vivi graph complete mir-swarm-wave-2:verify --json
vivi graph activate mir-swarm-wave-2:verify --task <task-handle> --json
vivi graph export mir-swarm-wave-2 --include-state
vivi graph node add --graph mir-swarm-wave-2 --id u4 --label "G-P-10/U4"
vivi graph edge add --graph mir-swarm-wave-2 --from accept --to u4
vivi board --graph --json
vivi task show <handle> --json
```

Lifecycle `--note` values remain in the event ledger and also become normal
captured replies. `mail thread --infer` enables a read-only best-effort view of
older messages using handle citations and reply subjects; inferred links are
marked separately and never replace captured links.

`vivi trace` builds a cross-role tree around any local handle: it walks captured
reply links, `tasked` lifecycle events, and inferred body-citation links, and it
collapses same-content copies (e.g., sender `sent` and recipient `inbox`) into a
single logical node. Use `--json` for agent consumption and `--max-depth` /
`--limit` to keep large mailspaces bounded.

### Executable work graphs

`vivi graph` stores **executable work topology** separately from `vivi trace`
(communication tree). The whole backlog lives in the graph: every `task` /
`need` / `want` send mints a node in the per-mailspace `backlog` graph, and
`--depends-on` on any work-kind send (task/need/want handles) becomes a
prerequisite edge — one dependency substrate for all kinds.

| Concern | Authority |
| --- | --- |
| Planning topology + ready frontier | `vivi graph` (project `mail.sqlite`) |
| Backlog citizenship + dependencies | `backlog` graph (auto-minted; `--depends-on` on send) |
| Lowering: need → unit tasks | `need bind`; the need completes when all units land |
| Dispatch/exception manifest | `vivi step [--json]`; `--apply <handle>` completes settled items |
| Communication history | `vivi trace` |
| Who to spawn / when | The host, not Vivi. Bind an attempt with `graph activate --task` |

Lifecycle moves keep nodes in step (`task done` completes and unlocks
dependents; `reopen` re-locks; `want promote` changes nothing and wants never
dispatch in `vivi step` before promotion). Every node completion records a
`step_decision` graph event in the same transaction.

**Operator gates.** Imported nodes carry kinds: rhombus `id{label}` imports as
`decision`, an `id:::kind` suffix or `class <ids> <kind>` statement marks
`decision` / `stub` / `parked`, and everything else is dispatchable `task`.
Gated kinds never appear in `graph ready`'s ready list (they render under
`gates`), `graph activate` refuses them, and they resolve with
`graph complete --note` — an operator ruling is recorded, never dispatched.

**Dotted couplings never gate.** `-.->` / `-.-` edges import as non-gating
couplings: topology evidence that never blocks readiness. Export round-trips
them dotted. `-->` remains the only prerequisite arrow.

`vivi step` adjudicates **only the `backlog` graph**. Imported topologies never
enter the step manifest — their nodes are dispatched with `graph activate` and
completed at reconcile. `graph ready` without an argument lists every graph's
frontier, including backlog.

Import a narrow Mermaid `flowchart` / `graph` (run `vivi graph import --help`
for the full accepted-subset summary); Vivi assigns
immutable handles, keeps Mermaid as revision evidence, and reports the ready
frontier (open roots). Use `--check` to validate without writing. Re-importing
identical source is idempotent. Later revisions use `graph apply` (source-id
reconciliation, freezes active/done prerequisites, allows new successors).

| Command | Effect |
| --- | --- |
| `graph import --code … --file …` | First create (or idempotent re-import) |
| `graph apply <code> --file …` | Additive revision of an existing graph |
| `graph show` / `export` | Mermaid topology only (`--include-state` optional) |
| `graph ready [--kind <k>]` | Compact frontier with counts for status loops |
| `graph audit [--repair]` | Backlog citizenship check; repair drift |
| `graph connect <dependent> <prereq>` | Post-hoc prerequisite between backlog items |
| `graph complete <code>:<id>` | Mark done; compact receipt (not full topology) |
| `graph activate <code>:<id> --task <h>` | Bind task attempt; compact receipt |
| `graph node add … [--kind <k>]` | Append a node, optionally a gate kind |
| `board --graph` | Frontier on the board JSON/text surface |

Watch graph lifecycle with `--kinds graph --events node_ready` (also
`node_state`, `attempt_bound`, `revision_imported`, `revision_applied`).

Needs and wants are also local messages with stable handles. Wants are parked in
`Wants` for later prioritization. Promoting a want moves it to `Needs`, where it
becomes first-cycle review material for the owner. Completing a need moves it
to `Done` without mixing it into completed task listings.

```sh
vivi want send --from ceo --to ceo \
  --subject "Improve board visibility" \
  --body "Consider a future governance dashboard."

vivi want set-priority <handle> --for ceo \
  --priority P1 --rank 20 --repo app --lane correctness
vivi want list --for ceo --repo app --lane correctness \
  --sort priority,rank,created --json
vivi want promote <handle> --for ceo --note "Prioritize next cycle"
vivi need list --for ceo
vivi need done <handle> --for ceo --note "Delegated and completed"
vivi need list --for ceo --status done --json
vivi want done <handle> --for ceo --note "No longer relevant"
vivi want list --for ceo --status done --json
```

For routine agent intake, start with status or board output, then show one
selected handle. `vivi board` summarizes actionable open tasks and needs first,
with wants capped as secondary backlog context. Prefer `--project <root>` when
the process cwd is not the mailspace root (both placements work):

```sh
vivi mailspace status --project /path/to/project --json
vivi board --project /path/to/project --for cto --json
vivi --project /path/to/project board --for cto --since 4h
vivi board --project /path/to/project --for cto \
  --watermark-file .vivi/agent-board.watermark --write-watermark
vivi task list --project /path/to/project --for cto --json
vivi need list --project /path/to/project --for ceo --json
vivi task show --project /path/to/project <handle>
```

`vivi board --process` adds a live process block per role (one role with `--for`,
or every role without it). It uses a quick probe — accurate `state`, `running`,
process `name`, `memory`, and `uptime`, but `cpu_percent` is null (a board scan
does not sleep for CPU samples; use `vivi role status <name>` for that). A role
with no binding reads `not_set`, which is the "available to assign" signal:

```sh
vivi board --process --project /path/to/project --json
vivi board --process --for hand-1 --project /path/to/project
```

`vivi board --graph` adds a `graphs[]` field (text section + JSON) for
executable work-graph frontiers without removing existing board fields. Each
node entry includes lifecycle state, readiness, blocked-by **handles**, and
successor handles. Use it with `--json` for agent intake of ready work across
campaigns.

Absorb seals a mail, task, need, want, or memo. After absorb, that record
cannot be moved, reopened, prioritized, deleted, or otherwise changed. A second
absorb of the same handle is a no-op. Replies and `task from` still create new
records.

Configure a dedicated git repo as the historical archive. Newly absorbed records
are written as Markdown with TOML frontmatter. Files are rewritten only when the
rendered bytes change. `archive export` backfills records absorbed before the
archive was configured:

```sh
vivi mailspace archive --project /path/to/project --set /path/to/vivi
vivi mailspace archive --project /path/to/project
vivi mailspace archive export --project /path/to/project
vivi mailspace archive --project /path/to/project --clear
```

```sh
vivi mail absorb --project /path/to/project --for mind <handle> \
  --note "Converted to priority request"
vivi task absorb --project /path/to/project --for hand <handle>
vivi need absorb --project /path/to/project --for ceo <handle>
vivi want absorb --project /path/to/project --for mind <handle>
vivi memo absorb --project /path/to/project --for mind <handle>
vivi mail list --project /path/to/project --for mind --status unabsorbed --json
vivi mail list --project /path/to/project --from mind --json
vivi mail list --project /path/to/project --to hand --json
vivi mail dump --project /path/to/project --for mind \
  --status absorbed --absorbed-by mind --json
```

When capacity opens, create executable tasking from a source handle while
preserving source lineage. The initial supported source kind is a want:

```sh
vivi task from <want-handle> --project /path/to/project \
  --for mind --to hand-2 \
  --subject "Fix the prioritized issue" --body-file task.md
```

For compact Mind-style intake, `cycle intake` gathers unabsorbed mail,
completed tasks since the cursor, open needs, and priority-sorted wants:

```sh
vivi cycle intake --project /path/to/project --for mind \
  --cursor-file .vivi/mind-cycle.cursor --write-cursor --json
```

### Blocking on local mailspace changes

Use project-local watch when an agent has handed off work and should wait for a
turn-end message or a board lifecycle change before running one more cycle:

```sh
# Mind: file work, then block for one matching reply
vivi mailspace watch --for mind --kinds mail --match-subject-prefix "turn end:" \
  --timeout 2m --json

# Or wait for one task completion, then invoke the next cycle
vivi task watch --for mind --events moved --statuses done \
  --match-from hunter-2 --timeout 2m --json
```

`mailspace watch` polls the project-local `.vivi/mail.sqlite` event ledger and
supports caller-owned event-id cursor files with `--cursor-file
<path> --write-cursor`. `--once` performs one non-blocking scan. The aliases
`mail watch`, `task watch`, `need watch`, and `want watch` each watch one
kind and do not take `--kinds`. Use `vivi mailspace watch --kinds` to mix
kinds. This is deliberately different from `vivi sync-events --watch` or the
account-scoped `vivi watch-inbox`, which observes inbound IMAP activity and
emits stable JSON events after local sync. `watch-inbox` never wakes an LLM or
executes outbound work; the Ops bridge owns wake delivery and debounce.

For long local bodies, keep using `--body @path` or pass an explicit body file.
`--body -` reads stdin:

```sh
vivi task send --from ceo --to cto --subject "Review evidence" --body-file evidence.md
printf "Long residual evidence\n" | vivi need send --from cto --to ceo --subject "Residual" --body -
```

Use dumps for audits or export. Work dumps default to open tasks or needs;
include `--status all` only when you intentionally want done history.
Stdout dumps over 25 records or 64 KiB refuse unless you pass
`--confirm-large` (or write the result with `--output <path>`):

```sh
vivi mail dump --participant cto --since 48h --output audit-mail-cto.md
vivi mail dump --participant mind --since 2026-07-14T03:44:00 \
  --status unabsorbed --json
vivi task dump --participant cto --body blocker --json
vivi need dump --participant ceo --status all --json --output audit-needs.json
vivi task dump --for cto --status all --confirm-large
vivi want dump --from ceo --status all --json
vivi want list --for ceo --json
vivi want list --from ceo --status all
```

Mailspace actions performed through Vivi are recorded in a local event ledger.
For example, local sends record sent-copy and delivery events, and task
completion/reopen commands record folder moves with optional `--note` text.
Dump output includes those events so a board review can distinguish current
state from command history.

Recovered mailspaces can be imported into an active project mailspace. Start with
`--dry-run` to see the message, blob, event, link, and conflict counts before
writing anything:

```sh
vivi mailspace import --project /path/to/current/project \
  --from /path/to/recovered/project --dry-run

vivi mailspace import --project /path/to/current/project \
  --from /path/to/recovered/project/.vivi
```

## Storage Layout

Each account lives under `~/.vivarium/{account}/`:

```
~/.vivarium/proton/
├── blobs/
│   └── ab/cd/<content_id>.eml
├── outbox/
├── Drafts/
└── .vivarium/
    ├── storage.sqlite
    └── embeddings/
```

Rules:

- `blobs/` is the immutable content store and the raw-message source of truth
- `.vivarium/storage.sqlite` stores message rows, remote bindings, flags, and metadata
- `.vivarium/embeddings/` stores provider/model-scoped semantic indexes
- `outbox/` and `Drafts/` are local working surfaces for compose/reply flows

Message handles shown by the CLI are short prefixes derived from Vivi-local
`message_id` values. They are stable within a given local cache but are not
folder-and-UID identifiers like `inbox-2050`.

## Commands

`vivi --help` is the live top-level list. In 9.0.0 that is: `init`, `sync`,
`sync-events`, `folders`, `doctor`, `proton`, `render`, `watch-inbox`, `list`,
`board`, `boot`, `mailspace`, `mail`, `task`, `need`, `want`, `memo`, `goal`,
`role`, `cycle`, `show`, `thread`, `trace`, `graph`, `step`, `reply`,
`compose`, `export`, `search`, `index`, `agent`, `exec`, `enqueue`, `queue`,
`labels`, `label`.
Project-mailspace commands are in the section above. Account-scoped examples:

```
vivi init                                      # create config directory and files
vivi --version                                 # print installed version
vivi sync                                      # sync all accounts
vivi sync --account proton                     # sync one account
vivi sync --account agent-proton --json        # sync a direct Proton API account
vivi sync --account proton --limit 100         # cap new downloads for this run
vivi sync --account proton --json              # machine-readable sync summary
vivi sync --account proton --since 3mo         # sync messages from the last 3 months
vivi sync --account proton --since 2025-05-02 --before 2026-05-02
vivi sync --account proton --reset             # delete local cache, then full resync
vivi sync-events --account agent-proton --json # poll Proton API events once
vivi sync-events --account agent-proton --watch --json
vivi folders --account proton --json           # list remote IMAP folders
vivi render --explain --format pdf              # explain installed render pipelines
vivi render report.md --output report.pdf       # render local Markdown
vivi compose --attach-document report.md ...    # draft with Markdown + PDF
vivi watch-inbox --account proton --json        # inbound-only IMAP event source
vivi doctor --account proton                   # check config, IMAP, and SMTP connectivity
vivi list                                      # list inbox (default)
vivi list sent                                 # list sent folder
vivi list -n 25                                # list the 25 newest inbox messages
vivi list inbox --filter DoorDash              # list inbox messages matching handle, sender, or subject
vivi list --flagged                            # list inbox messages with the starred/flagged IMAP flag
vivi list --since 3mo                          # list inbox messages from the last 3 months
vivi list --since 2025-05-02 --before 2026-05-02
vivi show 4f8c2d1                              # read a message by short handle
vivi show 4f8c2d1 --json                       # read a message as JSON with citation metadata
vivi thread 4f8c2d1 --json                     # read local thread context as JSON
vivi export 4f8c2d1 > message.eml              # export the raw RFC 5322 message
vivi export 4f8c2d1 --text                     # export normalized local text
vivi exec archive 4f8c2d1                      # immediately move from inbox to archive
vivi exec delete 4f8c2d1 a91be44 --json        # immediately delete multiple messages
vivi enqueue archive 4f8c2d1                   # queue an archive for later review
vivi queue list                                # list pending queued writes
vivi queue show q123                           # inspect one queued write
vivi queue run q123                            # execute one reviewed queued write
vivi queue run --all                           # execute all pending queued writes in FIFO order
vivi search "invoice"                          # keyword search
vivi search "invoice" --json                   # JSON search output with citation metadata
vivi search "DoorDash" --folder inbox --count  # print only the inbox match count
vivi search "invoice" --from person@example.com
vivi search "invoice" --from-domain example.com
vivi index rebuild --account proton            # rebuild deterministic local index state
vivi labels --json                             # show provider label support
vivi reply 4f8c2d1                             # draft a reply from a local message
vivi compose --to you@example.com --subject hi # create a new local draft
vivi compose --to you@example.com --subject hi --body "Plain text" --html-body-auto
vivi exec send --account agent-proton --from agent@proton.me path/to/draft.eml
vivi agent poll --from person@example.com --json  # trusted-inbox Codex helper
vivi boot --project .                             # one-read project frame
vivi step --project . --json                      # backlog dispatch/exception manifest
vivi step --apply <settled-handle> --project .    # complete a settled item's graph node
```

`compose` and `reply` can create multipart drafts with both plain text and HTML.
Use `--html-body <html>` for explicit HTML, or `--html-body-auto` with `--body`
to generate a simple styled HTML alternative from the plain-text body. Drafts
are still local-first; use `vivi exec send path/to/draft.eml` only after
reviewing the generated `.eml`. On `provider = "proton-api"` accounts, send
uses Proton's API directly. On Bridge-backed or standard IMAP accounts, send
uses the account's SMTP settings.

Write commands are split by effect. `vivi exec ...` performs the external write
now. `vivi enqueue ...` records a durable pending item under the selected
account's Vivi state, and `vivi queue run ...` is the explicit later execution
step. `vivi agent poll` still exists as a trusted-inbox Codex helper;
`vivi agent archive|delete|move|flag` remain plan-or-execute wrappers. Prefer
`exec` / `enqueue` / `queue` for new write flows.

All commands accept `--account <name>` to target a specific account. Without it, account-scoped commands use the first account in `accounts.toml`; `sync` and `list` operate on all accounts.

### Account Mutation Policy

Each account can declare an explicit mutation `policy` in `accounts.toml`.
This controls which remote side effects the selected account is authorized to
perform, independent of command names or queue provenance.

| Policy | `policy =` | Permitted remote operations |
|---|---|---|
| **Full-write** (default) | `full-write` | All: archive, move, trash, delete, expunge, flag, send |
| **Read-only** | `read-only` | Sync, read, search, show only |
| **Archive** | `archive` | Archive, non-trash moves, flags; denies trash, delete, expunge, send |

```toml
[[accounts]]
name = "vault"
email = "vault@proton.me"
# ...
policy = "read-only"
```

Policy is enforced at both enqueue admission and authoritatively during queue
execution, so stale or manually constructed queued items cannot bypass it.
Folder aliases are normalized before classification: `trash`, `deleted`, and
provider-specific trash folder names all classify as a denied move-to-trash
under read-only and archive policies.

Local project mailspace operations (board, task, need, want, mail, memo, role,
graph) are separate from external account mutation policy and are never
restricted by it.

Check the effective policy with `vivi doctor --account <name>` (text or JSON).

### Not in the default CLI

These surfaces are not available unless you build with extra features, or they
are not a compatibility promise:

- OAuth browser auth and token minting (`vivi auth` / `vivi token` require the
  `outbox` cargo feature). `auth = "xoauth2"` with `token_cmd` still works.
- a stable public compatibility promise for old Maildir-style handles

Inbound IMAP watch is `vivi watch-inbox --account <name> --json`. Direct Proton
event polling is `vivi sync-events --watch`. Project-local mailspace watch is
`vivi mailspace watch`.

## Providers

Vivarium handles provider differences at the account boundary:

| Provider | `provider =` | Read source | Send source |
| --- | --- | --- | --- |
| Direct Proton API | `"proton-api"` | Proton API | Proton API |
| Proton Bridge | `"protonmail"` | Bridge IMAP | Bridge SMTP |
| Gmail | `"gmail"` | Gmail IMAP labels | SMTP |
| Standard | `"standard"` | IMAP folders | SMTP |

Bridge-backed Gmail and ProtonMail use their provider `All Mail` views only as
internal sync sources for the local `Archive/` corpus. User-facing archive
operations target the provider's real `Archive` folder. Standard IMAP accounts
sync `INBOX` and `Sent` directly. Direct Proton API accounts map Proton labels
and message state into the same local roles without IMAP.

## Security

- `accounts.toml` is created with `chmod 600` and checked on load
- Group/world-readable `accounts.toml` is rejected unless `--ignore-permissions` is set
- `password_cmd` is supported as an alternative to plaintext passwords:
  ```toml
  password_cmd = "security find-generic-password -s vivarium -a you@proton.me -w"
  ```
- XOAUTH2 is supported for IMAP sync with `auth = "xoauth2"` and `token_cmd`; the command must print a current OAuth access token:
  ```toml
  auth = "xoauth2"
  token_cmd = "security find-generic-password -s gmail-access-token -w"
  ```
- Certificate validation is enabled for `provider = "protonmail"` by default
- Set `reject_invalid_certs = false` on an account, or use `--insecure` as a one-run override, when a local bridge uses an untrusted certificate
- Direct Proton API sessions are stored under the selected account's private
  Vivi state directory and can be refreshed without reusing the account password
  on every command
- Direct Proton encrypted message payload caches are account-local private
  implementation artifacts; do not publish or package them in release artifacts

## Local Operations

For a scheduled local refresh, run a bounded sync from launchd, cron, or a
similar user-level scheduler:

```
vivi sync --account proton --since 3mo
```

For a lightweight maintenance pass that refreshes derived local state without
downloading a batch, use:

```
vivi sync --account proton --limit 0
```

The normal repair path is a clean reset:

```
vivi sync --account <name> --reset
```

That clears the local cache for the account, then redownloads and reindexes it
from the selected remote source of truth: Proton API for `provider =
"proton-api"`, or IMAP for Bridge, Gmail, and standard accounts. If
deterministic search/thread state drifts without needing a full reset, use:

```
vivi index rebuild --account <name>
```

Before cutting a release that touches provider routing, sync, or send behavior,
run the live checks in [docs/release-smoke-checks.md](docs/release-smoke-checks.md).

## Architecture

- **Raw `.eml` blobs are the source of truth.** They are preserved unchanged under `blobs/`.
- **Mutable mailbox state lives in `storage.sqlite`.** Local role, flags, and remote bindings do not rename blobs.
- **Remote access is provider-scoped.** Direct Proton API accounts bypass
  Bridge entirely; Bridge, Gmail, and standard accounts keep using IMAP/SMTP.
- **Derived data is disposable and rebuildable.** Deterministic indexes and embeddings can be rebuilt from blobs plus storage metadata.
- **Search results point back to stable local content.** JSON search output includes the short handle, internal `message_id`, and `content_id` citation data.
- **Full corpus contents never leave the machine by default.** Any cloud access would be explicit, narrow, and user-approved.

## License

MIT

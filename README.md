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

## Why

Local agents need access to email. Existing tools (offlineimap, mbsync, mutt) are built for humans and carry decades of assumptions. Vivarium keeps the important part simple: the raw message bytes stay local, stable, and directly readable as `.eml` files, while Vivi owns mailbox placement, flags, bindings, and indexes.

Vivarium is especially useful for isolated agent containers. A container can be
initialized with a Proton username plus a password or `password_cmd`, run
`vivi proton login`, and then sync or send mail directly through Proton without
manual Bridge setup, generated Bridge passwords, or shared Bridge state. This
direct path uses Proton's internal API shape rather than a stable public Proton
API contract, so Bridge remains the conservative compatibility option.

## Install

With Homebrew on macOS:

```sh
brew install ianzepp/tap/vivarium
```

With curl on macOS or Linux:

```sh
curl -fsSL https://raw.githubusercontent.com/ianzepp/vivarium/main/install.sh | bash
```

From source, requires Rust 1.93+:

```sh
git clone https://github.com/ianzepp/vivarium.git
cd vivarium
cargo install --path .
```

## Quick Start

```
vivi init
```

This creates `~/.vivarium/` with two files:

- `config.toml` - general settings such as mail root and TLS policy
- `accounts.toml` - account credentials, created with mode `600`

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
vivi mailspace identity add ceo
vivi mailspace identity add cto
vivi mailspace status
```

The default local domain is derived from the project directory name. In a
project named `hanta-monitor`, `cto` resolves to
`cto@hanta-monitor.local`. Unknown local identities are rejected, external
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
vivi task done <handle> --for cto
vivi task list --for cto --status done
```

Needs and wants are also local messages with stable handles. Wants are parked in
`Wants` for later prioritization. Promoting a want moves it to `Needs`, where it
becomes first-cycle review material for the owner. Completing a need moves it
to `Done` without mixing it into completed task listings.

```sh
vivi want send --from ceo --to ceo \
  --subject "Improve board visibility" \
  --body "Consider a future governance dashboard."

vivi want promote <handle> --for ceo --note "Prioritize next cycle"
vivi need list --for ceo
vivi need done <handle> --for ceo --note "Delegated and completed"
vivi need list --for ceo --status done --json
```

For routine agent intake, start with list output and show one selected handle.
Use dumps for audits or export. Work dumps default to open tasks or needs;
include `--status all` only when you intentionally want done history:

```sh
vivi task list --for cto --json
vivi need list --for ceo --json
vivi task show <handle>

vivi mail dump --participant cto --since 48h --output audit-mail-cto.md
vivi task dump --participant cto --body blocker --json
vivi need dump --participant ceo --status all --json --output audit-needs.json
vivi want list --for ceo --json
```

Mailspace actions performed through Vivi are recorded in a local event ledger.
For example, local sends record sent-copy and delivery events, and task
completion/reopen commands record folder moves with optional `--note` text.
Dump output includes those events so a board review can distinguish current
state from command history.

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
vivi reply 4f8c2d1                             # draft a reply from a local message
vivi compose --to you@example.com --subject hi # create a new local draft
vivi compose --to you@example.com --subject hi --body "Plain text" --html-body-auto
vivi exec send --account agent-proton --from agent@proton.me path/to/draft.eml
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
step. The older `vivi agent ...` planning surface has been removed because it
named the caller rather than the effect.

All commands accept `--account <name>` to target a specific account. Without it, account-scoped commands use the first account in `accounts.toml`; `sync` and `list` operate on all accounts.

### Not Yet Supported

These surfaces are not available in the default CLI today:

- OAuth browser auth and token minting flows
- watch or background sync mode
- a stable public compatibility promise for old Maildir-style handles

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

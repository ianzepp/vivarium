# Vivarium

Local-first email archive and retrieval layer for private agents. Pulls email from IMAP (via Proton Bridge or any IMAP server) into a Vivi-managed local blob store. Raw RFC 5322 bytes stay on disk as `.eml` blobs, while mutable mailbox state and derived indexes live in SQLite.

## Why

Local agents need access to email. Existing tools (offlineimap, mbsync, mutt) are built for humans and carry decades of assumptions. Vivarium keeps the important part simple: the raw message bytes stay local, stable, and directly readable as `.eml` files, while Vivi owns mailbox placement, flags, bindings, and indexes.

Vivarium treats Proton Bridge as the transport/decryption boundary and does not attempt to speak ProtonMail private APIs.

## Install

Requires Rust 1.93+.

```
git clone https://github.com/ianzepp/vivarium.git
cd vivarium
cargo install --path .
```

## Quick Start

```
vivi init
```

This creates `~/.config/vivarium/` with two files:

- `config.toml` - general settings such as mail root and TLS policy
- `accounts.toml` - account credentials, created with mode `600`

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
smtp_security = "ssl"
provider = "protonmail"
```

The SMTP fields are still required by the account parser even when you only use
the read-only sync/search commands.

Then sync:

```
vivi sync
vivi sync --account proton --reset
```

`vivi sync --account <name> --reset` is the clean bootstrap path. It removes the
local cache for that account and rebuilds it from the remote mailbox.

Plain `vivi sync` is incremental. It downloads only missing IMAP messages, then
updates storage-backed metadata and extracted local content for new messages.

## Storage Layout

Each account lives under `~/.local/share/vivarium/{account}/`:

```
~/.local/share/vivarium/proton/
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
vivi sync --account proton --limit 100         # cap new downloads for this run
vivi sync --account proton --since 3mo         # sync messages from the last 3 months
vivi sync --account proton --since 2025-05-02 --before 2026-05-02
vivi sync --account proton --reset             # delete local cache, then full resync
vivi list                                      # list inbox (default)
vivi list sent                                 # list sent folder
vivi list -n 25                                # list the 25 newest inbox messages
vivi list inbox --filter DoorDash              # list inbox messages matching handle, sender, or subject
vivi list --since 3mo                          # list inbox messages from the last 3 months
vivi list --since 2025-05-02 --before 2026-05-02
vivi show 4f8c2d1                              # read a message by short handle
vivi show 4f8c2d1 --json                       # read a message as JSON with citation metadata
vivi thread 4f8c2d1 --json                     # read local thread context as JSON
vivi export 4f8c2d1 > message.eml              # export the raw RFC 5322 message
vivi export 4f8c2d1 --text                     # export normalized local text
vivi archive 4f8c2d1                           # move from inbox to archive
vivi delete 4f8c2d1 a91be44 --dry-run          # preview deleting multiple messages
vivi search "invoice"                          # keyword search
vivi search "invoice" --json                   # JSON search output with citation metadata
vivi search "DoorDash" --folder inbox --count  # print only the inbox match count
vivi index rebuild --account proton            # rebuild deterministic local index state
vivi reply 4f8c2d1                             # draft a reply from a local message
vivi compose --to you@example.com --subject hi # create a new local draft
vivi compose --to you@example.com --subject hi --body "Plain text" --html-body-auto
```

`compose` and `reply` can create multipart drafts with both plain text and HTML.
Use `--html-body <html>` for explicit HTML, or `--html-body-auto` with `--body`
to generate a simple styled HTML alternative from the plain-text body. Drafts
are still local-first; use `vivi send path/to/draft.eml` only after reviewing
the generated `.eml`.

All commands accept `--account <name>` to target a specific account. Without it, account-scoped commands use the first account in `accounts.toml`; `sync` and `list` operate on all accounts.

### Not Yet Supported

These surfaces are not available in the default CLI today:

- OAuth browser auth and token minting flows
- watch or background sync mode
- a stable public compatibility promise for old Maildir-style handles

## Providers

Vivarium handles the differences between IMAP providers:

| Provider     | `provider =` | Inbox source | Sent source          |
|--------------|--------------|--------------|----------------------|
| Gmail        | `"gmail"`    | INBOX label  | [Gmail]/Sent Mail    |
| ProtonMail   | `"protonmail"` | INBOX      | Sent folder          |
| Standard     | `"standard"` | INBOX folder | Sent folder          |

Gmail and ProtonMail use their provider `All Mail` views only as internal sync
sources for the local `Archive/` corpus. User-facing archive operations target
the provider's real `Archive` folder. Standard IMAP accounts sync `INBOX` and
`Sent` directly.

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
from the IMAP source of truth. If deterministic search/thread state drifts
without needing a full reset, use:

```
vivi index rebuild --account <name>
```

## Architecture

- **Raw `.eml` blobs are the source of truth.** They are preserved unchanged under `blobs/`.
- **Mutable mailbox state lives in `storage.sqlite`.** Local role, flags, and remote bindings do not rename blobs.
- **Derived data is disposable and rebuildable.** Deterministic indexes and embeddings can be rebuilt from blobs plus storage metadata.
- **Search results point back to stable local content.** JSON search output includes the short handle, internal `message_id`, and `content_id` citation data.
- **Full corpus contents never leave the machine by default.** Any cloud access would be explicit, narrow, and user-approved.

## License

MIT

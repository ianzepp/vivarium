# Vivarium

Local-first email archive and retrieval layer for private agents. Pulls email from IMAP (via Proton Bridge or any IMAP server) into standard Maildir folders on disk. No required database or service - just RFC 5322 message files that local AI and agents can read, search, and cite.

## Why

Local agents need access to email. Existing tools (offlineimap, mbsync, mutt) are built for humans and carry decades of assumptions. Vivarium keeps the storage layer compatible anyway: inbox, archive, sent, and drafts are Maildir folders. Point a local agent at the directory and it has real `.eml` files to work with.

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

`vivi sync` is incremental. It downloads only missing IMAP messages, then updates
the local catalog and extraction state for newly cataloged files.

## Mail Storage

Messages are stored as Maildir folders under `~/.local/share/vivarium/{account}/`:

```
~/.local/share/vivarium/proton/
├── INBOX/
│   ├── tmp/
│   ├── new/
│   └── cur/
├── Archive/
│   ├── tmp/
│   ├── new/
│   └── cur/
├── Sent/
└── Drafts/
```

Vivarium-generated filenames keep a `.eml` stem for non-mail tooling, while `cur/` entries use the usual Maildir info suffix such as `:2,S`.

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
vivi list --since 3mo                          # list inbox messages from the last 3 months
vivi list --since 2025-05-02 --before 2026-05-02
vivi show inbox-1                              # read a message
vivi show inbox-1 --json                       # read a message as JSON with citation metadata
vivi thread inbox-1 --json                     # read local thread context as JSON
vivi export inbox-1 > inbox-1.eml              # export the raw RFC 5322 message
vivi export inbox-1 --text                     # export normalized local text
vivi archive inbox-1                           # move from inbox to archive
vivi search "invoice"                          # keyword search
vivi search "invoice" --json                   # JSON search output with citation metadata
```

All commands accept `--account <name>` to target a specific account. Without it, account-scoped commands use the first account in `accounts.toml`; `sync` and `list` operate on all accounts.

### Not Yet Supported

These surfaces are not available in the default CLI today:

- semantic search or local embeddings
- catalog or extraction rebuild commands
- send, reply, compose, OAuth browser auth, token minting, or watch mode

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

Raw `.eml` files are the source of truth. If the derived catalog is corrupted,
stop Vivi, remove `{mail_root}/{account}/.vivarium/catalog.json`, and run
`vivi sync --account <name> --limit 0` to rebuild local catalog entries from
the preserved Maildir files. Parse or extraction errors should be investigated
against the raw path reported in JSON citation fields.

## Architecture

- **Raw `.eml` messages are the source of truth.** They are preserved unchanged.
- **Derived data is disposable and rebuildable.** Metadata, indexes, and future embeddings are derived from raw files.
- **Search results point back to original message files.** JSON search output includes handles and raw paths for local citation workflows.
- **Full corpus contents never leave the machine by default.** Any cloud access would be explicit, narrow, and user-approved.

## License

MIT

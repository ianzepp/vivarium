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
vivarium init
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
vivarium sync
```

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
vivarium init                                  # create config directory and files
vivarium sync                                  # sync all accounts
vivarium sync --account proton                 # sync one account
vivarium sync --account proton --limit 100     # cap new downloads for this run
vivarium list                                  # list inbox (default)
vivarium list sent                             # list sent folder
vivarium show inbox-1                          # read a message
vivarium archive inbox-1                       # move from inbox to archive
vivarium search "invoice"                      # keyword search
vivarium search "invoice" --json               # JSON search output
```

All commands accept `--account <name>` to target a specific account. Without it, account-scoped commands use the first account in `accounts.toml`; `sync` and `list` operate on all accounts.

### Not Yet Supported

These surfaces are not available in the default CLI today:

- semantic search or local embeddings
- `vivarium show <handle> --json`
- `vivarium thread <handle> --json`
- `vivarium export <handle>`
- catalog or extraction rebuild commands
- send, reply, compose, OAuth browser auth, token minting, or watch mode

## Providers

Vivarium handles the differences between IMAP providers:

| Provider     | `provider =` | Inbox source | Sent source          |
|--------------|--------------|--------------|----------------------|
| Gmail        | `"gmail"`    | INBOX label  | [Gmail]/Sent Mail    |
| ProtonMail   | `"protonmail"` | INBOX      | Sent folder          |
| Standard     | `"standard"` | INBOX folder | Sent folder          |

Gmail syncs `[Gmail]/All Mail` into `Archive/`. ProtonMail and standard IMAP accounts sync `INBOX` and `Sent` directly.

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

## Architecture

- **Raw `.eml` messages are the source of truth.** They are preserved unchanged.
- **Derived data is disposable and rebuildable.** Metadata, indexes, and future embeddings are derived from raw files.
- **Search results point back to original message files.** JSON search output includes handles and raw paths for local citation workflows.
- **Full corpus contents never leave the machine by default.** Any cloud access would be explicit, narrow, and user-approved.

## License

MIT

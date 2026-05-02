# Vivarium

Local-first email archive and retrieval layer for private agents. Pulls email from IMAP (via Proton Bridge or any IMAP server) into standard Maildir folders on disk. No required database or service — just RFC 5322 message files that local AI and agents can read, search, and cite.

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

- `config.toml` — general settings (mail root directory, check intervals)
- `accounts.toml` — account credentials (chmod 600 automatically)

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
vivarium list                                  # list inbox (default)
vivarium list sent                             # list sent folder
vivarium show inbox-1                          # read a message
vivarium archive inbox-1                       # move from inbox to archive
```

All commands accept `--account <name>` to target a specific account. Without it, account-scoped commands use the first account in `accounts.toml`; `sync` and `list` operate on all accounts.

### Future Capabilities (Planned)

- `vivarium search <query> --json` — local keyword and semantic search
- `vivarium show <handle> --json` — structured message retrieval with citation fields
- `vivarium thread <handle> --json` — thread-aware retrieval
- `vivarium export <handle>` — local-only raw/text export
- Catalog with stable message handles across syncs
- Local embedding generation for semantic search
- Incremental sync for maintenance

### Quarantined: Send/Reply/Compose (Not Default)

The `outbox` feature gates SMTP send, reply, compose, and watch surfaces. These are quarantined because **sending email is a separate risk class from reading it** and must be explicitly opted into:

```
cargo install --features outbox --path .
```

With `outbox` enabled, additional commands become available:

```
vivarium auth gmail                            # browser OAuth, store refresh token
vivarium token gmail                           # print an access token for token_cmd
vivarium send ~/.local/share/vivarium/proton/Drafts/new/draft.eml   # send an .eml file
vivarium reply inbox-1                         # edit a reply and send it
vivarium reply inbox-1 --body "Thanks"         # send a scripted reply
vivarium compose --to a@b.com --subject "Hi"   # edit and save a draft
vivarium watch --account proton                # watch IMAP and outbox changes
```

## Providers

Vivarium handles the differences between IMAP providers:

| Provider     | `provider =` | Inbox source | Sent source          |
|--------------|--------------|--------------|----------------------|
| Gmail        | `"gmail"`    | INBOX label  | [Gmail]/Sent Mail    |
| ProtonMail   | `"protonmail"` | INBOX      | Sent folder          |
| Standard     | `"standard"` | INBOX folder | Sent folder          |

Gmail syncs `[Gmail]/All Mail` into `Archive/`. Standard IMAP (Proton Bridge, Fastmail, etc.) syncs `INBOX` and `Sent` directly.

## Security

- `accounts.toml` is created with `chmod 600` and checked on load
- Group/world-readable `accounts.toml` is rejected unless `--ignore-permissions` is set
- `password_cmd` is supported as an alternative to plaintext passwords:
  ```toml
  password_cmd = "security find-generic-password -s vivarium -a you@proton.me -w"
  ```
- Gmail OAuth is supported with `auth = "xoauth2"` and `token_cmd`; the command must print a current OAuth access token:
  ```toml
  auth = "xoauth2"
  oauth_client_id = "your-google-oauth-client-id"
  oauth_client_secret = "your-google-oauth-client-secret"
  token_cmd = "vivarium token gmail"
  ```
- Run `vivarium auth gmail` once to approve access in the browser and store the refresh token in macOS Keychain
- Self-signed certs are accepted by default for compatibility with local bridges
- Set `reject_invalid_certs = true` under `[defaults]` or an account to require certificate validation
- Use `--insecure` as a one-run override when a strict TLS config needs to accept invalid certificates

## Architecture

- **Raw `.eml` messages are the source of truth.** They are preserved unchanged.
- **Derived data is disposable and rebuildable.** Metadata, indexes, and future embeddings are derived from raw files.
- **Every search result can cite an original message file.** Citation is mandatory in agent-facing output.
- **Full corpus contents never leave the machine by default.** Any cloud access would be explicit, narrow, and user-approved.

## License

MIT

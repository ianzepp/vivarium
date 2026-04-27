# Vivarium

Local-first IMAP email sync for LLMs. Pulls email from IMAP into plain `.eml` files on disk. No database, no flags, no labels — just files in folders.

## Why

LLMs need access to email. Existing tools (offstrstrstrstrlineimap, mbsync, mutt) are built for humans and carry decades of Maildir complexity. Vivarium is simple: inbox, archive, sent, drafts, outbox. Each message is a single `.eml` file. Point an LLM at the directory and it has everything it needs.

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

Edit `accounts.toml` to add an account:

```toml
[[accounts]]
name = "proton"
email = "you@proton.me"
username = "you@proton.me"
password = "your-bridge-password"
imap_host = "127.0.0.1"
imap_port = 1143
imap_security = "starttls"
smtp_host = "127.0.0.1"
smtp_port = 1025
smtp_security = "ssl"
provider = "standard"
```

Then sync:

```
vivarium sync
```

## Mail Storage

Messages are stored as plain `.eml` files under `~/.local/share/vivarium/{account}/`:

```
~/.local/share/vivarium/proton/
├── inbox/          <- messages in INBOX
├── archive/        <- archived messages
├── sent/           <- sent mail
├── drafts/         <- work in progress
└── outbox/         <- queued for sending
```

No Maildir `cur/new/tmp`. No UIDs in filenames. No flags. Just email files.

## Commands

```
vivarium init                                  # create config directory and files
vivarium sync                                  # sync all accounts
vivarium sync --account proton                 # sync one account
vivarium list                                  # list inbox (default)
vivarium list sent                             # list sent folder
vivarium show inbox-1                          # read a message
vivarium archive inbox-1                       # move from inbox to archive
vivarium compose --to a@b.com --subject "Hi"   # create a draft
vivarium send ~/Mail/proton/drafts/draft.eml   # send an .eml file
```

All commands accept `--account <name>` to target a specific account. Without it, the first account in `accounts.toml` is used.

## Providers

Vivarium handles the differences between IMAP providers:

| Provider   | `provider =` | Inbox source | Sent source          |
|------------|--------------|--------------|----------------------|
| Gmail      | `"gmail"`    | INBOX label  | [Gmail]/Sent Mail    |
| Standard   | `"standard"` | INBOX folder | Sent folder          |

Gmail syncs `[Gmail]/All Mail` into `archive/`. Standard IMAP (Proton Bridge, Fastmail, etc.) syncs `INBOX` and `Sent` directly.

## Security

- `accounts.toml` is created with `chmod 600` and checked on load
- `password_cmd` is supported as an alternative to plaintext passwords:
  ```toml
  password_cmd = "security find-generic-password -s vivarium -a you@proton.me -w"
  ```
- Self-signed certs are accepted (required for Proton Bridge)

## Status

Early. Working: init, sync, list, show, archive, compose, send. Not yet implemented: watch (IMAP IDLE), reply.

## License

MIT

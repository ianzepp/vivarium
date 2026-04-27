# Vivarium

Local-first IMAP email sync for LLMs. Pulls email from IMAP into standard Maildir folders on disk. No required database or service - just RFC 5322 message files that existing mail tools and local AI can read.

## Why

LLMs need access to email. Existing tools (offlineimap, mbsync, mutt) are built for humans and carry decades of assumptions. Vivarium keeps the storage layer compatible anyway: inbox, archive, sent, drafts, and outbox are Maildir folders. Point an LLM, notmuch, or another local tool at the directory and it has real message files to work with.

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
name = "gmail"
email = "you@gmail.com"
username = "you@gmail.com"
auth = "xoauth2"
token_cmd = "your-command-that-prints-a-gmail-access-token"
imap_host = "imap.gmail.com"
imap_security = "ssl"
smtp_host = "smtp.gmail.com"
smtp_security = "ssl"
provider = "gmail"
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
├── Drafts/
└── outbox/
    ├── tmp/
    ├── new/
    ├── cur/
    └── failed/
```

Vivarium-generated filenames keep a `.eml` stem for non-mail tooling, while `cur/` entries use the usual Maildir info suffix such as `:2,S`.

## Commands

```
vivarium init                                  # create config directory and files
vivarium sync                                  # sync all accounts
vivarium sync --account proton                 # sync one account
vivarium watch --account proton                # watch IMAP and outbox changes
vivarium list                                  # list inbox (default)
vivarium list sent                             # list sent folder
vivarium show inbox-1                          # read a message
vivarium archive inbox-1                       # move from inbox to archive
vivarium compose --to a@b.com --subject "Hi"   # edit and save a draft
vivarium reply inbox-1                         # edit a reply and send it
vivarium reply inbox-1 --body "Thanks"         # send a scripted reply
vivarium send ~/.local/share/vivarium/proton/Drafts/new/draft.eml   # send an .eml file
```

All commands accept `--account <name>` to target a specific account. Without it, account-scoped commands use the first account in `accounts.toml`; `sync` and `watch` operate on all accounts.

## Providers

Vivarium handles the differences between IMAP providers:

| Provider   | `provider =` | Inbox source | Sent source          |
|------------|--------------|--------------|----------------------|
| Gmail      | `"gmail"`    | INBOX label  | [Gmail]/Sent Mail    |
| Standard   | `"standard"` | INBOX folder | Sent folder          |

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
  token_cmd = "your-command-that-prints-a-gmail-access-token"
  ```
- Self-signed certs are accepted by default for compatibility with local bridges
- Set `reject_invalid_certs = true` under `[defaults]` or an account to require certificate validation
- Use `--insecure` as a one-run override when a strict TLS config needs to accept invalid certificates

## Status

Early. Working: init, sync, watch, list, show, archive, editor compose, editor/scripted reply, send, Message-ID dedup, TLS and permission hardening.

## License

MIT

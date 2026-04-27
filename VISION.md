# Vivarium

*A place where messages are preserved and organized.*

Named for the monastery founded by Cassiodorus in the 6th century — a place dedicated to the careful preservation, copying, and study of written works. Vivarium treats email the same way: messages are living documents, stored as files, readable by humans and machines alike.

## What It Is

Vivarium is a local-first, file-native email system written in Rust. It syncs IMAP mailboxes to the local filesystem as Maildir, and sends outbound mail by watching an outbox directory and dispatching via SMTP.

There is no GUI. There is no daemon with an API. Messages are files. The filesystem is the interface.

## Design Principles

1. **Messages are files.** Every email is a single file on disk in Maildir format. You can `cat` it, `grep` it, pipe it, or feed it to an LLM. No database. No index required for basic operation.

2. **The filesystem is the API.** Reading mail means reading files. Sending mail means dropping a file into `outbox/new/`. Archiving means moving a file. Deleting means deleting a file. Any tool that can work with files can work with Vivarium.

3. **Sync, don't serve.** Vivarium pulls remote state to local files and pushes local changes back. It is a synchronizer, not a server. It runs when you tell it to, or watches in the background — your choice.

4. **LLM-native by design.** The file-based architecture is not incidental. It is specifically designed so that language models can read, summarize, draft, triage, and respond to email by operating on the filesystem. No plugins. No integrations. Just files in, files out.

5. **Unix philosophy.** Vivarium does a few things well: sync, store, send, watch. Everything else — display, search, filtering, AI processing — is composed from external tools and pipelines.

## Architecture

```
┌─────────────┐         ┌──────────────┐         ┌─────────────┐
│  IMAP Server │◄───────►│   Vivarium   │◄───────►│    SMTP      │
│  (remote)    │  sync   │   (engine)   │  send   │  (relay)     │
└─────────────┘         └──────┬───────┘         └─────────────┘
                               │
                               ▼
                    ┌─────────────────────┐
                    │   ~/.local/share/vivarium/           │
                    │   ├── account/      │
                    │   │   ├── Inbox/    │
                    │   │   │   ├── cur/  │
                    │   │   │   ├── new/  │
                    │   │   │   └── tmp/  │
                    │   │   ├── Sent/     │
                    │   │   ├── Drafts/   │
                    │   │   ├── Archive/  │
                    │   │   └── ...       │
                    │   └── outbox/       │
                    │       ├── new/      │  ← drop files here to send
                    │       ├── cur/      │  ← sending in progress
                    │       └── failed/   │  ← delivery failures
                    └─────────────────────┘
                               │
                    ┌──────────┴──────────┐
                    │                     │
                    ▼                     ▼
              Shell / CLI            LLM Agents
              (cat, grep,            (read files,
               mblaze, etc.)          write drafts)
```

## Maildir Layout

Vivarium uses standard Maildir with one addition: the `outbox/` directory.

- **`new/`** — Newly arrived, unread messages.
- **`cur/`** — Messages that have been seen. Flags encoded in filename suffixes per Maildir convention (`:2,S` for seen, `:2,F` for flagged, etc).
- **`tmp/`** — Temporary files during atomic delivery.
- **`outbox/new/`** — Place a valid RFC 5322 message here. Vivarium will pick it up, send it via SMTP, and move it to `Sent/` on success or `outbox/failed/` on failure.

## Operations

### Sync

Pull remote IMAP state to local Maildir. Vivarium uses Message-ID sidecar indexes where possible and falls back to UID plus size when a message has no Message-ID.

```
vivarium sync                     # All accounts
vivarium sync --account work      # One account
```

### Watch

Long-running mode. Uses IMAP IDLE for push delivery and filesystem watching for the outbox.

```
vivarium watch                    # All accounts
vivarium watch --account personal
```

### Send

Immediate send of a composed message file.

```
vivarium send path/to/message.eml
```

### List / Show

Basic message listing and display for shell use.

```
vivarium list Inbox
vivarium show <message-id>
```

### Reply / Compose

Opens `$VISUAL`, `$EDITOR`, or `vi` with a properly formatted reply or blank composition. Compose saves a draft; reply sends on save. `reply --body` remains available for scripted sends.

```
vivarium reply <message-id>
vivarium reply <message-id> --body "Thanks"
vivarium compose --to someone@example.com --subject "Hello"
```

## Configuration

```toml
# ~/.config/vivarium/config.toml

[defaults]
mail_root = "~/.local/share/vivarium"
reject_invalid_certs = true

[[accounts]]
name = "work"
email = "you@example.com"
imap_host = "imap.example.com"
imap_port = 993
imap_security = "ssl"
smtp_host = "smtp.example.com"
smtp_port = 587
smtp_security = "starttls"
username = "you@example.com"
password_cmd = "security find-generic-password -s vivarium-work -w"

[[accounts]]
name = "personal"
email = "you@gmail.com"
imap_host = "imap.gmail.com"
imap_port = 993
imap_security = "ssl"
smtp_host = "smtp.gmail.com"
smtp_port = 587
smtp_security = "starttls"
username = "you@gmail.com"
auth = "xoauth2"
oauth_client_id = "your-google-oauth-client-id"
oauth_client_secret = "your-google-oauth-client-secret"
token_cmd = "vivarium token personal"
provider = "gmail"
```

## Non-Goals

- **Not a TUI mail client.** No curses interface, no panes, no keybindings. Use `neomutt` or `aerc` if you want that.
- **Not a search engine.** Use `grep`, `ripgrep`, or `notmuch` for indexing and search.
- **Not an AI agent.** Vivarium provides the substrate — files on disk — that AI agents operate on. It does not embed or invoke any LLM directly.
- **Not a calendar or contacts tool.** Email only.

## Future Possibilities

These are not planned — they are directions the architecture naturally supports:

- **notmuch integration** for fast full-text search and tagging.
- **Hook scripts** triggered on new message arrival (for notifications, auto-filing, LLM triage).
- **Multiple Maildir backends** (local, NFS, FUSE-mounted).
- **JMAP support** as an alternative to IMAP.

## Name

> *Vivarium* — from the Latin, "a place of living things." Cassiodorus founded his monastery at Vivarium in Calabria around 540 AD, establishing it as a center for the preservation and study of both sacred and secular texts. The monks did not merely store — they read, copied, annotated, and transmitted.
>
> So too with email.

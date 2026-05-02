# Pharos Proton Bridge Email Endpoints

Vivi talks to ProtonMail through Proton Bridge exposed on Pharos. Proton Bridge
is the transport and decryption boundary; Vivi stays on the local IMAP/SMTP
side and does not use Proton private APIs.

## Account Defaults

For `provider = "protonmail"`, Vivi defaults to:

- IMAP host: `127.0.0.1`
- IMAP port: `1143`
- IMAP security: `ssl`
- SMTP host: `127.0.0.1`
- SMTP port: `1025`
- SMTP security: `ssl`
- Inbox folder: `INBOX`
- Archive folder: `All Mail`
- Trash folder: `Trash`
- Sent folder: `Sent`
- Drafts folder: `Drafts`

When Pharos exposes Bridge over a different host or tunnel, set the explicit
`imap_host`, `imap_port`, `smtp_host`, and `smtp_port` fields in
`accounts.toml`.

## Discovery

Use the read-only discovery command before enabling remote writes:

```sh
vivi --account personal-proton folders
vivi --account personal-proton folders --json
```

The command reports the resolved Vivi folder roles, the remote folders returned
by IMAP `LIST`, and the key IMAP capabilities needed by later write phases.


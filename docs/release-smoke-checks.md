# Release Smoke Checks

Use this checklist before cutting a release that touches mail transport,
provider routing, Proton API support, sync, indexing, or outbound writes.

These checks intentionally run outside normal CI because they need live account
credentials, local Proton Bridge state, or both. Prefer disposable or dedicated
test accounts.

## Direct Proton API

Use a `provider = "proton-api"` account such as `agent-proton`.

```sh
target/debug/vivi proton session-check --account agent-proton --json
target/debug/vivi sync --account agent-proton --limit 10 --index --json
target/debug/vivi list inbox --account agent-proton --limit 5
```

If the stored session is missing required scopes or cannot refresh, log in once
and retry:

```sh
target/debug/vivi proton login --account agent-proton --json
```

For outbound direct Proton API smoke, create a small draft and send it only to a
controlled recipient:

```sh
target/debug/vivi compose \
  --account agent-proton \
  --from agent@example.com \
  --to recipient@example.com \
  --subject "Vivarium direct Proton API release smoke" \
  --body "Direct Proton API release smoke." \
  --html-body-auto

target/debug/vivi exec send \
  --account agent-proton \
  --from agent@example.com \
  /path/to/generated-draft.eml
```

Then sync the account and confirm any reply decrypts without errors:

```sh
target/debug/vivi sync --account agent-proton --limit 10 --index --json
target/debug/vivi show --account agent-proton --json <reply-handle>
```

## Proton Bridge

Use a `provider = "protonmail"` account with Bridge already running.

```sh
target/debug/vivi doctor --account personal-proton --json
target/debug/vivi sync --account personal-proton --limit 10 --index --json
target/debug/vivi list inbox --account personal-proton --limit 5
```

For Bridge-backed SMTP smoke, create and send a controlled draft:

```sh
target/debug/vivi compose \
  --account personal-proton \
  --from you@example.com \
  --to recipient@example.com \
  --subject "Vivarium Bridge SMTP release smoke" \
  --body "Bridge SMTP release smoke." \
  --html-body-auto

target/debug/vivi exec send \
  --account personal-proton \
  --from you@example.com \
  /path/to/generated-draft.eml
```

## Standard IMAP/SMTP

For any configured standard or Gmail account:

```sh
target/debug/vivi doctor --account <account> --json
target/debug/vivi sync --account <account> --limit 10 --index --json
target/debug/vivi list inbox --account <account> --limit 5
```

Only run outbound send checks against a controlled recipient and account.

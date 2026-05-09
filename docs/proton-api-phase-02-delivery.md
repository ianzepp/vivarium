# Proton API Phase 02 Delivery: Session Storage And Refresh

## Interpreted Problem

Phase 1 proved direct Proton username/password login without Bridge, but it discards the returned session. Containers still need to reuse an initialized session without reusing the account password on every command.

## Normalized Phase Spec

Add a direct Proton session store and two CLI commands:

- `vivi proton login --account <name>` logs in with the configured password/password command and writes direct Proton session state.
- `vivi proton session-check --account <name>` loads the stored session and refreshes it, writing the refreshed session back to disk.

The phase must not print access tokens, refresh tokens, passwords, SRP proof material, private keys, or decrypted key material.

## Repo-Aware Baseline

- Direct Proton auth lives in `src/proton_api.rs`.
- Direct Proton CLI dispatch lives in `src/proton_api_command.rs`.
- Account mail roots are resolved through `Account::mail_path`.
- Existing secret-bearing account and queue files use private filesystem modes.
- `.gitignore` already excludes `.env`; live test credentials should remain untracked.

## Stage Graph

1. Extend Proton API auth to return session material from login.
2. Add refresh-token API support for `POST /auth/v4/refresh`.
3. Add a per-account secret session store under `<mail_root>/.vivarium/proton-session.json`.
4. Add `vivi proton login` and `vivi proton session-check` CLI commands.
5. Add tests for CLI parsing, session-store private permissions, login request shape, and refresh request shape.
6. Run live validation against the `.env` agent account without printing secrets.

## Workstreams

- API client: login return type, refresh request/response parsing, app-version exposure.
- Session store: load/save helpers with `0700` parent directory and `0600` file permissions.
- CLI/runtime: commands, account guard, secret-safe reports.
- Docs/tests: README command examples and unit/CLI tests.

## Checkpoint

A fresh temp-home `provider = "proton-api"` account can run `vivi proton login`, persist session state, and then run `vivi proton session-check` in a separate command invocation using only the stored session and refresh token.

## Gate Plan

- `cargo fmt --check`
- `cargo test`
- Live temp-home `vivi proton login --json`
- Live temp-home `vivi proton session-check --json`
- Manual secret review: no token/password values printed to stdout/stderr or committed.

## Out Of Scope

- Authenticated identity/user/address/key probe.
- Message listing, body download, send, archive, delete, label mutation.
- Token encryption beyond local secret-file permissions; a later phase may add key wrapping if the container deployment needs it.

## Open Questions

- Whether long-lived production containers should provide a wrapping key for session-token encryption.
- Whether TOTP should come from a command/env source rather than only the current CLI flag.

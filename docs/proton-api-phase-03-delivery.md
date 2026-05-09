# Proton API Phase 03 Delivery: Authenticated Identity Probe

## Interpreted Problem

Phase 2 stores and refreshes direct Proton sessions, but it does not yet prove
that Vivi can use those sessions for authenticated API calls beyond auth
itself. Before listing or decrypting mail, Vivi needs a safe identity probe that
confirms the required auth headers and exposes enough non-secret account/address
key state to plan message decryption.

## Normalized Phase Spec

Add `vivi proton identity --account <name>` for direct Proton accounts.

The command must load the stored session, call authenticated Proton identity
metadata endpoints, refresh and retry once on an expired access token, save any
refreshed session, and print only non-secret user/address/key summaries.

## Repo-Aware Baseline

- `src/proton_api.rs` owns direct Proton HTTP calls.
- `src/proton_api/session.rs` owns persisted session state.
- `src/proton_api_command.rs` owns CLI runtime behavior and secret-safe reports.
- Existing Proton CLI subcommands already guard `provider = "proton-api"`.
- Live endpoint probe confirmed current wrappers:
  - `GET /users` returns top-level `User`.
  - `GET /addresses` returns top-level `Addresses`.

## Stage Graph

1. Add sanitized identity response models and summaries.
2. Add authenticated GET helpers that send `Authorization: Bearer`, `x-pm-uid`,
   and `x-pm-appversion`.
3. Add identity fetch with refresh-on-401 retry.
4. Add `vivi proton identity` CLI dispatch and JSON/text reports.
5. Add tests for CLI parsing, authenticated header shape, key-material
   redaction, and refresh-on-401 behavior.
6. Run live validation from a fresh temp-home login.

## Workstreams

- API client: `/users`, `/addresses`, sanitized summary, retry behavior.
- CLI/runtime: command wiring, stored session loading/saving, report printing.
- Tests/docs: CLI parse coverage, mock HTTP behavior, README command example.

## Checkpoint

A fresh temp-home `provider = "proton-api"` account can run `vivi proton login`
and then `vivi proton identity --json`, proving an authenticated direct Proton
API request from stored session state without printing token or key material.

## Gate Plan

- `cargo fmt --check`
- `cargo test`
- `git diff --check`
- Live temp-home `vivi proton login --json`
- Live temp-home `vivi proton identity --json`
- Manual secret review for tokens, passwords, SRP proof material, private keys,
  public keys, signatures, and activation tokens.

## Out Of Scope

- Decrypting user, address, or message keys.
- Listing messages or labels.
- Downloading headers or bodies.
- Sending, deleting, archiving, moving, labeling, or otherwise mutating remote
  mailbox state.

## Open Questions

- Which exact Proton key fields imply decryptability after mailbox key unlock.
- Whether Phase 4 should include a dedicated key-unlock probe before listing
  message metadata.

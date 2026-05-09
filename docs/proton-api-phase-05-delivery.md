# Proton API Phase 5 Delivery: Body Fetch And Decryption

## Phase Goal

Fetch encrypted Proton message payloads directly from the Proton API, unlock the account/address key chain, decrypt message bodies, and ingest reconstructed RFC 5322-compatible blobs into Vivi's existing local storage.

## Inputs

- Roadmap: `docs/proton-api-factory-roadmap.md`
- Existing direct sync: `src/proton_sync.rs`
- Existing Proton client/session modules: `src/proton_api.rs` and `src/proton_api/*`
- Existing storage ingestion: `src/storage.rs` and `src/storage/ingest.rs`
- Test account credentials supplied through local ignored environment/config files

## Scope

- Enable `provider = "proton-api"` accounts with `storage_mode = "bodies"`.
- Fetch full message payloads through `GET /mail/v4/messages/{id}`.
- Fetch private user/address key material only inside the authenticated client path and never expose it through CLI JSON summaries.
- Derive Proton mailbox key passphrases from auth-info salt plus the account password.
- Decrypt address key tokens with the user key, then decrypt message body payloads with the unlocked address key.
- Reconstruct message blobs from Proton's clear `Header` plus decrypted body bytes and ingest them through `Storage::ingest_message`.
- Continue syncing if a single message cannot be decrypted, recording that failure in the local message blob and sync result.

## Out Of Scope

- Attachments.
- Remote mutation such as send, delete, label, archive, or move.
- Semantic embedding/indexing for direct Proton bodies; that remains Phase 6.
- Persisting decrypted key material or mailbox passphrases.

## Implementation Plan

1. Add direct dependencies for PGP message decryption and base64 salt decoding.
2. Extend the Proton API client with:
   - full message fetch with refresh-on-401
   - private key material fetch with refresh-on-401
3. Add a Proton decrypt module that:
   - decodes and normalizes the auth salt
   - derives the mailbox password hash
   - unlocks address key passphrases via token decryption
   - decrypts armored message bodies
4. Update direct sync:
   - allow `storage_mode = "bodies"`
   - fetch/decrypt full bodies per message
   - reconstruct RFC-compatible blobs
   - fall back to a header-only diagnostic blob on per-message decrypt failure
5. Add focused unit tests for salt normalization, RFC reconstruction, and diagnostic recording.
6. Run formatting, tests, a live one-message checkpoint, and a completion gate before commit.

## Acceptance Criteria

- `vivi sync --account <direct-proton-account> --limit 1` works for a `storage_mode = "bodies"` account without Bridge.
- At least one live direct Proton message body is stored locally in decrypted form.
- `vivi show`, `vivi thread`, or `vivi export` can read the locally stored body.
- Failed decryption of an individual message does not abort the whole sync and leaves a clear local diagnostic marker.
- `cargo fmt --check`, `cargo test`, and `git diff --check` pass.
- No passwords, tokens, private keys, PGP message content, or decrypted key material are printed or committed.

## Checkpoint

A clean local cache for the agent Proton account can sync and display at least one real direct Proton message body without Proton Bridge running.

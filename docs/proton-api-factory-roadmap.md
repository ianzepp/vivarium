# Proton API Factory Roadmap

## Phase Set Source

- Goal: `docs/proton-api-direct-goal.md`
- Completed phase spec: `docs/proton-api-phase-01-delivery.md`
- Target repo: Vivarium
- Checkpoint policy: every phase must leave a runnable CLI checkpoint, pass `cargo fmt --check`, pass `cargo test`, and avoid printing or committing secrets.

## Completed

### Phase 1: Auth Bootstrap And Login Check

Status: completed.

Delivered:

- `provider = "proton-api"`
- `vivi proton auth-info`
- `vivi proton login-check`
- Proton SRP proof generation through `proton-srp`
- Live validation against the agent Proton test account with tokens discarded

Checkpoint:

- Direct username/password auth works without Proton Bridge.

### Phase 2: Session Storage And Refresh

Status: completed.

Goal:

Persist direct Proton session state so containers can initialize once and refresh without repeatedly using the account password.

Expected outputs:

- `vivi proton login --account <name>` stores session material under the account's Vivi state directory.
- `vivi proton session-check --account <name>` proves the stored session can refresh or report a clear re-login requirement.
- Stored state includes UID, access token, refresh token, app-version, and enough metadata to diagnose expiry without printing secrets.
- Container-safe storage is implemented without requiring browser auth or macOS Keychain.

Out of scope:

- Message listing, body download, sending, remote mutation.

Checkpoint:

- A fresh temp-home account can login, persist session state, and refresh or validate that session in a second command invocation.

### Phase 3: Authenticated Identity Probe

Status: completed.

Goal:

Use the stored/refreshed session to call authenticated Proton identity endpoints before touching mail.

Expected outputs:

- `vivi proton identity --account <name> --json` reports non-secret user/address/key metadata.
- The client sends the correct access token and `x-pm-uid` headers.
- Locked/unlocked key state is visible enough to decide whether message decryption can proceed.
- Refresh-on-401 behavior is covered.

Out of scope:

- Decrypting mail bodies, storing messages, mutation.

Checkpoint:

- Vivi can make an authenticated direct Proton API request from stored session state and recover from an expired access token.

### Phase 4: Header-Only Direct Sync

Status: completed.

Goal:

Implement direct Proton message metadata listing and ingest it into Vivi as the equivalent of `storage_mode = "headers"`.

Expected outputs:

- Direct Proton accounts can list remote message metadata without Bridge.
- Proton message IDs, conversation IDs, labels/folders, flags, sender, recipients, subject, timestamps, sizes, and attachment hints map into Vivi storage/index rows.
- `vivi sync --account <proton-api-account>` works for `storage_mode = "headers"` without using IMAP.
- Existing `list` and header/metadata search work from the local store after direct sync.

Out of scope:

- Body fetch/decryption, embeddings, send/mutation.

Checkpoint:

- A clean local cache can sync headers directly from Proton and list/search them locally while Bridge is stopped.

## Pending Phases

### Phase 5: Body Fetch And Decryption

Goal:

Fetch encrypted Proton message payloads, unlock the needed account/address keys, decrypt message bodies, and ingest decrypted mail into Vivi's blob store.

Expected outputs:

- Direct Proton accounts can support `storage_mode = "bodies"`.
- Decrypted message bytes or reconstructed RFC 5322-compatible blobs land in the existing content-addressed blob store.
- `show`, `thread`, and `export` work from direct Proton bodies.
- Decryption failures are recorded per message without poisoning the whole sync.

Out of scope:

- Attachments may be deferred unless required for the first complete body checkpoint.
- Send/mutation remains out of scope.

Checkpoint:

- A clean local cache can sync and display at least one real direct Proton message body without Bridge.

### Phase 6: Semantic Direct Sync

Goal:

Reuse Vivi's existing embedding/index pipeline for direct Proton accounts after decrypted bodies exist locally.

Expected outputs:

- `storage_mode = "semantic"` direct Proton accounts use the same fetch/decrypt/cache path as `storage_mode = "bodies"`, then run embeddings as a local post-processing step through `vivi sync --embed` or `vivi index embeddings`.
- Existing semantic and hybrid search paths work without provider-specific special cases beyond the direct sync source.

Out of scope:

- New embedding providers.
- Remote write operations.

Checkpoint:

- A direct Proton account can sync, embed, and return semantic search results without Bridge.

## Stop Conditions

- Stop before bypassing CAPTCHA, human verification, abuse controls, or device-verification flows.
- Stop before printing or committing access tokens, refresh tokens, passwords, SRP proof material, private keys, or decrypted key material.
- Stop before implementing send/delete/archive/label mutation unless a later goal explicitly authorizes it.
- Stop if Proton's API shape changes enough that the phase checkpoint cannot be validated from live evidence.

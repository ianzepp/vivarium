# Proton API Phase 04 Delivery: Header-Only Direct Sync

## Interpreted Problem

Phase 3 proves stored Proton sessions can authenticate, but Vivi still cannot
populate its local mail store without Bridge. Phase 4 should make direct Proton
accounts useful for local-first workflows by syncing message metadata as
header-only records into the existing storage, catalog, list, and index paths.

## Normalized Phase Spec

For `provider = "proton-api"` accounts, make `vivi sync --account <name>` fetch
Proton message metadata directly from the stored session and ingest header-only
messages into Vivi's existing content-addressed store.

The phase must remain read-only against Proton. It must not fetch encrypted
message bodies, decrypt mail, or perform mailbox mutations.

## Repo-Aware Baseline

- `vivarium::sync::sync_account` currently routes all non-zero syncs through
  IMAP and rejects `provider = "proton-api"` inside the IMAP sync layer.
- `Storage::ingest_message` can ingest arbitrary RFC-like bytes plus local
  role/read/starred state into the storage-backed catalog.
- Existing `list`, deterministic index, and search read from the storage-backed
  catalog and metadata tables.
- Phase 2 session storage and Phase 3 authenticated GET helpers can be reused.
- Live endpoint probe confirmed `GET /mail/v4/messages?Page=<n>&PageSize=<n>`
  returns `Messages`, `Total`, and `Limit` with the needed metadata fields.

## Stage Graph

1. Add Proton message-list response models and an authenticated list method with
   refresh-on-401 behavior.
2. Add direct Proton sync routing for `provider = "proton-api"` accounts.
3. Convert Proton message metadata into header-only RFC-like bytes and ingest
   them into existing storage.
4. Map Proton system labels to local roles (`inbox`, `sent`, `drafts`, `trash`,
   fallback `archive`) and preserve read/star/attachment hints as headers.
5. Support `--limit`, `--since`, and `--before` locally against Proton
   timestamps.
6. Add tests for message-list auth headers, header generation, role mapping,
   sync ingestion, and CLI integration through existing sync behavior.
7. Run a live clean-cache checkpoint against the agent Proton test account.

## Workstreams

- API client: message-list paging and sanitized message metadata models.
- Sync integration: provider dispatch, page loop, window/limit filtering,
  storage ingestion, refreshed session save.
- Tests/docs: mock API tests, unit tests for metadata mapping, README mention,
  and live checkpoint.

## Checkpoint

A clean local cache for the agent direct Proton account can run
`vivi sync --account <name> --limit <n> --index`, then `vivi list inbox` and a
metadata search can find synced header-only messages while Bridge is not used.

## Gate Plan

- `cargo fmt --check`
- `cargo test`
- `git diff --check`
- Live cache reset for the agent Proton account, then direct sync with a small
  limit.
- Live `vivi list inbox` and `vivi search ... --json` against the synced
  metadata.
- Manual secret review: no tokens, passwords, SRP proof material, private keys,
  or message bodies printed or committed.

## Out Of Scope

- Fetching encrypted message payloads.
- Decrypting body or attachment content.
- Semantic embeddings for direct Proton accounts.
- Remote send/delete/archive/move/label mutation.
- Durable storage of Proton string IDs in the existing numeric IMAP remote
  binding schema beyond stable local seed/header metadata.

## Open Questions

- Whether to extend the remote binding schema for provider-native string IDs in
  a later phase.
- Whether Phase 5 should add key unlock before body fetch, or split key unlock
  into its own checkpoint.

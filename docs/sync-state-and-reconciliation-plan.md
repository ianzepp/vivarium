# Sync State And Reconciliation Plan

## Problem

Vivi currently treats "new" mail as a derived local comparison: it fetches remote
metadata for a requested window, compares that metadata to local Maildir files
and the RFC Message-ID index, then downloads missing messages. This works, but
it does not persist a last-successful sync cursor.

A simple `last_sync_at` timestamp is not enough. Other processes can mutate the
mailbox between Vivi runs:

- another `vivi` process can archive, delete, move, flag, draft, or send
- a local agent can run `vivi agent ... --execute`
- a phone or Proton client can read, archive, delete, or move messages
- SMTP/provider behavior can create Sent copies outside Vivi's local state

The future implementation needs two related but separate capabilities:

1. fast new-message sync
2. reconciliation of known remote-backed local state

## Goals

- Add a `vivi sync --new` mode that avoids scanning large windows when possible.
- Persist per-account, per-remote-mailbox high-water state.
- Use IMAP UID and UIDVALIDITY as the primary cursor, not timestamps.
- Detect and reconcile messages changed by other Vivi processes or other clients.
- Keep local catalog, Maildir flags, remote identities, and audit records
  coherent under concurrent Vivi processes.

## Non-Goals

- Replacing full/date-window sync.
- Depending on provider private APIs.
- Treating wall-clock timestamps as authoritative mailbox state.
- Holding long-lived locks forever once finer-grained locking is implemented.
- Solving label/provider-special mutation beyond the existing provider-label
  boundary.

## Current Baseline

- `sync --since <window>` builds a `SyncWindow` and passes it into IMAP sync.
- IMAP sync scans configured folders, fetches remote metadata, computes missing
  messages, downloads missing bodies, and then updates catalog/extracted text.
- The local RFC index stores per-message RFC Message-ID, UID, and size, but it
  is not a sync checkpoint.
- Remote identities in the catalog contain account, provider, remote mailbox,
  UID, UIDVALIDITY, RFC Message-ID, size, and fingerprint.
- Mutations already reconcile local state after successful remote writes.

## State Model

Store sync state under the account mail root:

```text
<mail_root>/.vivarium/sync-state.json
```

Proposed shape:

```json
{
  "version": 1,
  "accounts": {
    "personal-proton": {
      "folders": {
        "INBOX": {
          "uidvalidity": 123,
          "highest_seen_uid": 2047,
          "last_success_at": "2026-05-02T18:49:12Z",
          "last_reconcile_at": "2026-05-02T18:49:12Z"
        },
        "Sent": {
          "uidvalidity": 456,
          "highest_seen_uid": 563,
          "last_success_at": "2026-05-02T18:49:12Z",
          "last_reconcile_at": null
        }
      }
    }
  }
}
```

Use remote mailbox names as keys because UID streams are scoped to one mailbox
and one UIDVALIDITY epoch.

## Locking

Add an account-level lock:

```text
<mail_root>/.vivarium/locks/account.lock
```

Use the lock for local critical sections:

- reading/writing `sync-state.json`
- catalog writes
- local Maildir moves/removals/flag changes
- local Sent/Drafts reconciliation
- audit writes that are used for state diagnosis

Initial implementation may hold the account lock for an entire sync. That is
safe and simple. A later optimization can split network and local phases:

1. lock and read state/catalog
2. unlock for remote fetch or remote mutation
3. lock again
4. re-read state/catalog
5. apply local changes only if still valid, otherwise run targeted reconcile

## `sync --new`

For each configured sync folder:

1. Select the remote mailbox.
2. Read current UIDVALIDITY.
3. If there is no state, fall back to current window/full behavior for that
   folder and write initial state after success.
4. If UIDVALIDITY changed, discard that folder cursor and run a fuller
   reconciliation.
5. If state is valid, fetch/search only `UID highest_seen_uid + 1:*`.
6. Download missing message bodies.
7. Update catalog, extraction, and remote identities.
8. Advance `highest_seen_uid` only after local persistence succeeds.

This mode answers "what arrived since the last successful sync for this
mailbox epoch?"

## Reconciliation

`sync --new` alone does not detect moves, deletes, or flag changes for existing
UIDs. Add a reconciliation pass for known catalog entries with remote identity.

Proposed command shapes:

```sh
vivi sync --reconcile
vivi sync --new --reconcile
```

For each account and remote mailbox:

1. Load catalog entries that have remote identity.
2. Group by `remote_mailbox` and `uidvalidity`.
3. Select each mailbox and verify UIDVALIDITY.
4. Fetch known UIDs:
   - `UID`
   - `FLAGS`
   - `RFC822.SIZE`
   - `BODY.PEEK[HEADER.FIELDS (MESSAGE-ID)]`
5. For returned UIDs:
   - mirror read/unread and starred/unstarred flags into Maildir flags
   - refresh remote size/message-id if useful
   - keep catalog remote identity valid
6. For missing UIDs:
   - mark the existing remote identity stale
   - search configured folders by RFC Message-ID and/or fingerprint
   - if found elsewhere, update catalog remote identity and local folder
   - if not found, classify as missing/deleted and apply the chosen local policy

The local policy for missing remote messages should be explicit before
implementation. Conservative default: mark stale in catalog and leave raw local
bytes in place until a later cleanup command is added.

## Interaction With Existing Mutations

Remote mutation commands already perform remote-first execution and then local
reconciliation. The sync-state implementation should extend that by updating
sync state after successful mutations:

- archive/move/delete to Trash: update local catalog location and keep the
  affected folder cursors unchanged unless the destination UID is known
- hard expunge: remove or stale-mark local remote identity
- flag: update local Maildir flags and keep remote identity
- compose/reply remote Drafts APPEND: update Drafts cursor only if APPENDUID is
  available; otherwise rely on next `sync --new`
- send: local Sent copy already exists; remote Sent verification remains a
  reconciliation responsibility unless SMTP/provider behavior is observed

When another Vivi process mutates the same account concurrently, the lock and
post-network re-read step should prevent stale local writes from overwriting
newer catalog state.

## CLI Proposal

Add:

```sh
vivi sync --new
vivi sync --reconcile
vivi sync --new --reconcile
```

Rules:

- `--new` uses UID high-water cursors.
- `--reconcile` checks known remote identities for flags, moves, deletes, and
  stale UIDVALIDITY.
- `--since` and `--before` remain date-window scans.
- `--limit` limits new downloads, not reconciliation checks.
- If `--new` is used with no state for a folder, bootstrap that folder via the
  existing scan path and then write state.

## Tests

Unit tests:

- sync-state load/save round trip
- missing state bootstraps without advancing cursor before persistence
- UIDVALIDITY change invalidates a folder cursor
- `highest_seen_uid` advances only after successful local persistence
- reconcile maps remote `\Seen` and `\Flagged` into Maildir flags
- missing UID produces stale remote identity without deleting raw bytes
- account lock serializes concurrent local state writes

Mock IMAP tests:

- `sync --new` fetches only `highest_seen_uid + 1:*`
- stale UIDVALIDITY triggers fallback/reconcile path
- flag changes made by another client are reflected locally
- message moved by another client is found by RFC Message-ID in another folder

Integration/CLI tests:

- parser accepts `--new`, `--reconcile`, and their combination
- `--limit` applies to new downloads only
- `sync --new --json` if a JSON result mode is added later

## Stop Conditions

Pause implementation if:

- the current catalog cannot represent stale remote identities cleanly
- UIDVALIDITY is missing for a provider folder where mutation/reconcile safety
  depends on it
- lock behavior cannot be implemented portably enough for the current target
  platforms
- reconciling missing remote messages would require deleting local raw bytes
  without an explicit retention policy

## Suggested Implementation Order

1. Add account lock helper.
2. Add sync-state model and load/save tests.
3. Add `--new` parser flag and state bootstrap path.
4. Implement UID high-water fetch for folders with valid state.
5. Add reconciliation data model for stale/missing remote identities.
6. Implement flag reconciliation for known UIDs.
7. Implement missing-UID classification without deleting local raw bytes.
8. Add targeted move discovery by RFC Message-ID across configured folders.
9. Update docs and command help.

# Storage Phase 09: Storage-Only Message Access

## Interpreted Phase Problem

After the catalog entry cleanup, ordinary message access still had a clean-break
gap: `MailStore` would scan legacy Maildir folders when storage rows were empty.
That allowed old `INBOX/new` and `INBOX/cur` files to masquerade as current
message state.

## Normalized Phase Spec

### Goal

Make normal list, read, locate, local-size, and RFC Message-ID lookup paths use
`storage.sqlite` for mailbox roles instead of falling back to Maildir scans.

### Inputs

- `docs/hash-addressed-storage-rewrite.md`
- `src/store.rs`
- `src/retrieve.rs`
- `src/imap/sync/tests.rs`
- store and retrieve tests

### Expected Outputs

- `list_messages` returns storage-backed rows for normal roles, even when empty
- `read_message` and `locate_message` require storage token resolution
- sync dedupe maps come from storage for normal roles
- old Maildir files no longer satisfy normal read/list/dedupe paths
- draft/outbox file mechanics remain scoped to their explicit local workflow

### Out Of Scope

- Replacing draft and outbox file staging with storage rows
- Removing low-level Maildir helper tests that cover draft/outbox mechanics
- Renaming `MailStore` across the codebase

## Checkpoint Target

Old Maildir files should not be treated as current inbox/archive/trash/sent
state, and `cargo test --lib` should pass.

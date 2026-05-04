# Storage Phase 10: Dead Maildir Mutation API Removal

## Interpreted Phase Problem

After normal message access became storage-only, `MailStore` still exposed old
file move and flag mutation helpers that were no longer called by runtime code.
Their tests preserved obsolete local mirror behavior even though mutations now
reconcile through storage rows and remote bindings.

## Normalized Phase Spec

### Goal

Remove dead Maildir move and flag mutation helpers while keeping the explicit
draft/outbox staging functions that are still used.

### Inputs

- `src/store.rs`
- `src/store/mutate.rs`
- `src/store/path.rs`
- `src/store/tests.rs`

### Expected Outputs

- remove `MailStore::move_message`
- remove `MailStore::contains`
- remove `MailStore::file_size`
- remove `MailStore::set_message_flag`
- remove flag-filename helper code used only by the deleted API
- keep draft removal and outbox/draft file staging intact

### Out Of Scope

- Replacing draft/outbox staging with storage rows
- Removing `message_id_from_path`, which is still used by draft/outbox filenames
- Renaming the `MailStore` type

## Checkpoint Target

Runtime code should not expose dead local Maildir move/flag mutation APIs, and
`cargo test --lib` should pass.

# Proton API Phase 05 Cache Delivery: Offline Body Retry

## Objective

Persist encrypted direct-Proton full message payloads as account-local private
cache artifacts before attempting body decryption, then reuse that cache on
later sync runs so body reconstruction can be retried without refetching the
same encrypted blob from Proton.

## Scope

- Add an account-local cache for encrypted `ProtonFullMessage` payloads under
  Vivi's private account state directory.
- Store cache files with private permissions and stable Proton-message-ID based
  names.
- Teach direct Proton `storage_mode = "bodies"` sync to prefer cached encrypted
  payloads before calling the live message body endpoint.
- Preserve the current decrypted RFC-like blob ingestion path.
- Keep private captured blobs out of the repository and release artifacts.

## Non-Goals

- Do not add write/mutation behavior to direct Proton accounts.
- Do not make Proton's private API a documented stable public compatibility
  contract.
- Do not commit live encrypted fixtures.
- Do not implement semantic embedding for direct Proton accounts in this slice.

## Verification

- Unit tests cover cache round-trip, missing-cache behavior, and private file
  permissions on Unix.
- `cargo test` passes.
- Optional live smoke: run a small `agent-proton` sync twice and confirm the
  second run can reuse cached encrypted body payloads.

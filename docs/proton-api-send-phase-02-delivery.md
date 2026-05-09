# Proton API Send Phase 02 Delivery: Clear External Send

## Objective

Move direct Proton API sending from a no-live-send scaffold to the first
network-capable delivery path for simple clear external recipients.

## Scope

- Encrypt created draft bodies to the sender address key before posting the
  Proton draft.
- Build Proton send packages with an encrypted data packet plus `BodyKey` for
  clear external delivery.
- Query recipient public keys before sending and stop if any recipient has an
  active Proton/PGP key.
- Preserve the existing `vivi exec send <draft.eml>` command surface.

## Non-Goals

- Do not send to keyed Proton or PGP recipients until encrypted recipient
  `BodyKeyPacket` support is implemented.
- Do not implement attachments or attachment key packets yet.
- Do not change Bridge-backed SMTP behavior.

## Checkpoint

Direct Proton API sends can now create an encrypted draft and submit a clear
external send package. The implementation intentionally rejects keyed
recipients instead of silently downgrading them to clear delivery.

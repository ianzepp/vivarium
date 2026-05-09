# Proton API Send Phase 03 Delivery: Keyed Recipients

## Objective

Extend direct Proton API sending from clear external delivery to keyed
recipients without routing through Bridge.

## Scope

- Use Proton recipient public-key lookup results to choose clear, internal, or
  PGP-inline send package entries.
- Encrypt the package body session key to keyed recipients as `BodyKeyPacket`.
- Sign outbound encrypted package bodies with the sender address key.
- Keep clear external recipients on `BodyKey` in the same package shape.

## Non-Goals

- Do not implement attachment key packets yet.
- Do not implement PGP/MIME for HTML or multipart external PGP recipients yet.
- Do not change SMTP or Bridge-backed sends.

## Checkpoint

Direct Proton API sends now build encrypted recipient package entries for
Proton/internal and text/plain external PGP recipients. HTML external PGP sends
are blocked until PGP/MIME package support exists.

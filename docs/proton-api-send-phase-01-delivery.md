# Proton API Send Phase 01 Delivery: Outbound Scaffold

## Objective

Prepare Vivi's existing draft-first send surface for direct Proton API sending
without sending live mail until Proton package encryption is implemented and
verified.

## Scope

- Keep the user-facing command surface as `vivi exec send <draft.eml>`.
- Route `provider = "proton-api"` sends away from SMTP and into a direct Proton
  send module.
- Parse outbound `.eml` bytes into the Proton draft template shape.
- Add Proton API client request/response models for create-draft and send-draft
  endpoints.
- Stop before any live direct Proton send with a clear error explaining that
  Proton package encryption is still pending.

## Non-Goals

- Do not send live direct Proton mail in this phase.
- Do not bypass Proton's encrypted package model with raw SMTP-style payloads.
- Do not change Bridge-backed SMTP send behavior.
- Do not implement attachments yet.

## Checkpoint

The codebase has tested request-shape and parsing scaffolding for direct Proton
outbound mail, while attempts to send through `provider = "proton-api"` fail
before network delivery until package encryption is complete.

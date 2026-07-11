# Mailspace Watch Delivery Spec

## Boundary

Implement the project-local `vivi mailspace watch` alarm/stream surface and
the `mail|task|need|want watch` kind aliases. The watcher reads the existing
`mailspace_events` ledger with an event-id cursor, filters locally, and emits
one text or JSON event per match. It must not touch account-scoped watch,
introduce a coordination database, or create stage-gate semantics.

## Implementation stages

1. Add a paged event scan after an exclusive event id, plus an initial cursor
   derived from `--since`. Keep event ordering deterministic.
2. Add watch arguments, parsing, filtering, timeout/poll/once behavior, and
   caller-owned cursor/watermark files with atomic write-back.
3. Add the canonical command and four aliases, then add focused unit and CLI
   coverage for delivery, lifecycle moves, handle filters, cursor advance,
   timeout, and JSON/text output.
4. Document the local watch workflow and its contrast with IMAP watch.

## Checkpoint

The phase is complete when the watch command can block on a local delivery or
lifecycle event, emits the required event fields, advances a caller cursor,
and passes `cargo fmt --check`, `cargo test --test hygiene`, and `cargo test`.

## Validation and stop conditions

Inspect the diff for accidental changes to remote watch. Stop and report if
correct observation requires another database or if the existing event
history cannot provide the required fields without redesigning storage.

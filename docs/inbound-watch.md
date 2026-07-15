# Inbound watch contract

`vivi watch-inbox --account <name> --json` is a pure inbound event source.
It is available in the default build and does not depend on the `outbox`
feature. It selects the account's IMAP `INBOX`, waits with IMAP IDLE when the
server advertises `IDLE`, and otherwise uses a bounded 30-second poll interval.
After every wake, changed inbound mail is synced to the local store before a
JSON Lines event is emitted.

The watcher has no draft, outbox, send, flag, read, directive-classification, or
LLM-wake authority. It never emits message bodies. Existing `vivi sync`
behavior is unchanged; this surface calls an inbound-only sync seam and never
syncs Sent or executes outbound work.

## Event schema

Each line is a bounded-schema JSON object:

```json
{
  "schema": 1,
  "kind": "inbound_mail",
  "account": "agent-proton",
  "source": "imap_idle",
  "observed_at": "2026-07-15T16:00:00Z",
  "batch_id": "agent-proton:imap:INBOX:7:42",
  "new_count": 1,
  "messages": [
    {
      "message_id": "8f…",
      "event_id": "imap:INBOX:7:42",
      "sender": "Operator <operator@example.com>"
    }
  ],
  "cursor": "imap:INBOX:7:42"
}
```

`event_id`, `batch_id`, and `cursor` are stable across reconnects and
restarts. `messages[].sender` preserves the exact parsed sender metadata when
it is valid; malformed or missing sender metadata is `null`. The bridge must
exact-match a non-null sender against its operator/delegate allowlist before
waking Pi. A null or non-matching sender is never a wake authorization.

The bridge must checkpoint these identities/cursors and deduplicate before
delivering a wake. Events are ordered as observed and message/event identities
are deduplicated within each event. A reconnect may re-observe a stable
identity, but must not cause a replay storm downstream. The bridge must not
call `vivi show` or any body retrieval command to classify an event; the
watcher event is the complete classification input.

## Ops wake bridge boundary

Vivi emits every trusted inbound event as soon as local sync succeeds. The Ops
wake bridge owns delivery policy and persists its checkpoint/debounce state; it
must not duplicate this policy in Vivi:

- the first trusted event is eligible for an immediate leading-edge wake;
- later arrivals remain synced and observable immediately, while wake delivery
  is coalesced;
- the trailing wake waits for a 60-second quiet window after the most recent
  arrival, with every arrival resetting that deadline;
- the trailing payload contains the ordered, deduplicated identities and count
  accumulated in the window;
- a restart during the window restores the bridge checkpoint and pending
  identities without replaying the leading wake;
- an event exactly at the deadline is handled by the bridge's inclusive
  boundary rule.

These rules are tested as an executable bridge-policy model next to the source
contract tests. The production watcher intentionally contains no debounce or
LLM wake call.

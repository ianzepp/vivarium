# Goal: Board Performance

## Summary

Make `vivi board` fast enough to run every agent cycle. Currently it times out
at 10 seconds on an 8.5K-message mailspace because it iterates all 22 identities
serially, calling `list_kind` three times each (66 total queries), then loads
events one-at-a-time per candidate message. Target: cold-cache `board` under
500ms for a mailspace of this size, and under 100ms warm-cache.

## Problem

`vivi board` is the heartbeat command for multi-agent fleets — Mind runs it
every cycle to decide what work is actionable. In the faberlang mailspace
(8,527 messages, 22 identities, 9,402 events), `board` takes **>10 seconds**
and gets killed by the 10-second timeout.

The performance test suite (`test-data/perf/01-run-tests.sh`) established a
baseline with this database copy at `test-data/faberlang-vivi/`.

The cold-cache breakdown from the previous round of optimisations:

| Operation | Cost per identity | ×22 identities |
|-----------|-----------------|----------------|
| `list_kind` (tasks) — SQL query + role-determined fast path | ~5ms | ~110ms |
| `list_kind` (needs) — SQL query + role-determined fast path | ~5ms | ~110ms |
| `list_kind` (wants) — SQL query + role-determined fast path | ~5ms | ~110ms |
| `list_mailspace_events_for_messages` (batch, per identity) | ~5ms | ~110ms |
| Rust overhead: struct construction, display handle, JSON | ~? | ~? |
| **Total per identity** | **~?** | **>10s** |

The 66 serial queries + per-message processing adds up. Some identities have
hundreds of messages, and even though the SQL filters now push down to
account+role, the serial identity loop prevents parallelism and pays setup
overhead (Storage::open, schema check) 22 times.

### Specific bottlenecks known or suspected

1. **Serial identity loop.** `build_board` calls `board_identities` which returns
   all 22 identity names, then iterates them one-by-one calling
   `build_identity_board`. Each call opens Storage, runs 3 queries, loads batch
   events. No parallelism.
2. **Redundant Storage::open.** Each `build_identity_board` calls
   `mailspace.list_kind()` which calls `self.storage()` — opening a new SQLite
   connection per identity, running schema version check, setting WAL mode, etc.
3. **list_kind events loading is wasted for board.** `list_kind` does a batch
   events query (for kind matching), then `board_items_with_count` does another
   batch events query. Though we already fixed the N+1, the double load still
   happens.
4. **No message-level parallelism.** Even within one identity, the 3 list_kind
   calls are serial.
5. **Board output is text-formatted with full event lists.** For non-JSON output,
   the event lists are printed inline. We could truncate or summarize.

## Goals

- Make `vivi board` complete in under 500ms cold-cache on the reference
  mailspace (8.5K msgs, 22 identities).
- Make `vivi board` complete in under 100ms warm-cache.
- Do NOT change the board output format (preserve existing JSON and text schema).
- Do NOT reduce the scope of board data (still shows all identities, all
  actionable work types).
- Preserve existing identity aliasing, participant filtering, and `--since`
  semantics.

## Non-goals

- Async/parallel board with tokio — the `board` command is synchronous and works
  on local SQLite. True parallelism across identities would need a separate
  process model.
- Changing the mailspace schema (no new indexes, no schema migration).
- Reducing board content (all existing identity boards must still render).
- Changing the watch/delta surface (that's a separate goal).

## Approach Candidates

These are exploratory directions, not a committed plan. The discovery session
should measure and decide.

**A. Reuse Storage across identities.** Pass a single `&Storage` to all
`build_identity_board` calls instead of letting each `list_kind` open a new
connection.

**B. Merge the three identity queries into one.** Instead of calling
`list_kind("tasks")`, `list_kind("needs")`, `list_kind("wants")` separately,
call `list_messages_by_account_roles(identity, &["tasks", "needs", "wants"])`
once and partition the results. Saves 2 of 3 SQL queries per identity.

**C. Pre-compute event maps for all identities in one pass.** Instead of 22×
batch event queries, load ALL events for all identities in one query
(`WHERE account IN (...) AND local_role IN (...)`) and partition client-side.

**D. Skip full DumpRecord construction.** Board doesn't need the message body,
links, or full event history. It only needs handle, subject, date, from, and
last relevant event. Add a lightweight `board_items` query that avoids the
`dump_records` machinery entirely.

**E. Parallelize the identity loop.** Spawn a thread per identity (or use a
thread pool) so 22 identities run concurrently. Each identity has its own
SQLite connection. Limited by SQLite write-lock contention, but board is
read-only so WAL mode handles concurrent readers fine.

## Invariants

1. `vivi board` output format stays identical — existing fleet minds parse it.
2. `vivi board --json` output format stays identical.
3. `vivi board --since` must still work correctly.
4. `vivi board --for <identity>` scopes to one identity (must be fast already,
   but must not regress).
5. All existing board-related tests in `tests/local_mailspace_cli.rs` pass.

## Acceptance Signals

- `vivi board --project test-data/faberlang-vivi` completes in under 500ms
  cold-cache (first call after boot / `purge`).
- `vivi board --project test-data/faberlang-vivi` completes in under 100ms
  warm-cache (second call).
- `vivi board --for codex --project test-data/faberlang-vivi` completes in under
  100ms (single identity, already fast but must not regress).
- Output matches existing format exactly (text and JSON).
- `cargo test` passes.
- `cargo clippy --all-targets -- -D warnings` passes.

## Ground Truth

- `src/local_board_command.rs`: `build_board`, `build_identity_board`,
  `board_items`, `board_items_with_count`, `board_item`.
- `src/mailspace/delivery.rs`: `list_kind` — the per-identity, per-role query
  that board calls 66 times.
- `src/mailspace/dump.rs`: `dump_records` — the heavyweight machinery that
  board currently bypasses (board constructs its own `BoardItem` struct, not
  `DumpRecord`).
- `src/storage/query.rs`: `list_messages_by_account_roles` — the batched
  account+role query method added in a previous round.
- `src/storage/events.rs`: `list_mailspace_events_for_messages` — batch events
  loading.
- `test-data/faberlang-vivi/`: reference mailspace for benchmarking.
- `test-data/perf/01-run-tests.sh`: performance test harness.
- `test-data/perf/02-diagnose-cycle.sh`: diagnostic timing script.

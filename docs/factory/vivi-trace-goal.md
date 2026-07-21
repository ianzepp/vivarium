# Goal: Add a `vivi trace` command for cross-role communication trees

## Summary

Add a new `vivi trace <handle>` command to Vivarium that, given any local mailspace
handle (task, want, need, mail, or memo), reconstructs the surrounding tree of
communication across fleet roles. The command must work with the data already
stored in Vivi (`mailspace_links`, `mailspace_events`, message bodies, and subject
tags), collapsing multi-copy messages into a single logical node and following
both captured reply chains and inferred cross-references.

## Problem

- Fleet communication in Vivi spreads across many roles (mind, heads, hands,
  operator, auditor) and many message kinds (task, want, need, mail, memo).
- The same logical message exists in multiple folders and role mailboxes, but
  each copy has its own `message_id` and folder path, making it easy to mistake
  a folder copy for a distinct event.
- Reply threading is captured only when `In-Reply-To` / `References` headers are
  present. Many fleet relationships (e.g., "want created from hand report",
  "triage forwarded to head", "task done report forwarded to reviewer") are only
  expressed as handle citations in the body or as lifecycle events.
- There is no single command that shows, for an arbitrary handle, both the
  ancestor chain (higher-level wants/goals/sources) and the descendant chain
  (replies, reports, completion evidence, lifecycle moves).

## Goals

- Add a top-level `vivi trace <handle>` command that resolves any mailspace
  handle and prints a tree of related messages.
- The tree must include ancestor, descendant, and same-content-copy edges.
- Edge sources must be visible: `captured` (reply headers), `event` (lifecycle
  events such as `tasked` or `moved`), `inferred` (body handle citations or
  subject matching), and `copy` (same `content_id` in another folder/account).
- Provide `--json` output for agents and scripts that need to consume the tree.
- Provide `--max-depth` and `--limit` bounds so large mailspaces do not explode.
- Work on the existing Vivi data model without requiring a migration or new
  headers; new optional headers that improve trace quality are a deferred
  enhancement, not a requirement.
- Add targeted tests against the local mailspace CLI and the Faberlang fixture
  data so the trace behavior is reproducible.
- Update the Vivarium README with the new command and a short example.

## Non-goals

- No new mailspace schema migration required for the first version.
- No external API, network, or provider changes.
- No change to how existing commands send mail or create tasks.
- No new runtime goal or persistent background process.
- No attempt to fully reconstruct a goal hierarchy that was never explicitly
  recorded; the command shows the communication graph, not a semantic ontology.
- No support for tracing external (non-mailspace) messages in this phase.

## Ground Truth Researched

- `sqlite3 .vivi/mail.sqlite ".schema"`: tables `messages`, `blobs`,
  `mailspace_links`, `mailspace_events`, `mailspace_item_metadata`.
- `sqlite3 test-data/faberlang-vivi/mail.sqlite ...`: 1459 captured links,
  `tasked` events carry `active_tasks=<handle>` notes, and the same `content_id`
  appears in both sender `sent` and recipient `inbox`/`wants`/`tasks`/`done`
  folders.
- Manual trace of Faberlang handle `d5e0bcf` (task) → want `096bc357` (captured
  link) → head-cto triage `9f254b0` (inferred body citation) → hand-5 report
  `9cdea03` (inferred body citation) → completion report `fc593d5` (captured
  reply).
- `src/mailspace/thread.rs`: existing `Mailspace::thread` already walks
  `mailspace_links` and has `add_inferred_links` for body handle citations and
  subject matching.
- `src/mailspace/lifecycle.rs`: `task_from_source` records `tasked` events with
  `active_tasks=<handle>`; reply headers are captured as links.
- `src/cli.rs` and `src/local_mailspace_command.rs`: top-level dispatch and
  mailspace subcommand wiring; new command belongs here alongside `task`,
  `need`, `want`, `mail`.

## Reference Packet

Before editing, inspect:

- `src/mailspace/thread.rs`: existing thread graph builder and inference logic.
- `src/mailspace/lifecycle.rs`: `task_from_source`, `record_tasked_source`, and
  event emission.
- `src/mailspace/dump.rs`: `DumpRecord` shape, event loading, and link loading.
- `src/storage/handles.rs`: handle resolution and `short_handle_map` semantics.
- `src/storage.rs`: `Storage` struct, message queries, event queries.
- `src/cli.rs`: top-level `Command` enum.
- `src/local_mailspace_command.rs`: dispatch for mailspace commands.
- `tests/local_mailspace_cli.rs`: existing local-mailspace CLI tests.
- `test-data/faberlang-vivi/mail.sqlite`: rich fixture for manual and
  automated trace verification.
- `README.md`: current CLI reference section.

## Constraints And Invariants

- Keep all production errors in `VivariumError` with `thiserror`; no `anyhow`.
- Avoid panics in production paths; use `Result` and `unreachable!` only for
  true invariants.
- Use `clap` derive for the new command parser.
- Maintain the 1000-line file ceiling and 60-line function ceiling for checked
  `src/**/*.rs` files.
- Prefer existing modules and helper APIs before adding new abstractions.
- No new dependencies for logic covered by the standard library or existing
  crates.
- The command must be local-mailspace only; it does not operate on external
  account stores.
- Preserve backward compatibility for existing CLI commands and storage
  schema.

## Supporting Skills

- `factory`: overall multi-phase execution loop.
- `delivery`: compile the implementation into a stage graph and delivery spec.
- `correctness`: behavioral audit of graph traversal and edge classification.
- `cleanliness`: structural pass on new modules.
- `polish`: per-file cleanup after implementation.
- `bonsai`: readability and naming review.

## Implementation Shape

- Phase 1 (this factory run): implement `vivi trace` with captured-link,
  event-link, inferred-link, and copy edges; text and JSON output; bounded by
  `--max-depth` and `--limit`; tests on the Vivarium local mailspace and the
  Faberlang fixture; README update.
- Later (deferred): optional `X-Vivi-Trace-Root` / `X-Vivi-Trace-Parent`
  headers that make goal/feature ancestry explicit; topic-clustering edges from
  subject tags; richer text rendering (e.g., Mermaid or indented tree variants).

## Release Posture

Decision: defer release until the feature has been used on real fleet data.

- No version bump or release tag in this phase.
- A README update is sufficient operator-facing documentation for now.
- If the user later requests a release, add a changelog entry and bump the
  crate version following the existing `release-v*.md` convention.

## Exit Strategy

Decision: included.

- If the command proves too noisy on real data, the trace edge classes can be
  gated behind `--include` flags and defaults can be tightened without changing
  the schema.
- If a better relationship model (explicit trace headers) is added later, the
  existing graph builder can ingest those edges without removing the
  legacy-data inference path.

## Acceptance Criteria

- `vivi trace <handle>` resolves an arbitrary mailspace handle and prints a
  tree with at minimum the seed node and its directly linked ancestors/
  descendants.
- JSON output contains a stable node/edge structure that includes: handle,
  message_id, content_id, account, role, kind, date, from, to, subject, and a
  list of `edges` with `target`, `source`, and `direction`.
- The tree collapses multi-copy messages (same `content_id`) so a sender copy
  and a recipient copy do not appear as unrelated nodes.
- Running `vivi trace d5e0bcf` against the Faberlang fixture reaches the parent
  want `096bc357` and the completion report `fc593d5`.
- `cargo fmt --check`, `cargo test --test hygiene`, `cargo test --test
  local_mailspace_cli`, and `cargo test` pass.
- README documents the new command and gives a short example.

## Validation

- `cargo fmt --check`
- `cargo test --test hygiene`
- `cargo test --test local_mailspace_cli` (extend with trace tests)
- `cargo test`
- Manual: `vivi trace d9c2ae04` in the Vivarium `.vivi` mailspace shows the
  task and its reply.
- Manual: direct SQLite trace against the Faberlang fixture confirms the
  command output matches the expected ancestor/descendant set.

## Open Questions

- Should the top-level command be `vivi trace` or `vivi mailspace trace`? The
  goal assumes `vivi trace` for ergonomics; final decision can be made during
  delivery planning if dispatch wiring makes another placement materially
  simpler.
- Should the default depth be large enough to show the full reply chain, or
  conservative? Default 5 is proposed; can be adjusted during implementation.

## Stop Conditions

- Stop if implementing the command requires a storage schema migration or a
  breaking change to existing CLI commands.
- Stop if the Faberlang fixture cannot be used for automated testing without
  restructuring the test data.
- Stop if adding the command causes existing `cargo test` or hygiene checks to
  fail and the root cause is not clearly traceable to this change.

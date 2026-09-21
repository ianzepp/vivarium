# Vivarium 9.4.0

Vivarium 9.4.0 is a structural release. The email half of the codebase moves
into its own crate, `vivi-mail`, as the first step toward publishing it as a
separate repository in 10.0.0.

**Nothing about the CLI, the mailbox, or the mailspace changes.** Every command
parses and behaves exactly as in 9.3.0, the on-disk layout is untouched, and no
database or configuration migration is required. The release exists so the
10.0.0 repository split starts from a tree whose halves are already separate.

## Why

The mailspace is the surface Vivi is used for day to day. The email transport
and archive stack — IMAP, SMTP, the direct Proton API, sync, send, indexing,
embeddings, search, drafts, and the outbox — is built into the same binary but
is largely unused in day-to-day work. Splitting the crates first makes the
later repository extraction a path move, and it keeps the two halves from
accumulating new dependencies on each other while they are still one tree.

## What moved

`vivi-mail` now owns:

- Transport: IMAP, SMTP, the direct Proton API, OAuth, and Proton session sync.
- Write paths: send, draft and reply composition, provider labels, the
  outbox, and the durable review queue.
- Local archive: the catalog, the deterministic metadata index, embeddings,
  keyword and semantic search, retrieval, threading, and text extraction.
- The agent mailbox and the mail runtime itself (`Runtime`), which owns the
  loaded `config.toml` and `accounts.toml`.

The root package keeps the project mailspace: boot, roles and cadences, goals,
tasks, needs, wants, memos, work graphs, and the board.

The CLI splits the same way. Mail argument structs and their subcommand
modules live in `vivi_mail::cli`; the root `Command` enum holds them in tuple
variants, so `vivi sync`, `vivi list`, and the rest of the command list stay
flat and unchanged.

## Compatibility notes for this stage

These are implementation details of the transition, not behavior changes, and
they resolve in 10.0.0:

- `VivariumError` is one type shared by both crates, re-exported by the root.
  The 10.0.0 repositories each carry their own copy.
- The mailspace's outbound delivery reuses the mail crate's message
  composition, `.eml` reader, and JSON renderer rather than carrying a second
  parser.
- `duration` parsing is duplicated, because both halves need it.

## Validation

`cargo fmt --check` is clean and `cargo test --workspace` passes, including the
CLI parsing suite that covers both halves of the command list. The hygiene
ratchet is now one per crate, with budgets re-derived from each crate's own
counts rather than copied, so neither crate inherits the other's allowance.

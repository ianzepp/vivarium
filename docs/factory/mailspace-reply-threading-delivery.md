# Mailspace Reply Threading Delivery Spec

## Boundary

Implement authoritative cross-kind reply capture, project-local thread views,
note-as-reply lifecycle behavior, opt-in historical inference, and dump JSON
surfacing over the existing mailspace storage. Use `content_id` as the stable
logical-message identity and a single additive `mailspace_links` relation.
Do not change account email threading, network providers, folder identities,
or add gate/coordination storage.

## Implementation stages

1. Add the additive link schema and storage seams. Extend local send with
   `--reply-to`, add `mail reply`, resolve stable handles, write reply headers,
   and expose captured parent metadata in dump JSON.
2. Add a kind-agnostic thread assembler that walks ancestors and descendants
   from any node, with text/JSON output, caps, and task/need/want show context.
3. Make every existing lifecycle `--note` operation create a reply while
   preserving the event note and keeping move, event, message, and link writes
   atomic.
4. Add default-off in-memory inference from handle citations, reply subjects,
   and deterministic prior-message ordering. Mark inferred links and never
   override captured links.
5. Add integration coverage for cross-kind replies, forks, stable handles,
   notes, inference, and failure cases; update README and release notes.

## Checkpoint

The phase is complete when replies and notes assemble into a full thread from
any handle, captured and inferred links remain distinguishable, existing
mailspace semantics remain intact, and `cargo fmt --check`,
`cargo test --test hygiene`, and `cargo test` pass.

## Validation and stop conditions

Inspect the diff for network/account-store changes and for any second
coordination table. Stop and report if stable handle resolution across folder
moves cannot be preserved or if tests would need a weakened hygiene/policy
rule.

## Result

**Complete** on `main` as part of `48482f6 feat(mailspace): add watch and
reply threading` (with follow-on polish commits on storage/reply/thread).
Validation re-run after polish closeout: `cargo fmt --check`,
`cargo test --test hygiene`, and `cargo test` all green. No second
coordination store; links live in additive `mailspace_links`.

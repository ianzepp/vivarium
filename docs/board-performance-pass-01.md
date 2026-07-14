# Board Performance Pass 01

## Invariant

`--for <identity> + <handle>` is the canonical scoped message token for identity-bound mailspace work commands. Display handles are minimum 8-character message-id prefixes, unique within the relevant identity scope when that scope is known.

## Scope

- Make short-handle computation linear-ish instead of quadratic.
- Add account/identity-scoped handle decoration and resolution.
- Use scoped resolution for identity-bound item moves.
- Make `list_kind` use account+role SQL and skip blob/event kind checks when role already determines kind.
- Make `vivi board` load work messages and events in batches, then partition by identity/role.

## Validation

- Release-build benchmark against `test-data/faberlang-vivi`.
- `cargo fmt --check`.
- `cargo test --test hygiene`.
- `cargo test --test local_mailspace_cli`.

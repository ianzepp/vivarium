# Delivery Spec: `vivi mail list --from` / `--to`

Status: **complete**

Shipped on `main` as `feat(mail): add --from and --to filters to mail list`.
`--for` remains mailbox scope. `--from` / `--to` are header filters. At least
one of the three is required. Hygiene budgets raised to the new production
totals (33_437 lines, 1_209 functions).

Parent goal: `docs/mailspace-filter-search-goal.md` (slice 1, mail list
only — not verdict filters, project search, dump exit, or task/need list).

## Interpreted Unit

Agents and operators keep typing `vivi mail list --from mind` and
`vivi mail list --to hand`. Those flags do not parse. `mail list` only
accepts required `--for` (mailbox owner). `--from` / `--to` already exist
on send/reply (addresses) and on dump (header filters).

This unit adds **true header filters** on `mail list`, not aliases of
`--for`. `--from` meaning `--for` would list the sender's inbox (mail
*to* them). That is the opposite of the guessed command.

## Normalized Spec

### Functional requirements

1. `vivi mail list` accepts optional `--from <identity-or-address>` and
   optional `--to <identity-or-address>`.
2. `--for` remains mailbox scope (account copies in `--folder`, default
   `inbox`). It becomes optional when `--from` or `--to` is present.
3. At least one of `--for`, `--from`, `--to` is required.
4. `--from` filters `from_addr`. `--to` filters `to_addr` or `cc_addr`.
5. Known roles resolve to their local address before matching
   (`mind` → `mind@<project>.local`). Unknown values fall back to
   case-insensitive substring, same as dump.
6. Existing `mail list --for <id>` output and JSON shape stay the same
   when `--from`/`--to` are omitted.
7. Combine freely: `--for hand --from mind` is hand's inbox from mind.

### Constraints

- True flags, not clap aliases of `--for`.
- No new crate dependencies.
- Errors via `VivariumError`; clap derive; no panics in production paths.
- File ≤ 1000 lines; functions ≤ 60 lines.
- Reuse dump's header-match semantics; do not invent a second matcher.

### Non-goals

- Aliasing `--from` or `--to` to `--for`
- `--from` / `--to` on `task list` / `need list` / `want list` / `board`
- Hard `--limit`, verdict filters, project `mail search`
- Dump removal or dump UX changes
- Changing send/reply `--from`/`--to` meaning
- Growing dump as the analysis product

## Repo-Aware Baseline

| Surface | Role |
| --- | --- |
| `src/cli/mailspace_command.rs` | `MailCommand::List` — `--for` required `String`; no from/to |
| `src/local_mailspace_command.rs` | Dispatch into `list_local_mail` |
| `src/local_mail_list.rs` | Render; calls `Mailspace::list(identity, role)` |
| `src/mailspace/delivery.rs` | `list(identity, role)` filters by account names |
| `src/mailspace/dump.rs` | Proven `--from`/`--to`/`--participant` matching |
| `src/mailspace/identity.rs` | `resolve_identity`, `address_for` |
| `tests/cli.rs` | `parses_local_mail_list_with_json_and_project` |
| `tests/local_mailspace_cli.rs` | Inbox/sent list after send |
| `README.md` | Documents `mail list --for` only |

`--for` = mailbox account. `--from`/`--to` on dump = header substring.
Those are different axes; list must keep both.

## Stage Graph

One logical change. Splitting CLI / filter / tests / README would make
process larger than the product change.

```text
[CLI flags + optional --for] → [list + header filter] → [tests + README]
```

| Hand | Output | Done when |
| --- | --- | --- |
| H1 | Flags, list behavior, parse + CLI tests, README | Commands below work; existing `--for` list unchanged |

## Implementation Work

**H1 — mail list from/to filters**

- Write scope: CLI list shape, `list_local_mail` / `print_mail_list`,
  optional `Mailspace` folder-wide list, `tests/cli.rs`,
  `tests/local_mailspace_cli.rs`, `README.md`
- Done when:
  - `vivi mail list --from ceo` lists inbox copies from ceo
  - `vivi mail list --to cto` lists inbox copies to/cc cto
  - `vivi mail list --for cto --from ceo` intersects mailbox + sender
  - `vivi mail list` with none of `--for`/`--from`/`--to` fails clap
  - `vivi mail list --for cto` still lists that inbox

## Checkpoints And Gates

**Batching / Split Decision:** one Hand. Same behavior family, same
write surface, same tests.

**Hand sanity:** parse tests + one send/list integration covering from,
to, combined, and empty-scope rejection.

**Lane-owned:**

- lint: `cargo fmt --check`
- test: `cargo test --test hygiene`; `cargo test --test cli --test local_mailspace_cli`
- merge: parent commit after factory closeout

**Release:** `defer-release`. User-visible CLI, but no version bump in
this unit. Reconsider at the next 7.2.x / 7.3.0 checkpoint.

## Validation

1. `cargo fmt --check`
2. `cargo test --test hygiene`
3. `cargo test --test cli parses_local_mail_list`
4. `cargo test --test local_mailspace_cli mail_list_from_to`
5. Existing `local_mail_send_creates_readable_inbox_and_sent_copy` still
   passes ( `--for` path unchanged)

## Companion Skill Plan

After ship: `vivi` skill board/list snippet should show `--from` / `--to`
as list filters, distinct from send. That skill lives outside this repo.

## Open Questions

None blocking. Deferred to the parent goal: `--limit`, task/need list
parity, verdict filter, project search, dump exit.

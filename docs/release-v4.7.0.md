# Vivarium 4.7.0

Vivarium 4.7.0 adds identity rename to the project-local mailspace on top of
the 4.6 watch/reply/thread surface.

## Highlights

- Adds `vivi mailspace identity rename <old> <new> [--project]`. Renaming
  updates the roster entry in place and keeps the old name as an alias.
- Renaming never rewrites stored `.vivi/mail.sqlite` message rows: mail
  already delivered under the old name stays under that name in storage.
- The old name keeps resolving everywhere an identity is accepted
  (`--from`, `--to`, `--for`, `mailspace watch --for`, dump `--for-identity`
  and `--participant`), and always resolves to the current canonical name.
- `mailspace status`, `identity list`, `task`/`need`/`want`/`mail list`, and
  `dump` aggregate historical mail under the renamed identity by matching
  the current name plus all recorded aliases, so counts and listings don't
  silently drop history after a rename.
- `identity list` and `mailspace status` print a `formerly: <name>` line
  under any identity with recorded aliases.
- Rejects renaming to a name already in use as another identity's current
  name or alias, and rejects renaming an unknown identity.

## Migration

Existing `mailspace.toml` files gain an `aliases` field on each identity via
serde default; no manual migration is needed. Existing `.vivi/mail.sqlite`
databases are untouched by this release — rename is a roster-only, alias-only
operation and performs no bulk rewrite of stored message rows.

## Release Checks

Before publishing the tag, run:

```sh
cargo fmt --check
cargo test --test hygiene
cargo test
cargo build --release
target/release/vivi --version
```

This release does not change provider routing, sync, or send paths, so live
provider smoke checks from `docs/release-smoke-checks.md` are optional rather
than required for 4.7.0.

## Publication

GitHub release assets and the Homebrew tap (`ianzepp/homebrew-tap` formula
`vivarium`) are part of this release contract. Crates.io is not.

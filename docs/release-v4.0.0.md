# Vivarium 4.0.0

Vivarium 4.0.0 is the direct Proton API release.

## Highlights

- Adds `provider = "proton-api"` as a direct-to-Proton account path.
- Supports non-interactive Proton login, session refresh, identity checks,
  header sync, encrypted body fetch/decryption, and semantic indexing without
  Proton Bridge.
- Adds direct Proton API sending through Vivi's existing draft-first
  `vivi exec send` surface.
- Supports clear external recipients, Proton/internal recipients, and
  text/plain external PGP recipients for direct Proton sends.
- Keeps Proton Bridge, standard IMAP, Gmail-style IMAP, SMTP send, queues,
  local storage, lexical search, and semantic search available.

## Breaking Framing

The major-version bump reflects the project-level shift from an IMAP-first mail
archive into a provider-routed mail layer where direct Proton API accounts are a
first-class path alongside Bridge and standard IMAP/SMTP accounts.

Existing `provider = "protonmail"` Bridge accounts remain supported.

## Release Checks

Before publishing the tag, run:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

For live provider smoke checks, use `docs/release-smoke-checks.md`.

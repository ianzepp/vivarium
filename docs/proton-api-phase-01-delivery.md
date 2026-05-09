# Proton API Phase 01 Delivery: Auth Bootstrap Probe

## Interpreted Problem

Bridge blocks fully automated container initialization because a human must add each Proton account to Bridge and copy Bridge-generated credentials into Vivi. The first direct-Proton milestone should not pretend to solve sync. It should prove that Vivi can model a direct provider and contact Proton's auth bootstrap API for an account without Bridge.

## Normalized Phase Spec

Add an experimental `proton-api` provider plus narrow `vivi proton auth-info` and `vivi proton login-check` commands. The commands select a configured account, verify it uses `provider = "proton-api"`, call Proton's SRP bootstrap endpoint, and, for `login-check`, generate a password proof and submit `POST /auth/v4` without storing or printing returned tokens.

## Repo-Aware Baseline

- Provider parsing lives in `src/config/types.rs`.
- Account secret resolution lives in `src/config/account.rs`; phase one should not need to print or transmit the password because `/auth/v4/info` only needs the username.
- CLI command parsing lives in `src/cli.rs`.
- Runtime dispatch lives in `src/main.rs`.
- Connectivity reporting can follow the style of `src/doctor_command.rs`.
- HTTP client usage already exists through `reqwest` for OAuth and embeddings.
- Tests live in `src/config/tests.rs` and `tests/cli.rs`.
- Proton validates the `x-pm-appversion` header. Phase one defaults to the
  current `web-mail@5.0.113.4` version discovered from
  `https://mail.proton.me/assets/version.json` and allows
  `VIVI_PROTON_APP_VERSION` to override it.

## Stage Graph

1. Add `Provider::ProtonApi` with TOML spelling `proton-api`, display output, and tests.
2. Add a `Proton` CLI subcommand group with `auth-info` and `login-check`.
3. Add a small `proton_api` module that posts to `/auth/v4/info`, deserializes SRP bootstrap metadata, and uses Proton's Rust SRP implementation to perform a token-discarding login check.
4. Add runtime dispatch that requires `provider = "proton-api"` and prints text or JSON output.
5. Update docs to show the experimental account shape and first probe command.
6. Run formatting and tests.

## Workstreams

- Config: provider enum, parser coverage, provider-specific guard helpers if needed.
- CLI/runtime: command shape, account resolution, output.
- HTTP module: minimal request/response types, default base URL, error messages.
- Docs/tests: README note and parsing/unit coverage.

## Checkpoint

Phase 1 passes when the repo exposes an explicit direct Proton API account type and non-interactive bootstrap/login-check commands that can be run from a container without Bridge. It is acceptable for this phase to stop before token storage, authenticated metadata fetch, and message sync.

## Gate Plan

- `cargo fmt --check`
- `cargo test`
- Manual code review for secret leakage: the command must not print passwords or tokens.
- Manual code review for provider isolation: Bridge-backed `protonmail` behavior must remain unchanged.

## Open Questions

- Session storage and authenticated metadata fetch are intentionally deferred.
- Live validation needs a real configured Proton test account; automated tests should avoid hitting Proton.

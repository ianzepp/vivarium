# Goal: Direct Proton API Backend

## Summary

Add an experimental direct Proton API backend to Vivi so isolated agent containers can initialize email access from Proton account credentials without provisioning Proton Bridge, generated Bridge passwords, or shared local Bridge state.

## Problem

- Vivi currently reaches Proton Mail through Proton Bridge over local IMAP/SMTP.
- Bridge requires an interactive account setup flow, stores its own sessions, and generates a separate mailbox password that must be copied into Vivi.
- Containerized agent backgrounds need one Proton account per isolated container; Bridge creates a manual provisioning hole in otherwise scriptable initialization.
- Running one shared Bridge couples independent containers to shared mutable infrastructure; running Bridge inside every container still requires manual setup.

## Goals

- Introduce a distinct experimental provider path for direct Proton API work instead of changing the existing Bridge-backed `protonmail` provider.
- Let a container provide a Proton username/password or secret command to Vivi and run a non-interactive connectivity/authentication bootstrap.
- Reuse Vivi's existing storage modes and local blob/index architecture once direct message fetch/decryption is implemented.
- Keep Bridge support as the conservative fallback for users who want the supported IMAP/SMTP surface.
- Make the first milestone prove contact with Proton's API shape before attempting message sync.

## Non-goals

- Do not remove or degrade the current Proton Bridge provider.
- Do not implement remote mutation, sending, archiving, deletion, or label writes in the first milestone.
- Do not expose account secrets to agent worker processes.
- Do not treat Proton's internal API as a stable public contract.
- Do not build a Bridge automation wrapper as the long-term solution.

## Ground Truth Researched

- `README.md`: Vivi currently documents Proton Bridge as the transport/decryption boundary and supports `storage_mode = "headers" | "bodies" | "semantic"`.
- `docs/pharos-bridge-email.md`: Pharos/Bridge docs assume local IMAP/SMTP endpoints and explicitly avoid Proton private APIs.
- `src/config/types.rs`: providers are currently `standard`, `gmail`, and Bridge-backed `protonmail`.
- `src/config/account.rs`: accounts already support `password_cmd`, `token_cmd`, storage modes, and provider-specific defaults.
- `src/imap/sync.rs` and `src/imap/sync/download.rs`: current sync is IMAP-specific but now separates header/body/full fetch behavior.
- `src/oauth.rs`: existing OAuth code is browser/keychain oriented and not suitable for headless Proton password login.
- Proton `go-proton-api` `manager_auth.go`: login starts with `POST /auth/v4/info`, then uses SRP proof generation and `POST /auth/v4`.
- Proton `go-proton-api` `manager_auth_types.go`: auth bootstrap returns SRP version, modulus, server ephemeral, salt, session, and 2FA metadata.
- User conversation: the desired deployment model is Docker containers with isolated agent backgrounds, each initialized from a Proton username/password without manual Bridge provisioning.

## Reference Packet

Before editing, inspect:

- `docs/proton-api-phase-01-delivery.md`: selected phase boundary and checkpoint.
- `src/config/types.rs`: provider and account config model.
- `src/config/account.rs`: secret resolution and provider defaults.
- `src/cli.rs`: CLI command shape.
- `src/main.rs`: runtime command dispatch.
- `src/doctor_command.rs`: existing account connectivity-report pattern.
- `src/lib.rs`: exported library modules.
- Proton `go-proton-api` `manager_auth.go` and `manager_auth_types.go`: current SRP bootstrap and login endpoints.

## Constraints And Invariants

- Existing `provider = "protonmail"` remains Bridge-backed IMAP/SMTP.
- Direct Proton API support must use a new provider name, `proton-api`.
- First-phase code may call unauthenticated Proton API bootstrap endpoints, but must not log passwords, access tokens, refresh tokens, or SRP proof material.
- `password_cmd` remains the preferred secret source for automation.
- Container use must not require macOS Keychain or interactive browser auth.
- The feature is experimental because Proton's Mail API is not a documented public compatibility contract.
- All new code must preserve `cargo test` and `cargo fmt --check`.

## Supporting Skills

- `goal-forge`: used to turn the problem into this durable goal.
- `factory`: use to execute one implementation phase at a time with saved delivery specs and commits.
- `mail`: use when touching Vivi, Proton Bridge, IMAP/SMTP, or mail workflow behavior.

## Implementation Shape

- Phase 1: Add `provider = "proton-api"` config support plus `vivi proton auth-info` and `vivi proton login-check` probes that can call Proton's auth bootstrap endpoint and perform token-discarding SRP login verification.
- Phase 2: Implement optional 2FA handling, local encrypted session storage suitable for containers, and authenticated session refresh.
- Phase 3: Fetch account/user/address/key metadata and prove authenticated API requests without message download.
- Phase 4: Implement read-only message listing and raw/encrypted payload fetch with local decryption into Vivi storage.
- Later: Add attachment handling, incremental sync state, label/folder mapping, and carefully gated send/mutation support.

## Exit Strategy

Decision: included.

- Keep Bridge-backed `provider = "protonmail"` intact as the stable fallback.
- Gate direct API behavior behind explicit `provider = "proton-api"` and `vivi proton ...` commands.
- Treat failed 2FA, CAPTCHA, human verification, or API-shape drift as stop conditions rather than attempting evasive automation.
- The direct backend can be removed without breaking existing Bridge accounts as long as `protonmail` remains unchanged.

## Acceptance Criteria

- A goal document and phase delivery specs exist under `docs/`.
- A config account can choose `provider = "proton-api"`.
- A CLI command exists to probe Proton API auth bootstrap for a selected account.
- The probe is non-interactive and can use `password_cmd` without printing the secret.
- Tests cover provider parsing and CLI parsing for the new command.
- The first implementation phase is committed separately from the pre-existing storage-mode work.

## Validation

- `cargo fmt --check` should pass.
- `cargo test` should pass.
- `vivi proton auth-info --account <proton-api-account> --json` should contact Proton and return non-secret SRP bootstrap metadata when valid credentials/account identity are configured.
- Manual review should confirm no password, access token, refresh token, SRP proof, or session token is printed.

## Open Questions

- How should container session storage encrypt refresh tokens when no OS keychain is available?
- Should phase 2 support TOTP input via env/command, or stop and report that 2FA must be disabled for isolated agent accounts?

## Stop Conditions

- Stop before bypassing CAPTCHA, human verification, or anti-abuse controls.
- Stop before sending mail or mutating remote mailbox state.
- Stop before storing Proton session tokens without an explicit storage design.
- Stop if Proton changes the auth bootstrap response shape enough that phase-one metadata cannot be validated.

# Phase 01: Proton Bridge Read-Only Ingest

## Interpreted Problem

Phase 00 established the local archive thesis and quarantined send/reply surfaces.
Phase 01 makes Proton Bridge the first-class read-only ingestion path: Bridge-friendly
defaults, clear errors for Bridge-specific failure modes, and proven read-only ingest.

## Phase Spec

### Goal

Make Proton Bridge the first-class read-only ingestion path.

### Inputs

- Proton Bridge local IMAP host/port behavior
- existing account config
- existing IMAP transport (src/imap/transport.rs)
- existing Maildir store (src/store.rs)

### Expected Outputs

1. `provider = "protonmail"` / bridge-friendly config documented and tested
2. Read-only IMAP ingest from Proton Bridge into raw `.eml` Maildir (existing, verified)
3. Bridge-friendly defaults for localhost, security mode, and credential command
4. No SMTP requirement for ingest (already true — outbox is quarantined)
5. Clear errors for Bridge not running, bad Bridge credentials, and TLS/cert mismatch

### Out Of Scope

- Proton private API
- SMTP/send
- embeddings
- global historical import performance tuning

### Checkpoint Target

A configured Proton Bridge account can ingest messages into preserved Maildir files
using only local IMAP access.

## Workstreams

### WS-01-A: Protonmail Provider Defaults

- Add Proton Bridge default values to the account config (localhost:1143, SSL, self-signed certs)
- Update `Provider::Protonmail` to suggest Bridge defaults in `oauth_config()` or a new
  `bridge_defaults()` method
- Add `imap_port` defaulting for Protonmail (1143 for SSL, 1144 for STARTTLS)
- Add `smtp_port` defaulting for Protonmail (1025 for SSL, 1024 for STARTTLS)
- When provider is Protonmail and `imap_host` is not set, default to "127.0.0.1"

### WS-01-B: Better Bridge Error Messages

- Detect common Bridge failure modes in `imap/transport.rs::connect()`:
  - Connection refused → "Proton Bridge is not running on localhost:{port}"
  - TLS cert errors → "TLS certificate rejected; set reject_invalid_certs = true if using a self-signed Bridge cert"
  - Auth failure → "Bridge authentication failed (check app password / credentials)"
- Add `bridge_app_password` as a hint in config docs

### WS-01-C: Config Template Updates

- Update `src/init.rs` DEFAULT_ACCOUNTS template with Proton Bridge defaults
- Document Bridge-specific settings (app passwords, localhost ports, self-signed certs)

### WS-01-D: Test Bridge Defaults

- Add unit tests for Protonmail provider default resolution
- Test that missing imap_host defaults to 127.0.0.1 for Protonmail
- Test that default ports are correct for Protonmail

## Verification Commands

```
cargo check
cargo test
```

## Gate Plan

| Gate | Trigger | Pass Criteria | Fail Action |
|------|---------|--------------|-------------|
| Build + Tests | After WS-01-A through WS-01-D | `cargo check` clean, `cargo test` all green | Fix compilation or test failures |
| Checkpoint | After all workstreams | Protonmail provider has correct defaults; error messages include Bridge hints | Revise WS-01-B error detection |

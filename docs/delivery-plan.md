# Vivarium Delivery Plan

## 1. Interpreted Problem

### Claimed Problem
Vivarium 0.1 has a working core sync loop but a list of security, correctness, completeness, and polish issues that need fixing before 0.2.

### Inferred Actual Problem
The project has completed its first vertical slice (init → sync → list → show → archive). The remaining work is horizontal: hardening the foundation (security, dedup), closing advertised-but-stubbed features (watch, reply, compose with editor), and polishing the surface (output format, error messages, edge cases).

### Evidence / Rationale
- `cargo check` passes, `cargo test` passes (11 tests)
- Core sync logic in `imap.rs::sync_folder` is functional with parallel workers
- Four stub functions (`watch`, `watch_outbox`, `watch_all`, `watch_account`) are advertised in CLI and docs
- `danger_accept_invalid_certs` is unconditional in both IMAP and SMTP
- Permissions check is a warning, not an error
- UID-only dedup has known race with server-side UID renumbering

### Confidence
High. The code has been read top-to-bottom, builds clean, tests pass. The issues are concrete and fixable.

### Ambiguities / Open Questions
- Should `password_cmd` also support `gpg --decrypt` natively, or is shell-wrapping sufficient?
- Should `list` gain a `--json` flag, or is machine-parsable output an out-of-scope non-goal?
- Is `--body` required on `reply` acceptable, or should it open `$EDITOR`?

---

## 2. Normalized Spec

### 2.1 Project Frame
- **Project:** Vivarium — local-first IMAP-to-Maildir sync tool
- **Language:** Rust, edition 2024
- **Stack:** `async-imap`, `lettre`, `mail-parser`, `tokio`, `clap`, `serde`, `thiserror`, `tracing`
- **Target:** `vivarium 0.2`
- **Repository:** `~/code/ianzepp/vivarium`

### 2.2 Problem Statement
Vivarium 0.1 shipped a working sync loop but left critical security posture weak (silent cert bypass, plaintext passwords, non-blocking permission warnings), correctness fragile (UID-only dedup, fragile path parsing in move), and advertised features stubbed (watch, editor-based compose/reply). The 0.2 release must harden these areas without breaking the 0.1 CLI contract.

### 2.3 Functional Requirements

**FR-1: TLS validation must be configurable**
- Add `reject_invalid_certs: bool` (default: `false` for backward compat) to config
- When `true`, use `native_tls::TlsConnector::builder()` without `danger_accept_invalid_certs(true)`
- Apply to both IMAP and SMTP connections
- Add `--insecure` CLI flag as emergency override (logged at warn level)

**FR-2: Permission check must be a hard gate**
- Change `check_permissions` from warning to error when mode & 0o077 != 0
- Add `--ignore-permissions` flag for scripted/non-interactive use

**FR-3: UID-based dedup → Message-ID-based dedup**
- Primary dedup key: RFC 2822 Message-ID header (stripped of angle brackets, lowercase)
- Secondary: UID+size as fast-path optimization (unchanged behavior when Message-ID is missing)
- Store Message-ID as a sidecar index file (JSON or simple text per message) in `{folder}/.vivarium_index/`
- On sync, check index first; only fetch full body if not found

**FR-4: Fix `move_message` path inference**
- Validate that source subdirectory is one of `new`, `cur`, `tmp`
- Return error with specific message if source is in an unexpected location
- Document invariant: moves preserve subdirectory

**FR-5: Implement watch (IMAP IDLE + outbox filesystem watch)**
- IMAP IDLE: connect to IMAP server, send IDLE command, wait for EXISTS/EXPUNGE notifications, re-IDLE on disconnect
- Filesystem watch: use `notify` crate to watch `{account}/outbox/new/` for new `.eml` files
- On outbox event: call `process_entry` → parse → SMTP send → move to `Sent/` or `outbox/failed/`
- Run as `vivarium watch` (long-running)

**FR-6: Compose/reply with $EDITOR**
- `compose`: parse args, build template headers, open `$EDITOR` (or `$VISUAL`), read saved file, save to `Drafts/` or `outbox/new/`
- `reply`: fetch message, build `Re:` headers with quoted text, open `$EDITOR`, on save send
- Remove `--body` required flag from `reply` (make it optional for non-editor sends)

### 2.4 Non-Functional / Technical Constraints
- No `anyhow` — continue using `thiserror`
- No panics in production code
- Keep files under 400 lines, functions under 60 lines
- Inline tests at bottom of modules
- `--insecure` and `--ignore-permissions` are CLI-only; not in config
- Backward compatible: existing config files must continue to work
- Message-ID index is additive — do not alter Maildir file format

### 2.5 Required Languages
- Rust (primary)
- TOML (config)
- RFC 2822 / RFC 5322 (email format awareness)

---

## 3. Repo-Aware Baseline

### 3.1 Stack Profile
| Layer | Crate | Purpose |
|-------|-------|---------|
| IMAP | `async-imap` v0.10 | IMAP protocol, IDLE support |
| SMTP | `lettre` v0.11 | SMTP sending, TLS, envelope |
| Parsing | `mail-parser` v0.9 | RFC 2822 header/body parsing |
| Async | `tokio` v1 (full) | Runtime, process, sync primitives |
| Config | `serde` + `toml` v0.8 | TOML deserialization |
| CLI | `clap` v4 (derive) | Argument parsing |
| Error | `thiserror` v2 | Typed error enum |
| Watch | `notify` v7 | Filesystem change events |
| Time | `chrono` v0.4 | Date parsing/formatting |

### 3.2 Hard Gates
- No breaking changes to `MailStore` public API
- No breaking changes to CLI subcommand names or argument shapes
- `accounts.toml` format unchanged (password field remains)
- Maildir output format unchanged (`.eml` files, `:2,S` suffixes)
- Message-ID index is opaque — future versions may change format

### 3.3 Architecture Discovery
- `src/lib.rs` declares 11 modules but only re-exports `VivariumError` — likely intended as binary-only crate
- `MailStore` is the single source of truth for local filesystem operations — all sync, list, show, archive route through it
- `imap.rs::connect` and `smtp.rs::send_raw` are the only TLS entry points
- `config.rs` owns all credential resolution — `resolve_password()` is the single path
- `watch.rs` and `outbox.rs` are stubs but `notify` is already in dependencies

### 3.4 Tradeoffs Accepted
- **UID+size as fast path** (existing) → **Message-ID as primary key** (new): slower first sync (need to parse all local files for Message-ID), but correct dedup going forward
- **Warning-only permissions** (existing) → **Error by default** (new): breaks scripted automation that relies on warning-only behavior. Mitigated by `--ignore-permissions`.
- **`--body` required on reply** (existing) → **`$EDITOR` by default** (new): changes CLI contract slightly but improves UX. `--body` becomes optional.

### 3.5 Scope Boundaries
- **In scope for 0.2:** FR-1 through FR-6 above
- **Out of scope:** JMAP support, hook scripts, notmuch integration, TUI, multi-account concurrent sync, credential keyring integration, encrypted password storage

---

## 4. Stage Graph

### Stage 0: Foundation (blocks all parallel work)
| Input | Output | Dependencies | Verification |
|-------|--------|-------------|-------------|
| None | Updated `Cargo.toml` with any new deps | — | `cargo check` passes |

### Stage 1: Security Hardening (FR-1, FR-2)
| Input | Output | Dependencies | Verification |
|-------|--------|-------------|-------------|
| `config.rs`, `imap.rs`, `smtp.rs`, `error.rs` | Configurable TLS rejection, hard permission gate | Stage 0 | `cargo test` passes; manual cert validation test |
| Files | `config.rs` (new fields), `cli.rs` (new flags), `imap.rs`, `smtp.rs`, `error.rs` (new error variant) | — | Build + unit tests for new config fields |

### Stage 2: Correctness Fixes (FR-3, FR-4)
| Input | Output | Dependencies | Verification |
|-------|--------|-------------|-------------|
| `sync.rs`, `store.rs`, `imap.rs` | Message-ID dedup, robust move | — | Sync test with UID renumbering simulation |
| Files | `store.rs` (fixed move), new index module or sidecar in `store.rs` | — | Test: move from wrong subdir fails; test: dedup uses Message-ID |

### Stage 3: Watch Implementation (FR-5)
| Input | Output | Dependencies | Verification |
|-------|--------|-------------|-------------|
| `watch.rs`, `outbox.rs`, `main.rs` | Working IDLE + filesystem watch | Stage 0 | Integration: starts, watches, handles outbox events (mock SMTP) |

### Stage 4: Editor-Based Compose/Reply (FR-6)
| Input | Output | Dependencies | Verification |
|-------|--------|-------------|-------------|
| `message.rs`, `cli.rs`, `main.rs` | `$EDITOR` compose and reply | — | Integration: opens editor, sends on save |

### Stage 5: Housekeeping
| Input | Output | Dependencies | Verification |
|-------|--------|-------------|-------------|
| All changes from 1-4 | Clean code, updated tests, updated docs | Stages 1-4 | `cargo test` all green, no warnings |

---

## 5. Epic Candidates And Scopable Issues

### Epic A: Security Hardening
**Purpose:** Make TLS validation configurable and permissions a hard gate.
**Surface:** `config.rs`, `cli.rs`, `imap.rs`, `smtp.rs`, `error.rs`
**Dependencies:** None
**Parallelization:** Independent of all other epics. Can be implemented in parallel with Epic B.
**Checkpoint Target:** Foundation Merge

#### Issue A-1: Configurable TLS validation
- **Purpose:** Add `reject_invalid_certs` config field + `--insecure` CLI flag
- **Depends On:** None
- **Acceptance Criteria:**
  1. `config.toml` accepts `reject_invalid_certs = true` under `[defaults]` or per-account
  2. When `false` (default), behavior is unchanged (invalid certs accepted)
  3. When `true`, invalid certs cause a connection error
  4. `--insecure` CLI flag overrides config and forces `danger_accept_invalid_certs(true)` with a warn log
  5. Applied to both IMAP `connect()` and SMTP `send_raw()`
  6. New `VivariumError::Tls(String)` variant
  7. Unit tests for config parsing, integration test for cert rejection
- **Out of Scope:** PKI/ca-bundle configuration

#### Issue A-2: Permission check hardening
- **Purpose:** Make accounts.toml permission check a hard error
- **Depends On:** None
- **Acceptance Criteria:**
  1. `AccountsFile::load()` returns `Err(VivariumError::Config("insecure permissions"))` when mode & 0o077 != 0
  2. `--ignore-permissions` CLI flag bypasses the check
  3. Old warning log removed
  4. Test: loading file with wrong permissions fails
  5. Test: loading file with 0600 passes
- **Out of Scope:** Auto-fixing permissions on load

### Epic B: Correctness Fixes
**Purpose:** Fix UID-only dedup and fragile move semantics.
**Surface:** `store.rs`, `imap.rs`, `sync.rs`, `message.rs`
**Dependencies:** None
**Parallelization:** Independent of Epic A and Epic C. Can be done in parallel.
**Checkpoint Target:** Foundation Merge

#### Issue B-1: Message-ID-based dedup
- **Purpose:** Add Message-ID as primary dedup key for sync
- **Depends On:** None
- **Acceptance Criteria:**
  1. Message-ID extracted from local `.eml` files (via `mail-parser`)
  2. Message-ID stored in sidecar index: `{mail_root}/.vivarium_index/{account}/{folder}/{message_id_hash}` containing UID + size
  3. Sync checks index first: if Message-ID present and size matches, skip fetch
  4. If Message-ID absent or size mismatch, fetch body and update index
  5. Graceful degradation: if index is missing/corrupt, fall back to UID+size scan
  6. Index is opaque — format may change without warning
  7. Test: first sync populates index; second sync skips known messages
  8. Test: UID renumbering does not cause re-fetch (Message-ID matches)
- **Out of Scope:** Index migration tool, index compression, distributed index

#### Issue B-2: Robust move_message
- **Purpose:** Validate source subdirectory in `move_message`
- **Depends On:** None
- **Acceptance Criteria:**
  1. `move_message` verifies source is in `new/` or `cur/` (not `tmp/` or other)
  2. Returns descriptive error if source is unexpected
  3. Preserves source subdirectory in destination
  4. Test: move from `new/` succeeds, ends up in `new/` of target
  5. Test: move from `tmp/` fails with error
- **Out of Scope:** Cross-device moves, atomic cross-folder renames

### Epic C: Watch Implementation
**Purpose:** Implement IMAP IDLE + outbox filesystem watching.
**Surface:** `watch.rs`, `outbox.rs`, `main.rs`, `smtp.rs`
**Dependencies:** None (but depends on Stage 0 passing)
**Parallelization:** Independent. Can run alongside A and B.
**Checkpoint Target:** Integration Checkpoint

#### Issue C-1: IMAP IDLE loop
- **Purpose:** Implement persistent IMAP IDLE for push mail delivery
- **Depends On:** None
- **Acceptance Criteria:**
  1. Connects to IMAP, sends IDLE command
  2. Waits for untagged EXISTS/EXPUNGE responses
  3. On notification, performs targeted FETCH for new UIDs
  4. Stores new messages in appropriate folder (`new/` for inbox, `cur/` for others)
  5. Re-enters IDLE after each notification batch
  6. Handles disconnect/reconnect gracefully (with exponential backoff)
  7. Supports `--account` flag
- **Out of Scope:** Concurrent IDLE on multiple folders per account

#### Issue C-2: Outbox filesystem watch
- **Purpose:** Watch outbox/new/ for new `.eml` files and dispatch via SMTP
- **Depends On:** Issue C-1 (shares connection pool, but functionally independent)
- **Acceptance Criteria:**
  1. Uses `notify` crate to watch `{account}/outbox/new/`
  2. On CREATE event: parse `.eml`, extract envelope, send via SMTP
  3. On success: move file to `Sent/`
  4. On failure: move file to `outbox/failed/` with `.error` extension
  5. Handles file-in-use (move to tmp before processing)
  6. Logs all events at debug level
- **Out of Scope:** Batch processing, rate limiting, retry with backoff (basic failure logging only)

### Epic D: Editor-Based Compose/Reply
**Purpose:** Replace `--body`-only compose/reply with `$EDITOR` flow.
**Surface:** `cli.rs`, `message.rs`, `main.rs`
**Dependencies:** None
**Parallelization:** Independent. Can run alongside A, B, C.
**Checkpoint Target:** Integration Checkpoint

#### Issue D-1: Compose with editor
- **Purpose:** `compose` opens `$EDITOR` with template headers
- **Depends On:** None
- **Acceptance Criteria:**
  1. Builds template: `From:`, `To:`, `Subject:`, blank body
  2. Writes temp file with `.eml` extension
  3. Opens `$VISUAL` or `$EDITOR` (or fallback to `vi`)
  4. On exit (success): reads file, validates it has headers, saves to `Drafts/new/`
  5. On exit (non-zero): deletes temp file
  6. Prints path of saved draft
- **Out of Scope:** Signature insertion, template customization, syntax highlighting

#### Issue D-2: Reply with editor
- **Purpose:** `reply` opens `$EDITOR` with quoted text
- **Depends On:** None
- **Acceptance Criteria:**
  1. Fetches original message
  2. Builds `Re:` subject, `In-Reply-To` / `References` headers
  3. Quotes original body with `> ` prefix (CRLF line endings)
  4. Opens `$EDITOR` with complete reply template
  5. On save: sends via SMTP (no draft step)
  6. On cancel: prints "reply cancelled" and exits 0
  7. `--body` flag remains as optional override (backward compat)
- **Out of Scope:** Threaded reply detection, HTML body handling

---

## 6. Checkpoints

### Checkpoint 1: Foundation Merge
**Purpose:** Security and correctness fixes merged, all passing tests.
**Required Inputs:** Issues A-1, A-2, B-1, B-2 complete.
**Merge Criteria:**
- `cargo test` passes (all existing + new)
- `cargo check` passes with no warnings
- New error variant `VivariumError::Tls` present and used
- Permission check is a hard error
- Message-ID index exists and works on first+second sync
**Blocks Until Met:** None (this is the first convergence point)
**Companion Skills:** `$carmack-linus` (interface review), `$bonsai` (code polish), `$gate-check`

### Checkpoint 2: Integration Checkpoint
**Purpose:** Watch and compose/reply integrated with existing sync flow.
**Required Inputs:** All issues from A, B, C, D complete.
**Merge Criteria:**
- `cargo test` passes
- `vivarium watch` starts, connects, handles events (manual smoke test)
- `vivarium compose` opens editor, saves draft
- `vivarium reply` opens editor, sends
- All 0.1 CLI commands still work unchanged
**Blocks Until Met:** Checkpoint 1
**Companion Skills:** `$bonsai`, `$housekeeping`, `$gate-check`

### Checkpoint 3: Release Readiness
**Purpose:** Final polish, docs update, release prep.
**Required Inputs:** Checkpoint 2 passed.
**Merge Criteria:**
- README updated with new features and flags
- VISION.md updated (no changes needed if scope is contained)
- `cargo test` passes, `cargo clippy` clean (or clippy issues acknowledged)
- `cargo build --release` succeeds
- Changelog entry prepared
**Blocks Until Met:** Checkpoint 2
**Companion Skills:** `$carmack-linus`, `$bonsai`, `$housekeeping`, `$gate-check`

---

## 7. Companion Skill Plan

| Checkpoint | Companion Skills | Purpose |
|-----------|-----------------|---------|
| Foundation Merge | `$carmack-linus` | API surface review — ensure new config fields and error variants are clean |
| Foundation Merge | `$bonsai` | Polish the new code for readability and consistency |
| Foundation Merge | `$gate-check` | Go/no-go: security fixes are solid |
| Integration Checkpoint | `$bonsai` | Polish watch + compose/reply code |
| Integration Checkpoint | `$housekeeping` | Run full repo maintenance cycle |
| Integration Checkpoint | `$gate-check` | Go/no-go: all features integrated |
| Release Readiness | `$carmack-linus` | Final architecture/API review |
| Release Readiness | `$bonsai` | Final polish pass |
| Release Readiness | `$housekeeping` | Final maintenance: README, tests, docs |
| Release Readiness | `$gate-check` | Go/no-go: release ready |

---

## 8. Gate Plan

| Gate | Trigger | Pass Criteria | Fail Action |
|------|---------|--------------|-------------|
| Foundation Merge Gate | After A-1, A-2, B-1, B-2 merged | All tests pass, TLS configurable, perm check hard, Message-ID dedup works | Return to Epic A or B for fixes |
| Integration Checkpoint Gate | After C + D merged | Watch starts and responds, compose/reply work, 0.1 CLI unchanged | Return to Epic C or D for fixes |
| Release Readiness Gate | After housekeeping | `cargo clippy` clean, README updated, release builds | Fix remaining issues or defer to 0.3 |

---

## 9. Open Questions

1. **Message-ID index location and format** — Decided: `{mail_root}/.vivarium_index/{account}/{folder}/{id}` containing `{uid}\n{size}\n`. Opaque, may change.
2. **`--insecure` vs `reject_invalid_certs`** — Config field controls default; `--insecure` CLI flag is an emergency override. This gives operators a safe default with an escape hatch.
3. **Should watch also sync flags bidirectionally?** — Out of scope for 0.2. Flag sync was noted as "working" in README status but not in the review issues. Defer to 0.3.
4. **Should `compose` save to `Drafts/` or `outbox/new/`?** — Decided: `Drafts/` (user can review before sending). This is a UX decision: compose = draft, send = dispatch.
5. **Should `reply` be a send or draft?** — Decided: send (immediate). This is the traditional `reply` semantics. If user wants to review first, they can `compose` and paste.

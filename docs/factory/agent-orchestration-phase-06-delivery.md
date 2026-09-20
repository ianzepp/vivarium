# Agent Orchestration Phase 06: Judgment Provider (Shadow Screens)

## Interpreted Phase Problem

The deferred provider phase: `vivi step` is mechanical-only. The write-edge
screen (does a settled task's evidence cover its done_when clauses?) needs
semantic judgment. Operator ruling 2026-09-20 accepted the shape: user-level
`[judgment]` section in `config.toml`, `key_cmd` authentication (no envvar —
ambient-secrets law), off by default.

## Normalized Phase Spec

### Goal

When `[judgment]` is configured, `vivi step --apply` runs provider screens on
the settled item's receipt and appends the answers to a calibration corpus —
shadow only: mechanical completion proceeds regardless of provider answers.
No provider call exists on any read path. Absent/failed/timed-out provider
degrades to today's mechanical behavior with a note.

### Config (user-level `config.toml`)

```toml
[judgment]
provider = "typesafe"                                # only vendor in v1
endpoint = "https://api.typesafe.ai/v1/systemone"    # default; overridable
model    = "jev-latest"                              # default
timeout_ms = 4000                                    # default
key_cmd  = "cat ~/.config/secrets/typesafe-ai.key"   # required when provider set
```

No inline key field exists at all. `key_cmd` executes via `sh -c` exactly
like `password_cmd` (trimmed stdout; failures report stderr only, never the
secret). Absent section = feature entirely off.

### Provider surface

- Trait `JudgmentProvider` with one method; v1 uses Noul questions only.
- `TypesafeProvider`: POST `{state, model, questions}` with Bearer auth to
  the endpoint; answers parsed per question id. HTTP runs on a dedicated
  thread with its own current-thread tokio runtime (reqwest has no blocking
  client; the mailspace path is sync). Errors classify as auth / timeout /
  http / key_cmd and never embed the key.
- Screens on apply: one Noul per `done_when` clause ("does the reported
  validation evidence prove this clause?") plus one completion-honesty Noul.

### Corpus

`.vivi/judgment-corpus.jsonl` (project-local, append-only): timestamp, item,
via, provider, model, per-question answers, mechanical outcome, shadow flag.
This is the calibration corpus a later release needs before any
provider-gated apply behavior.

### Constraints

- Read paths (`step` without `--apply`, board, graph, lists) make zero
  provider calls — structurally: no provider parameter exists on them.
- Provider failures never fail `step --apply`; the manifest notes
  `judgment=skipped(<class>)`.
- Tests use a fake provider; the timeout test targets an instantly-refused
  local endpoint. No real network in tests.
- Standard gates green; hygiene ratchet to measured.

### Out Of Scope

- Provider-gated apply (needs corpus calibration first; separate ruling).
- Choice/Score questions, retries on 429/529, `--config` override for the
  judgment section (default path only in v1), `vivi doctor` check.

## Repo-Aware Baseline

- API contract verified live 2026-09-20 (docs.typesafe.ai/api.md):
  `POST /v1/systemone`, Bearer auth, `{state, model, questions}` →
  `{model, answers, usage}`; Noul answers carry `noul` 0..1.
- `password_cmd` precedent: `src/config/account.rs` `sh -c` execution.
- `Config::load` returns defaults when the file is missing.
- Embedding precedent for endpoint config: `config.toml` `defaults.*`.

## Validation

- Unit: fake-provider screens, corpus append, shadow semantics (completion
  proceeds on "uncovered" answers), skip-on-error, key_cmd resolution and
  failure, timeout downgrade, read-path purity (structural).
- Full gates before commit.

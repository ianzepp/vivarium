# Vivarium Factory Plan: Local Email Archive For Agents

## Factory Intake

### Phase Set Source

This plan replaces the earlier "mail sync client" direction with a narrower
privacy-driven product frame:

> Vivarium is a local-first email archive and retrieval layer that lets private
> local agents search, read, summarize, and process historical email without
> exposing the full corpus to cloud LLMs.

The primary mail source is ProtonMail through Proton Bridge. Vivarium should
treat Proton Bridge as the transport/decryption boundary and should not attempt
to speak ProtonMail private APIs.

### Target Repo

`/Volumes/code/ianzepp/vivarium`

### Delivery Spec Directory

Write one phase delivery spec per phase under:

`docs/factory/`

Recommended names:

- `docs/factory/phase-00-pivot-baseline.md`
- `docs/factory/phase-01-proton-bridge-ingest.md`
- `docs/factory/phase-02-catalog.md`
- `docs/factory/phase-03-extraction.md`
- `docs/factory/phase-04-local-search.md`
- `docs/factory/phase-05-agent-interface.md`
- `docs/factory/phase-06-incremental-ops.md`

### Checkpoint Policy

Each phase must end with:

- a saved phase delivery spec
- implementation complete for that phase only
- focused correctness review
- repo validation commands run, or skipped checks documented
- a phase checkpoint note in the delivery spec
- a local commit

### Commit Policy

Commit after every completed phase. Use small, coherent commits. Do not bundle
multiple phases into one commit. Do not preserve backwards compatibility with
the current CLI unless the phase explicitly says to.

### Agent Policy

Use explorer agents for bounded codebase questions and implementation agents
only for narrow, disjoint write surfaces. Factory remains responsible for final
integration, correctness review, verification, and commits.

### Correctness Policy

Preserve these invariants throughout:

- raw email bytes are preserved unchanged
- derived data is disposable and rebuildable
- every search result can cite an original message file
- full corpus contents never leave the machine by default
- cloud export, if ever added, is explicit, narrow, and user-approved
- send/reply/outbox behavior is out of scope until read-only archive value is
  proven

### Current Baseline Note

The repo currently contains an in-progress refactor and the older
`docs/delivery-plan.md` still describes the pre-pivot 0.2 mail-client plan.
The first factory phase must reconcile the live worktree before making product
changes. Do not discard user work blindly.

## Irreducible Requirements

1. Proton Bridge is the primary integration path.
2. Vivarium is read-only at first.
3. Original `.eml` messages are the source of truth.
4. Metadata, extracted text, keyword indexes, embeddings, and summaries are
   derived artifacts.
5. Local models and local embedding generation are the default and must be
   usable without cloud APIs.
6. Agent-facing output must be scriptable and citeable.
7. Sending email is a later, separate capability and must not shape the first
   architecture.

## Phase Set

### Phase 00: Pivot Baseline

#### Goal

Turn the repo from "mail client/sync utility" into a clean read-only archive
project without losing useful low-level work.

#### Inputs

- current worktree
- `README.md`
- `VISION.md`
- `docs/delivery-plan.md`
- IMAP, Maildir, config, and message parsing modules

#### Expected Outputs

- project docs updated to the new local archive thesis
- old 0.2 delivery plan clearly marked as superseded or archived
- send/reply/compose/outbox/watch surfaces either removed from the main CLI or
  quarantined behind non-default modules
- baseline build and tests passing
- first phase commit

#### Out Of Scope

- SQLite catalog
- embeddings
- new search UI
- any SMTP behavior

#### Checkpoint Target

`vivarium --help` no longer presents a mail-client action surface as the main
product. The repo builds cleanly and has docs that name the Proton Bridge plus
local-agent archive direction.

### Phase 01: Proton Bridge Read-Only Ingest

#### Goal

Make Proton Bridge the first-class read-only ingestion path.

#### Inputs

- Proton Bridge local IMAP host/port behavior
- existing account config
- existing IMAP transport
- existing Maildir store

#### Expected Outputs

- `provider = "protonmail"` / bridge-friendly config documented and tested
- read-only IMAP ingest from Proton Bridge into raw `.eml` Maildir
- Bridge-friendly defaults for localhost, security mode, and credential command
- no SMTP requirement for ingest
- clear errors for Bridge not running, bad Bridge credentials, and TLS/cert
  mismatch

#### Out Of Scope

- Proton private API
- SMTP/send
- embeddings
- global historical import performance tuning

#### Checkpoint Target

A configured Proton Bridge account can ingest messages into preserved Maildir
files using only local IMAP access.

### Phase 02: Durable Catalog

#### Goal

Add a local catalog that gives every message a stable handle and stores
rebuildable metadata without replacing raw `.eml` files as the source of truth.

#### Inputs

- raw Maildir files
- existing message parsing helpers
- existing Message-ID dedupe logic

#### Expected Outputs

- local SQLite catalog
- stable message handles
- raw file path and content fingerprint per message
- account, folder, Maildir subdir, date, from/to/cc/bcc, subject, RFC Message-ID
- duplicate tracking across folders
- catalog rebuild command
- tests for handle stability and rebuild behavior

#### Out Of Scope

- embeddings
- summaries
- attachment OCR
- cross-device sync

#### Checkpoint Target

Deleting the catalog and rebuilding it from raw mail produces the same stable
handles for unchanged messages.

### Phase 03: Text Extraction And Attachment Inventory

#### Goal

Produce normalized local text that agents can read while preserving enough
provenance to return to the original message.

#### Inputs

- SQLite catalog
- raw `.eml` files
- MIME parsing

#### Expected Outputs

- extracted plain text body
- HTML-to-text fallback for HTML-only mail
- attachment inventory with filenames, MIME types, sizes, content IDs, and local
  extraction status
- extraction version metadata
- invalid or lossy parse errors recorded without blocking the whole corpus
- command to rebuild extraction artifacts

#### Out Of Scope

- OCR
- attachment embedding
- arbitrary document conversion
- cloud parsing services

#### Checkpoint Target

An agent can request a message handle and receive normalized text plus source
metadata, while the original raw `.eml` remains the citation target.

### Phase 04: Local Search And Embeddings

#### Goal

Make historical email discoverable through local keyword search and local
semantic search.

#### Inputs

- catalog metadata
- extracted text
- local embedding model/runtime

#### Expected Outputs

- keyword search over subject, sender, recipients, and extracted body
- local embedding generation only
- embedding model identity and dimensions stored with vectors
- rebuildable embedding index
- stale-index detection when extraction version or embedding model changes
- `vivarium search` with text and JSON output
- tests for search result handles and citation metadata

#### Out Of Scope

- cloud embeddings
- cloud reranking
- model download management
- multi-user service mode

#### Checkpoint Target

Search returns stable handles with enough metadata for an agent to fetch,
inspect, and cite the underlying message locally.

### Phase 05: Agent Interface

#### Goal

Expose the archive as a predictable local tool surface for agents and local
LLMs.

#### Inputs

- catalog
- extraction artifacts
- search index

#### Expected Outputs

- `vivarium show <handle> --json`
- `vivarium thread <handle> --json`
- `vivarium search <query> --json`
- `vivarium export <handle>` for local-only raw/text export
- explicit citation fields in every JSON response
- bounded result sizes and pagination
- clear errors for missing, stale, or corrupted artifacts

#### Out Of Scope

- MCP server
- daemon/API server
- cloud model integration
- sending mail

#### Checkpoint Target

A local agent can search, retrieve, thread, and cite email using CLI JSON
without needing direct unrestricted filesystem traversal.

### Phase 06: Incremental Operations

#### Goal

Make the archive maintainable as new mail arrives without turning Vivarium back
into a general mail client.

#### Inputs

- read-only Proton Bridge ingest
- catalog
- extraction
- search/embedding indexes

#### Expected Outputs

- incremental ingest command
- incremental catalog update
- incremental extraction update
- incremental search/embedding update
- optional local-only watch mode for Bridge/IMAP polling or filesystem changes
- operational docs for scheduled local runs
- corruption recovery and rebuild documentation

#### Out Of Scope

- SMTP/send
- cloud sync service
- always-on multi-user daemon
- mobile support

#### Checkpoint Target

A scheduled local run can pull new Proton Bridge mail and update catalog,
extraction, keyword search, and embeddings without reprocessing the full archive.

## Deferred Capabilities

### Sending Mail

Sending is deferred until read-only archive value is proven. If restored, it
should be a separate explicit capability with approval boundaries. Agents being
able to read private email is one risk class; agents being able to send email is
another. The follow-on factory plan for upstream mailbox writes and outbound
sending lives in `docs/email-write-send-factory-plan.md`.

### Cloud LLM Assistance

Cloud models must not receive the full corpus or unrestricted search access.
Any cloud boundary should be a later phase with explicit commands, redaction,
small selected excerpts, and audit-friendly logs.

### MCP Or Local Server

An MCP/server surface may be useful later, but the first agent interface should
be the CLI JSON contract. A server would add lifecycle, auth, and concurrency
surface area before the core archive model is proven.

## Factory Stop Conditions

Pause the factory run if any of these become true:

- Proton Bridge behavior cannot be validated locally
- stable handle design would require changing raw message storage
- local embedding runtime choice blocks phase progress
- catalog schema cannot preserve source provenance
- a phase requires sending or cloud access to pass its checkpoint
- the current dirty worktree contains user changes that conflict with the pivot

## Suggested First Command For Execution

When ready to execute, start with:

```sh
factory phase 00 from docs/local-email-archive-factory-plan.md
```

The first phase should save its own delivery spec to
`docs/factory/phase-00-pivot-baseline.md` before editing implementation files.

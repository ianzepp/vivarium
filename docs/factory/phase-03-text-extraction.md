# Phase 03: Text Extraction And Attachment Inventory

## Interpreted Problem

Phase 02 established the durable catalog with stable handles. Phase 03 adds
text extraction and attachment inventory so agents can read message content
without opening raw .eml files directly.

## Phase Spec

### Goal

Produce normalized local text that agents can read while preserving enough
provenance to return to the original message.

### Expected Outputs

1. Extracted plain text body from .eml files
2. HTML-to-text fallback for HTML-only messages
3. Attachment inventory with filenames, MIME types, sizes, content IDs, local extraction status
4. Extraction version metadata (to track stale extraction)
5. Invalid or lossy parse errors recorded without blocking
6. Command to rebuild extraction artifacts (`vivarium extract rebuild`)

### Out Of Scope

- OCR
- Attachment embedding
- Arbitrary document conversion
- Cloud parsing services

### Checkpoint Target

An agent can request a message handle and receive normalized text plus source
metadata, while the original raw .eml remains the citation target.

## Workstreams

### WS-03-A: Text Extraction

- Add `extract_text(data: &[u8]) -> Result<ExtractedText, ...>` function
- Use `mail-parser` to extract body_text for plain-text messages
- For HTML-only messages, use `body_html` and strip HTML tags to text
- Record `extracted_format` field: "plain" | "html-stripped" | "none"
- Record `extract_quality`: "full" | "partial" | "none"

### WS-03-B: Attachment Inventory

- Parse MIME parts from the .eml data
- For each attachment: filename, MIME type, size, content_id, extraction_status
- attachment extraction_status: "pending" | "extracted" | "skipped"
- Store attachment info in catalog entries or sidecar files

### WS-03-C: Extraction Version Tracking

- Add `extraction_version: String` to catalog entries
- Default version: "1" — increment when extraction logic changes
- Track which entries have been extracted vs need extraction

### WS-03-D: Rebuild Command

- `vivarium extract rebuild` — re-extract all messages for an account
- Prints summary: N scanned, N extracted, N skipped, N errors
- Idempotent: running again produces same results

## Verification Commands

```
cargo check
cargo test
```

## Gate Plan

| Gate | Trigger | Pass Criteria | Fail Action |
|------|---------|--------------|-------------|
| Build + Tests | After WS-03-A through WS-03-D | `cargo check` clean, `cargo test` all green | Fix compilation or test failures |

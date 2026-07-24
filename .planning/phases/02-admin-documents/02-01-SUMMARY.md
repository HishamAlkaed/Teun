---
phase: 02-admin-documents
plan: 01
subsystem: admin-documents-backend
tags: [multipart-upload, axum, pdf, bytea, document-management, ingest]
requires:
  - "01-03: ingest_document (upsert + delete_chunks + status tracking; re-run safe by construction)"
  - "01-01: RagStore + EmbedConfig::from_env (Azure text-embedding-3-large)"
provides:
  - "POST /api/teun/admin/documents: multipart PDF upload -> spawned ingestion, 202 (ADM-01)"
  - "GET /api/teun/admin/documents: DB-backed document list with status/chunk_count/size (ADM-02)"
  - "DELETE /api/teun/admin/documents/{id}: row + cascade chunk removal (ADM-03)"
  - "GET /api/teun/documents/{filename}/pdf: inline original PDF bytes from bytea (ADM-05)"
  - "rag::store: list_documents / delete_document / get_pdf_bytes"
affects: [02-02-frontend-document-manager]
tech-stack:
  added: ["axum multipart feature (teun package only; pulls multer via workspace axum 0.8)"]
  patterns:
    - "per-route DefaultBodyLimit::max(60MB) on the upload method router overrides the outer global 64KB layer (inner layer wins — axum extension overwrite)"
    - "upload handler pre-creates the pending documents row, then tokio::spawn's ingest_document; spawned task marks set_document_error on failure so rows never stick in 'pending'"
key-files:
  created:
    - apps/teun/service/src/routes/admin_documents.rs
  modified:
    - apps/teun/service/src/rag/store.rs
    - apps/teun/service/src/routes/documents.rs
    - apps/teun/service/src/routes/mod.rs
    - apps/teun/service/src/main.rs
    - apps/teun/service/Cargo.toml
    - Cargo.lock
decisions:
  - "Whole-request 400 on ANY invalid file: all multipart files are read + validated BEFORE any row creation or ingestion spawn — no partial accepts"
  - "202 body is exactly [{filename, status:\"pending\"}] per plan spec (no id) — the frontend gets ids from GET list"
  - "Pending row created synchronously (insert_document with empty extracted_text) so GET list reflects the upload immediately; ingest_document's filename upsert reuses the same id and overwrites"
  - "Content-Disposition filename sanitized to visible ASCII minus quote/backslash (header-injection safety); path separators rejected with 400 before DB lookup"
metrics:
  duration: "~15 min"
  completed: "2026-07-24"
---

# Phase 2 Plan 01: Backend Document Management Summary

**One-liner:** Multipart PDF upload (MIME + `%PDF-` magic + 50MB validation, per-route 60MB body limit) feeding the existing re-run-safe ingest pipeline via tokio::spawn, plus DB-backed list, cascade delete, and inline bytea PDF serving — all live-tested against the seeded pgvector DB.

## Endpoint Shapes (contract for Plan 02-02)

### POST /api/teun/admin/documents
- Request: `multipart/form-data`, one or more fields named `file` (filename + content type per part). Per-route body limit 60MB; per-file cap 50MB.
- Validation (any failure rejects the WHOLE request, nothing is ingested): content type must contain "pdf" (case-insensitive), bytes must start with `%PDF-`, size ≤ 50MB, filename non-empty and free of `/` `\` `..`.
- `202 Accepted`: `[{"filename": "x.pdf", "status": "pending"}, ...]`
- `400`: `{"error": "<Dutch message>"}` (invalid file, unreadable multipart, or no `file` fields)
- `503`: `{"error": "Embeddings-provider is niet geconfigureerd"}` (EmbedConfig::from_env failed)
- `500`: `{"error": "Kon document niet opslaan: <filename>"}`
- Side effect: a pending `documents` row exists by the time 202 returns; ingestion (extract→chunk→embed→store, status pending→indexing→indexed/error) runs in a spawned task. Re-uploading an existing filename replaces it in place (same id, chunks refreshed — no duplicates).

### GET /api/teun/admin/documents
- `200`: JSON array, newest first (`created_at DESC`):
```json
[{
  "id": "uuid-v4-string",
  "filename": "MUNT Hypotheekgids 2026-2.pdf",
  "status": "pending|indexing|indexed|error",
  "chunk_count": 79,
  "page_count": 35,
  "size_bytes": 1234567,
  "error_message": null,
  "created_at": "2026-07-24T09:31:00+00:00",
  "indexed_at": "2026-07-24T09:32:10+00:00"
}]
```
- `error_message` and `indexed_at` are `null` unless set. Timestamps are RFC 3339.
- `500`: `{"error": "Kon documenten niet ophalen"}`

### DELETE /api/teun/admin/documents/{id}
- `204 No Content` on success (chunks removed via `ON DELETE CASCADE`, bytes + metadata row gone).
- `404`: `{"error": "Document niet gevonden"}`
- `500`: `{"error": "Kon document niet verwijderen"}`

### GET /api/teun/documents/{filename}/pdf
- `200`: raw PDF bytes, `content-type: application/pdf`, `content-disposition: inline; filename="<sanitized>"`. Filename is the URL-encoded path segment (spaces etc. decoded by axum) and matches the citation `document` field.
- `400`: `{"error": "Ongeldige bestandsnaam"}` (path separators / `..`)
- `404`: `{"error": "Document niet gevonden"}`

No in-service auth on any of these, matching `routes/admin.rs` — portal auth fronts /admin in deployment (plan interfaces; not a gap).

## What Was Built

### Task 1 — Store additions (commit ce0ff0d)
- `rag/store.rs`: `DocumentListItem` struct + `list_documents()` (`octet_length(original_bytes)::BIGINT AS size_bytes`, never loads payloads, `ORDER BY created_at DESC`), `delete_document(id) -> bool` (`rows_affected > 0`), `get_pdf_bytes(filename) -> Option<Vec<u8>>`. Runtime `sqlx::query().bind()` style, no macros.
- 3 new `#[ignore]`-gated integration tests: list metadata + size + deterministic newest-first (older row backdated 1 day to avoid same-microsecond flakiness), delete-cascades-chunks + false-on-unknown, pdf-bytes round trip + None-on-unknown.

### Task 2 — Routes + wiring (commit 63bf5be)
- New `routes/admin_documents.rs` (~330 lines): upload/list/delete handlers as specced above; `validate_upload` factored out with 9 offline unit tests (MIME case/charset, magic bytes, exact-cap boundary, oversize, empty/traversal filenames).
- Upload: reads + validates ALL files first, then per file creates the pending row and `tokio::spawn`s `ingest_document(state.http_client, RagStore, EmbedConfig::from_env, filename, bytes, None)`. Spawned task calls `set_document_error` on failure (covers the extraction-failure window where `ingest_document` propagates before it can record — the pre-created row would otherwise stick in 'pending'). Logs filename + size only, never bytes.
- `routes/documents.rs`: `GET /api/teun/documents/{filename}/pdf` + `sanitize_header_filename` (visible-ASCII filter, strips `"` and `\`) with 2 unit tests.
- `main.rs`: `.merge(routes::admin_documents::router())`; `routes/mod.rs` registers the module; `Cargo.toml`: `axum = { workspace = true, features = ["multipart"] }` (scoped to the teun package).
- Per-route `.route_layer(DefaultBodyLimit::max(60 * 1024 * 1024))` on the `/api/teun/admin/documents` method router — the inner layer overwrites the global 64KB extension from main.rs.

## Verification Results

All cargo runs in the `teun-rust-build` helper image (no host toolchain), target/registry in named volumes; live tests against the seeded DB via `host.docker.internal:15432` (seeded 4 documents untouched — tests use unique filenames and clean up).

- Task 1 compile gate: `cargo test --release --package teun rag::store` → `test result: ok. 0 passed; 0 failed; 8 ignored`.
- Task 1 LIVE (`-- --ignored --skip live_semantic` with DATABASE_URL): `test result: ok. 7 passed; 0 failed` (3 new + 4 pre-existing store tests, no regressions).
- Task 2: `cargo build --release --package teun` → `Finished 'release' profile [optimized] target(s) in 25.20s`; full suite `cargo test --release --package teun` → **`test result: ok. 91 passed; 0 failed; 12 ignored`** (was 80/9 after 01-04: +11 unit tests, +3 ignored DB tests).
- Warnings unchanged in character: only the pre-existing dead_code set (ingest bin's `#[path]`-mounted rag copy doesn't use admin APIs; `Document`/`get_document`/`count_chunks` now test-only in the main bin).
- Live HTTP smoke (upload → poll indexed → cite → serve inline → delete) is the ORCHESTRATOR's step per the plan's verification block; `teun-svc` on :13000 was not touched.

## Deviations from Plan

### Auto-fixed / adjusted

**1. [Rule 2 - missing critical functionality] Pending row pre-created + spawned-task error marking**
- The plan's interface requires 202 "immediately after validation + document row creation", but `ingest_document` only creates the row AFTER PDF extraction (seconds). The handler pre-creates the row via the same filename-upsert (`insert_document` with empty text; ingest re-upserts the same id). Consequence handled: an extraction failure in the spawned task propagates before `ingest_document` can record an error (documented 01-03 behavior), which would leave the pre-created row 'pending' forever — the spawn wrapper calls `set_document_error` on any failure.

**2. [Rule 2 - security] Upload filename validation beyond the plan's list**
- Rejects empty filenames and path separators/`..` at upload time (the filename becomes the serving path segment and citation key). Mirrors the existing `documents.rs` traversal checks; Content-Disposition value additionally sanitized to prevent header injection via crafted filenames.

**3. [Note] TDD RED/GREEN collapsed into one commit (Task 1, tdd="true")**
- Same rationale/precedent as all Phase-1 plans: same-module Rust tests can't compile before the code under test; DB tests are `#[ignore]`-gated. All behavior-block bullets are covered AND were run green against the live DB, not just compiled.

**4. [Note] Task 2 commit amended to include Cargo.lock**
- The multipart feature pulls `multer` into Cargo.lock; the lockfile change was amended into the Task 2 commit (unpushed) rather than left dangling.

**5. [Note] `gsd-sdk query` unavailable** — on-PATH gsd-sdk is `@gsd-build/sdk` v0.1.0 without the `query` glue (known since 01-01); STATE/ROADMAP/REQUIREMENTS updates done manually.

## Known Stubs

None. The pre-created pending row briefly has `extracted_text=""` until the spawned ingest re-upserts the real canonical body — a transient state by design (status column communicates it), not a stub.

## Threat Flags

None beyond the plan's interfaces. New network surface (upload/list/delete/pdf-serve) is exactly the plan's scope; no in-service auth is the plan-mandated deployment model (portal fronts /admin). Implemented mitigations: MIME + magic-byte + size validation before any persistence, whole-request rejection on invalid files, path-separator rejection on both upload filenames and the /pdf path segment, header-injection-safe Content-Disposition, filename+size-only logging (never bytes), 50MB/file + 60MB/request caps.

## Commits

| Task | Commit | Message |
|------|--------|---------|
| 1 | ce0ff0d | feat(02-01): add list/delete/pdf-bytes document store methods |
| 2 | 63bf5be | feat(02-01): add admin document upload/list/delete + inline PDF serving |

## Self-Check: PASSED

- Files exist: `routes/admin_documents.rs` (created, ~330 lines ≥ min 80), store/documents/mod/main/Cargo.toml modified.
- Key-link patterns present: `ingest_document` spawned in admin_documents.rs with AppState http_client + EmbedConfig; `admin_documents` merged in main.rs; `DefaultBodyLimit::max(60 * 1024 * 1024)` route_layer on the upload route.
- Commits ce0ff0d, 63bf5be present on `gsd/rag-rebuild`.
- Must-have truths: upload validates + kicks off ingestion (validated offline; row-creation + upsert semantics live-tested at store level); list from DB with all fields (live test); delete cascades (live test); /pdf streams bytea inline (live store test + handler unit tests); re-upload replaces via ingest_document's upsert+delete_chunks by construction (01-03, live-verified then at 210-chunks-stable). Full live HTTP pass = orchestrator smoke.

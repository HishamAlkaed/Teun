---
phase: 03-citation-integrity
plan: 01
subsystem: citation-integrity
tags: [citations, document-viewer, db-first, regression-gate, rag]
requires:
  - "02-01: RagStore::list_documents / get_extracted_text / get_pdf_bytes (DB store methods)"
  - "01-03: documents.extracted_text as the canonical line-numbered body chunk line numbers refer to"
provides:
  - "GET /api/teun/documents: DB-backed list of indexed documents (disk .md merge fallback) — CIT-02"
  - "GET /api/teun/documents/{filename}: DB extracted_text served in the unchanged JSON shape + additive content_type — CIT-02"
  - "DocumentViewer defaults to line view for pdf_text content — CIT-02"
  - "CIT-03 regression evidence: zero diff on agent/types.rs, agent/stream.rs, routes/chat.rs, judge/* this phase"
affects: []
tech-stack:
  added: []
  patterns:
    - "DB-first with disk fallback: get_extracted_text (empty text = pending row, treated as absent) -> std::fs read -> 404; list endpoint 500s only if BOTH sources fail"
    - "additive-only response evolution: content_type (\"pdf_text\"|\"markdown\") appended to the existing document JSON; all pre-existing fields byte-identical"
key-files:
  created:
    - .dockerignore
  modified:
    - apps/teun/service/src/routes/documents.rs
    - apps/teun/web/src/chat/components/DocumentViewer.tsx
decisions:
  - "List endpoint includes only status='indexed' DB rows (pending/indexing/error rows have no citable text); legacy disk .md files not present in the DB are merged in for transition safety"
  - "line_count in the list is computed from extracted_text (per-doc get_extracted_text) so it matches the content endpoint's total_lines — corpus is small (4 docs), N+1 acceptable"
  - "Empty extracted_text (pre-created pending row, 02-01 design) is treated as NOT FOUND in the DB so the content endpoint falls through to disk/404 instead of rendering an empty document"
  - "DB errors on either endpoint degrade to the disk path (log + fallback) rather than 500 — DB-first, not DB-only"
  - "Viewer default-view switch happens in the fetch resolution (once per document load), leaving the manual Opgemaakt/Regelnummers toggle fully functional afterwards"
metrics:
  duration: "~15 min"
  completed: "2026-07-24"
---

# Phase 3 Plan 01: DB-backed Document Viewer Endpoints + Regression Gate Summary

**One-liner:** Document list + content endpoints now serve the DB canonical `extracted_text` (the text citation line numbers actually refer to) with disk .md fallback and an additive `content_type` field, DocumentViewer defaults PDF-extracted text to the line view, and git evidence confirms zero changes to the SSE/answer contract files this phase.

## What Was Built

### Task 1 — DB-backed document endpoints, shape-preserving (commit dab36d6)

`apps/teun/service/src/routes/documents.rs`:

- **`GET /api/teun/documents`**: DB-first via `RagStore::list_documents()`, filtered to `status == "indexed"` (only those have citable text). Each item keeps the exact existing shape `{filename, size_bytes, line_count}`; `line_count` is counted from `get_extracted_text` so it matches the content endpoint's `total_lines`. Legacy `.md` files in the resources dir that are NOT in the DB are merged in (transition safety), result still sorted by filename. Returns 500 only if BOTH the DB query and the dir read fail; a single-source failure logs and degrades.
- **`GET /api/teun/documents/{filename}`**: resolves `RagStore::get_extracted_text(filename)` first — this is the canonical body chunk `line_start`/`line_end` refer to, so citation highlights land on the right lines. Empty `extracted_text` (the 02-01 pre-created pending row) is treated as absent; on `None`/error the previous disk read remains as fallback, then 404. Path-traversal guard, `line_range` parsing, bounds clamping, and the out-of-bounds highlight drop are all unchanged. Response JSON: identical fields plus ONE additive `content_type: "pdf_text" | "markdown"`.

### Task 2 — DocumentViewer default view for PDF text (commit ebb262a)

`apps/teun/web/src/chat/components/DocumentViewer.tsx`:

- `DocumentData` gains optional `content_type?: string` (additive, tolerant of absence).
- On fetch resolution, `content_type === "pdf_text"` switches the default view mode to `"lines"` (extracted PDF text is not markdown; the rendered view would mangle it). The Opgemaakt/Regelnummers toggle still works after load. No other behavior changes.
- Also in this commit: root `.dockerignore` (see Deviations #1).

### Task 3 — CIT-03 regression evidence (verification only, no code)

Phase start = `b4a86f4` (last Phase 2 commit). Contract-file diff for the phase:

```
$ git diff --stat b4a86f4..HEAD -- apps/teun/service/src/agent/types.rs \
    apps/teun/service/src/agent/stream.rs apps/teun/service/src/routes/chat.rs \
    apps/teun/service/src/judge/
(empty — ZERO changes; all four paths confirmed to exist)

$ git diff --stat b4a86f4..HEAD          # full phase diff
 .dockerignore                                      |   8 ++
 apps/teun/service/src/routes/documents.rs          | 147 +++++++++++++------
 .../web/src/chat/components/DocumentViewer.tsx     |  11 +-
 3 files changed, 123 insertions(+), 43 deletions(-)
```

`agent/types.rs`, `agent/stream.rs`, `routes/chat.rs`, and `judge/*` (llm.rs, mod.rs, types.rs, verifier.rs) are untouched this phase. SSE `ChatEvent` and `MortgageAnswer`/`SourceReference` shapes cannot have changed. (Orchestrator re-runs the live E2E SSE-shape check.)

### CIT-01 — verified, not re-implemented

Delivered by commit `06fe9d6` (Phase 1 pull-forward): `judge/verifier.rs` loads `store.get_extracted_text(&source.document)` and matches cited quotes/line ranges against it, with a disk fallback. Confirmed present by code inspection this session; covered by the existing suite (91 green below). No code change.

## Verification Results

All cargo runs in the `teun-rust-build` helper image (no host toolchain), registry/target in the named volumes; web build in the Dockerfile's own `frontend` stage (`node:22-bookworm-slim`), since the Windows host blocks local esbuild execution (known since 02-02).

- Task 1: `cargo build --release --package teun` → `Finished 'release' profile [optimized] target(s) in 26.05s` (same 4 pre-existing dead_code warnings, nothing new). Full suite → `test result: ok. 91 passed; 0 failed; 12 ignored` — identical to the 02-01/02-02 baseline, no regressions.
- Task 2: `docker build -f apps/teun/Dockerfile --target frontend .` → `✓ built in 2.02s`, `dist/assets/index-C-Q5dPSb.js 464.61 kB │ gzip: 139.36 kB` (throwaway image removed after verification).
- Task 3: full suite re-run → `test result: ok. 91 passed; 0 failed; 12 ignored; finished in 0.21s`; contract diff empty (quoted above).
- Live E2E (open a citation from a real answer against the running `teun-svc`) is the orchestrator's step per the plan; the running container (old binary) was not touched.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Docker build context broke on Linux symlinks in host node_modules — added root `.dockerignore`**
- **Found during:** Task 2 verification (`docker build --target frontend`).
- **Issue:** `ERROR: invalid file request apps/teun/web/node_modules/.bin/acorn` during context transfer. 02-02's in-container `npm ci` (bind-mounted) wrote Linux symlinks into the host `node_modules` (documented in 02-02's summary); Docker on Windows can't read them as context files. No `.dockerignore` existed.
- **Fix:** Created root `.dockerignore` with `**/node_modules` and `.git`. Both Dockerfile stages install their own dependencies in-image (`npm ci` / cargo), so excluding host trees makes the frontend stage MORE correct (previously `COPY apps/teun/web/ ./` could overlay host node_modules onto the clean `npm ci` output) and shrinks the context from ~165MB.
- **Files modified:** `.dockerignore` (created).
- **Commit:** ebb262a (with Task 2).

No other deviations — Tasks 1 and 3 executed exactly as written.

## Known Stubs

None. Both endpoints are fully wired to the live store methods; the disk fallback is deliberate transition behavior mandated by the plan, not a stub.

## Threat Flags

None. No new network surface — both endpoints existed; only their content source changed (disk → DB-first). The path-traversal guard is retained (and now defense-in-depth, since the DB lookup is an equality match). Error bodies remain the generic Dutch messages; extracted text is corpus content already served before.

## Commits

| Task | Commit | Message |
|------|--------|---------|
| 1 | dab36d6 | feat(03-01): serve document list + content from DB extracted_text (disk fallback) |
| 2 | ebb262a | feat(03-01): default DocumentViewer to line view for PDF-extracted text |
| 3 | — | verification only (evidence above) |

## Self-Check: PASSED

- Files exist: `.dockerignore` (created); `documents.rs`, `DocumentViewer.tsx` (modified) — per `git diff --stat b4a86f4..HEAD`.
- Commits dab36d6, ebb262a present on `gsd/rag-rebuild`.
- Key-link pattern present: `get_extracted_text` called in `routes/documents.rs` (DB-first lookup, disk fallback) and `list_documents` for the list endpoint.
- Must-have truths: list is DB-backed for indexed docs (+.md merge); content endpoint serves DB extracted_text in the unchanged JSON shape with additive `content_type` only; viewer defaults pdf_text to the lines view with highlight logic untouched; contract files have a provably empty phase diff.

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-07-17)

**Core value:** Grounded, source-cited mortgage-acceptance answers at a fraction of the current per-request cost — by retrieving only relevant policy passages, with citations still verifiable against the source documents.
**Current focus:** Phase 2 — Admin Document Management (Web)

## Current Position

Phase: 2 of 3 (Admin Document Management — Web)
Plan: 2 of 2 executed (02-01 backend document management DONE; 02-02 frontend document-manager tab DONE)
Status: 02-02 complete — Documentatie admin tab replaced with a live DocumentManager (drag-drop multi-upload, status/size/pages/chunks table, delete w/ window.confirm, self-scheduling 3s poll while pending/indexing, PDF link opens /api/teun/documents/{filename}/pdf); existing static help content preserved in a closed-by-default collapsible. Build/tests verified via the Dockerfile's `frontend` stage (node:22-bookworm-slim) — the host's Windows Group Policy blocks local esbuild.exe execution (code 1260, unrelated to code correctness); `tsc -b` passes directly on host. Phase 2 requirements ADM-01..05 all Complete. Orchestrator live HTTP smoke (upload→indexed→cite→serve→delete) from 02-01 still pending, plus live verification of this tab against the running teun-svc container.
Phase 1: COMPLETE (E2E checkpoint passed 2026-07-24; verifier rewire pulled forward in 06fe9d6).
Remaining before deploy (user-side, see .planning/DEPLOYMENT-CHECKLIST.md): pgvector on target Postgres, env vars in NextEpoch App settings, seed ingest run against production DB (cargo run --bin ingest).
Last activity: 2026-07-24 — Executed Plan 02-02 (frontend document-manager tab). Commit a154d00.

Progress: [██████████] Phase 2: 2/2 plans (Phase 1: 4/4 done)

## Resume Next Week (paused 2026-07-17)

**Branch:** `gsd/rag-rebuild` (all planning committed; Teun default branch = `main`).

**Next action:** execute Phase 1 Plan `01-04` (retriever + run_rag swap). Plans 01-01/01-02/01-03 are complete; the seed corpus is live in pgvector.

**Do BEFORE executing (prereqs, user-side):**
1. pgvector `vector` extension available on target Postgres (Plan 01-01 has a blocking checkpoint that checks `pg_available_extensions`).
2. Azure OpenAI embeddings vars set as NextEpoch App settings: `AZURE_OPENAI_ENDPOINT`, `AZURE_OPENAI_API_KEY`, `AZURE_OPENAI_DEPLOYMENT=text-embedding-3-large`, `AZURE_OPENAI_API_VERSION=2024-02-01`. (Provided; key never committed.)
3. PDFium native lib bundling in Docker — LOW-confidence spike; Plan 01-02 validates `.so` load + extraction on real PDFs before 01-03 depends on it. `.md` files are fallback if extraction quality poor.

**How to execute (toolchain note):** on-PATH `gsd-sdk` is `@gsd-build/sdk` v0.1.0 (run/auto/init) and LACKS the `gsd-sdk query` glue the `~/.claude/get-shit-done/` skill workflows call — so `/gsd:*` commands won't run as-scripted. Drive manually: spawn the installed `gsd-executor` agent per plan (worked fine for gsd-roadmapper/gsd-phase-researcher/gsd-planner/gsd-plan-checker), do git ops by hand.

**Plans:** 01-01 data layer+embeddings (RET-01,02,ING-03) · 01-02 PDFium extract (ING-01) · 01-03 chunker+ingest+seed (RET-03,ING-02,04) · 01-04 retriever+run_rag swap, delete claude.rs (RET-04,05,06).

**Seed corpus:** ingest 4 PDFs from resources/acceptatie/; skip near-duplicate `handboek_accept_versie_2026_4.pdf` (use `_definitief`).

## Performance Metrics

**Velocity:**
- Total plans completed: 6 (01-01, 01-02, 01-03, 01-04, 02-01, 02-02)
- Average duration: ~28 min
- Total execution time: ~2h20m

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 01 | 4 (01-01: 3 tasks, 9 files · 01-02: 2 tasks, 5 files · 01-03: 3 tasks, 7 files · 01-04: 4 tasks, 9 files) | ~125 min | ~31 min |
| 02 | 2 (02-01: 2 tasks, 7 files · 02-02: 1 task, 3 files) | ~35 min | ~18 min |

**Recent Trend:**
- Last 5 plans: 01-03, 01-04, 02-01, 02-02 — all green, no checkpoints hit
- Trend: stable; Phase 2 done in 2 plans as planned

*Updated after each plan completion*

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- Vector store = pgvector (reuse existing sqlx + Postgres)
- Embeddings = Azure OpenAI `text-embedding-3-large`, native 3072-dim (deployment `text-embedding-3-large`, api-version 2024-02-01, endpoint dmfco-ai-tools-resource). pgvector column `VECTOR(3072)`. Plain OpenAI = fallback only. App deployed on NextEpoch; Azure is outbound embeddings API only. Key lives in NextEpoch App settings, never committed.
- PDF extraction = pdfium-render (PDFium native lib must be bundled in Docker)
- PDFium pinned to bblanchon chromium/7881 (= pdfium-render 0.9.3 pdfium_latest), sha256-checked in docker build; .so loads + extracts all 4 seed PDFs in the runtime image (spike PASSED; executor verdict: quality GOOD, no .md fallback)
- PDFium is NOT thread-safe — all native access serialized via process-wide mutex in rag::extract; call extract_from_bytes via spawn_blocking from async contexts
- PDFium binds ONCE per process (pdfium-render global OnceLock): extract handles PdfiumLibraryBindingsAlreadyInitialized by reusing existing bindings via Pdfium::default() (fix a925510)
- Chunking = 700 cl100k tokens / 120 overlap via text-splitter chunk_indices; line_start/line_end from byte offsets, page from Canonical::page_of_line
- Idempotent ingest lives IN ingest_document (filename upsert + delete_chunks before re-chunk) — any caller is re-run safe; verified live (210 chunks stable across re-runs)
- Seed corpus LIVE in teun-pg-dev: 4 docs indexed (handboek_definitief 58, Beheergids 32, Hypotheekgids 79, Voorleggids 41 = 210 chunks); near-duplicate handboek skipped
- Citations = line-based over extracted text (store line-numbered canonical body; tag chunks with page)
- PDF bytes stored in Postgres `bytea`; `mode` field (tools/inline) collapses to one RAG answer path — DONE in 01-04 (both modes → run_rag; claude.rs + inline.rs deleted; image de-Node'd)
- Generation LLM switchable via LLM_PROVIDER=anthropic (default, RAG_MODEL→INLINE_MODEL→CLAUDE_MODEL) | azure-openai (AZURE_OPENAI_CHAT_DEPLOYMENT=gpt-5.6-luna; body uses max_completion_tokens — gpt-5.x REJECTS max_tokens); ai.rag span records gen_ai.system + gen_ai.request.model for Langfuse
- DELIBERATE behavior changes in 01-04 (re-baseline eval runner): judge score-based retry loop removed (single pass + one judge call; transient-error retry kept) and search_depth SNELLE/UITGEBREIDE depth prompt removed for both modes (field still accepted + logged)
- run_rag returns chunk-derived ToolEvidence (option-A); chat.rs passes it to run_judge — verifier.rs stays disk-based until Phase 3
- Admin document endpoints (02-01): no in-service auth (portal fronts /admin, per plan); upload = whole-request 400 on any invalid file; 202 body exactly [{filename, status:"pending"}] (ids come from GET list); pending row pre-created before 202 so the list reflects uploads immediately (ingest re-upserts same id); per-route DefaultBodyLimit 60MB overrides global 64KB; /pdf serves bytea inline with sanitized Content-Disposition
- Frontend document manager (02-02): single dropzone (drag-drop + click) uploads immediately with a busy state, no separate select/confirm step; polling is a self-scheduling setTimeout (not setInterval) that re-arms only while any doc is pending/indexing; delete uses window.confirm (diverges from QuestionsTab's inline-confirm-row pattern, per explicit plan instruction); help content preserved behind a closed-by-default collapsible ("Toon/Verberg help & uitleg", same idiom as JudgePanel's details toggle)
- Windows dev host cannot run `npm run build`/`vite` locally (Group Policy blocks unsigned native exe like esbuild.exe, code 1260); verify frontend builds/tests via `docker build -f apps/teun/Dockerfile --target frontend .` (same toolchain as production) or `docker run node:22-bookworm-slim` for vitest — not a code issue, `tsc -b` alone runs fine on host

### Pending Todos

None yet.

### Blockers/Concerns

- Brownfield: SSE `ChatEvent` + `MortgageAnswer`/`SourceReference` + existing REST endpoints must stay backward-compatible (frontend, judge, eval, sessions depend on them).
- ~~`pdfium-render` requires the PDFium native library bundled into the Docker image before Phase 1 ships.~~ RESOLVED by Plan 01-02 (chromium/7881 bundled, in-image extraction verified).
- pgvector `vector` extension must be enabled via sqlx migration before the app serves traffic.
- This Windows dev host's Group Policy blocks execution of unsigned native binaries (e.g. `esbuild.exe`) — local `npm run build`/`vitest` must run inside Docker (`node:22-bookworm-slim`, same as the production `frontend` Dockerfile stage) rather than directly on host. Not a code/deploy blocker (production build is already containerized), just a local-dev workflow note.

## Deferred Items

Items acknowledged and carried forward from previous milestone close:

| Category | Item | Status | Deferred At |
|----------|------|--------|-------------|
| *(none)* | | | |

## Session Continuity

Last session: 2026-07-24 (executed Plan 02-02: DocumentManager.tsx + DocsTab rework + api.ts helpers; build/tests verified via `docker build --target frontend` and `docker run node:22-bookworm-slim` since the Windows host blocks local esbuild.exe execution via Group Policy — tsc -b passes directly on host)
Stopped at: Plan 02-02 complete (SUMMARY written). Phase 2 plans both done (02-01 backend, 02-02 frontend). Next: orchestrator live verification — (1) the 02-01 HTTP smoke (upload real PDF via curl -F, poll list until indexed, ask citing question, serve /pdf inline, delete, confirm gone) against the running teun-svc container, and (2) visually verify the rebuilt Documentatie tab (upload, status polling, delete, collapsible help) in a browser once the image is rebuilt with this frontend change. Then Phase 3 (Citation Integrity & Interface Preservation) planning.
Resume file: .planning/phases/02-admin-documents/02-02-SUMMARY.md + .planning/phases/02-admin-documents/02-01-SUMMARY.md + this STATE.md.
Env for E2E (values in NextEpoch App settings / .env, keys NEVER in repo): DATABASE_URL (pgvector, seeded) · AZURE_OPENAI_ENDPOINT=https://dmfco-ai-tools-resource.cognitiveservices.azure.com/ · AZURE_OPENAI_DEPLOYMENT=text-embedding-3-large · AZURE_OPENAI_API_VERSION=2024-02-01 · AZURE_OPENAI_API_KEY=<set> · ANTHROPIC_API_KEY=<set, also used by judge> · LLM_PROVIDER=anthropic|azure-openai · AZURE_OPENAI_CHAT_DEPLOYMENT=gpt-5.6-luna (azure generation) · optional RAG_MODEL, LANGFUSE_*.

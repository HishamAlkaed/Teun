# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-07-17)

**Core value:** Grounded, source-cited mortgage-acceptance answers at a fraction of the current per-request cost — by retrieving only relevant policy passages, with citations still verifiable against the source documents.
**Current focus:** Phase 1 — RAG Retrieval Core & PDF Ingestion

## Current Position

Phase: 1 of 3 (RAG Retrieval Core & PDF Ingestion)
Plan: 3 of 4 executed (01-03 COMPLETE — all 3 tasks done, seed corpus live: 4 docs indexed, 210 chunks in pgvector)
Status: READY — next execute Plan 01-04 (retriever + run_rag swap, delete claude.rs). Earlier 01-01/01-02 human checkpoints effectively satisfied by the live seed run (pgvector migration applied, Azure embeddings working, extraction quality proven in production path).
Last activity: 2026-07-24 — Executed Plan 01-03 (chunk.rs + ingest.rs + bin/ingest); REAL seed run against teun-pg-dev with Azure embeddings: all 4 acceptatie PDFs status=indexed (58/32/79/41 chunks, 210 total), re-run idempotent (total unchanged). Commits c9583e4/79730c7/a925510/41802f8 on gsd/rag-rebuild.

Progress: [███████░░░] ~70% (3 of 4 plans executed)

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
- Total plans completed: 3 (01-01, 01-02, 01-03)
- Average duration: ~30 min
- Total execution time: ~1.5 hours

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 01 | 3 (01-01: 3 tasks, 9 files · 01-02: 2 tasks, 5 files · 01-03: 3 tasks, 7 files) | ~90 min | ~30 min |

**Recent Trend:**
- Last 5 plans: —
- Trend: —

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
- PDF bytes stored in Postgres `bytea`; `mode` field (tools/inline) collapses to one RAG answer path

### Pending Todos

None yet.

### Blockers/Concerns

- Brownfield: SSE `ChatEvent` + `MortgageAnswer`/`SourceReference` + existing REST endpoints must stay backward-compatible (frontend, judge, eval, sessions depend on them).
- ~~`pdfium-render` requires the PDFium native library bundled into the Docker image before Phase 1 ships.~~ RESOLVED by Plan 01-02 (chromium/7881 bundled, in-image extraction verified).
- pgvector `vector` extension must be enabled via sqlx migration before the app serves traffic.

## Deferred Items

Items acknowledged and carried forward from previous milestone close:

| Category | Item | Status | Deferred At |
|----------|------|--------|-------------|
| *(none)* | | | |

## Session Continuity

Last session: 2026-07-24 (executed Plan 01-03; seed run executed FOR REAL in the `teun-pdfium-spike` runtime image against `teun-pg-dev` (localhost:15432 from host / host.docker.internal:15432 from containers) with Azure credentials from .env via --env-file)
Stopped at: Plan 01-03 complete (SUMMARY written). Next: execute Plan 01-04 (retriever + run_rag single-call answer path, delete claude.rs + whole-corpus inline). The 01-01/01-02 checkpoint items are demonstrated working by the live seed run (migration 003 applied, vector(3072) inserts OK, Azure embeddings OK, extraction quality good in practice).
Resume file: .planning/phases/01-rag-retrieval-core/01-03-SUMMARY.md + this STATE.md.
Embedding env vars (values live in NextEpoch App settings, key NEVER in repo): AZURE_OPENAI_ENDPOINT=https://dmfco-ai-tools-resource.cognitiveservices.azure.com/ · AZURE_OPENAI_DEPLOYMENT=text-embedding-3-large · AZURE_OPENAI_API_VERSION=2024-02-01 · AZURE_OPENAI_API_KEY=<set in App settings>.

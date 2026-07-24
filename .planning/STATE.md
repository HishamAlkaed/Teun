# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-07-17)

**Core value:** Grounded, source-cited mortgage-acceptance answers at a fraction of the current per-request cost — by retrieving only relevant policy passages, with citations still verifiable against the source documents.
**Current focus:** Phase 1 — RAG Retrieval Core & PDF Ingestion

## Current Position

Phase: 1 of 3 (RAG Retrieval Core & PDF Ingestion)
Plan: 2 of 4 executed (01-02 tasks 1-2 done; BLOCKED on its human-verify quality gate. 01-01 checkpoint also still pending)
Status: CHECKPOINT — Plan 01-02 awaits human verify (extraction quality gate; executor recommends approve, no .md fallback). Plan 01-01 checkpoint (pgvector + Azure env vars) also outstanding.
Last activity: 2026-07-24 — Executed Plan 01-02 (pdfium extract.rs + PDFium chromium/7881 bundled in Docker, spike PASSED on all 4 seed PDFs in-image); commits e4fc501/cf7d436/e08eecb on gsd/rag-rebuild

Progress: [█████░░░░░] ~45% (2 of 4 plans executed, checkpoints pending)

## Resume Next Week (paused 2026-07-17)

**Branch:** `gsd/rag-rebuild` (all planning committed; Teun default branch = `main`).

**Next action:** execute Phase 1 Plan `01-01` (data layer + embeddings). Then 01-02 → 01-03 → 01-04 in order (sequential waves).

**Do BEFORE executing (prereqs, user-side):**
1. pgvector `vector` extension available on target Postgres (Plan 01-01 has a blocking checkpoint that checks `pg_available_extensions`).
2. Azure OpenAI embeddings vars set as NextEpoch App settings: `AZURE_OPENAI_ENDPOINT`, `AZURE_OPENAI_API_KEY`, `AZURE_OPENAI_DEPLOYMENT=text-embedding-3-large`, `AZURE_OPENAI_API_VERSION=2024-02-01`. (Provided; key never committed.)
3. PDFium native lib bundling in Docker — LOW-confidence spike; Plan 01-02 validates `.so` load + extraction on real PDFs before 01-03 depends on it. `.md` files are fallback if extraction quality poor.

**How to execute (toolchain note):** on-PATH `gsd-sdk` is `@gsd-build/sdk` v0.1.0 (run/auto/init) and LACKS the `gsd-sdk query` glue the `~/.claude/get-shit-done/` skill workflows call — so `/gsd:*` commands won't run as-scripted. Drive manually: spawn the installed `gsd-executor` agent per plan (worked fine for gsd-roadmapper/gsd-phase-researcher/gsd-planner/gsd-plan-checker), do git ops by hand.

**Plans:** 01-01 data layer+embeddings (RET-01,02,ING-03) · 01-02 PDFium extract (ING-01) · 01-03 chunker+ingest+seed (RET-03,ING-02,04) · 01-04 retriever+run_rag swap, delete claude.rs (RET-04,05,06).

**Seed corpus:** ingest 4 PDFs from resources/acceptatie/; skip near-duplicate `handboek_accept_versie_2026_4.pdf` (use `_definitief`).

## Performance Metrics

**Velocity:**
- Total plans completed: 2 (01-01, 01-02; checkpoints pending)
- Average duration: ~35 min
- Total execution time: ~1.2 hours

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 01 | 2 (01-01: 3 tasks, 9 files · 01-02: 2 tasks, 5 files) | ~70 min | ~35 min |

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

Last session: 2026-07-24 (executed Plan 01-02 via dockerized cargo helper `teun-rust-build`; full image built as `teun-pdfium-spike`, PDFium spike PASSED in-image on all 4 seed PDFs)
Stopped at: Plan 01-02 tasks 1-2 committed + SUMMARY written. BLOCKED on TWO human-verify checkpoints before 01-03: (a) 01-01 — pgvector available on DATABASE_URL Postgres, migration 003 applies, AZURE_OPENAI_* set in NextEpoch App settings; (b) 01-02 — extraction quality gate (executor recommends "approved: PDFium loads, extraction quality acceptable", no .md fallback). Then execute 01-03 (chunker + ingest + seed).
Resume file: .planning/phases/01-rag-retrieval-core/01-02-SUMMARY.md + 01-01-SUMMARY.md (checkpoint steps) + this STATE.md.
Embedding env vars (values live in NextEpoch App settings, key NEVER in repo): AZURE_OPENAI_ENDPOINT=https://dmfco-ai-tools-resource.cognitiveservices.azure.com/ · AZURE_OPENAI_DEPLOYMENT=text-embedding-3-large · AZURE_OPENAI_API_VERSION=2024-02-01 · AZURE_OPENAI_API_KEY=<set in App settings>.

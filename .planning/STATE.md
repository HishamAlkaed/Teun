# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-07-17)

**Core value:** Grounded, source-cited mortgage-acceptance answers at a fraction of the current per-request cost — by retrieving only relevant policy passages, with citations still verifiable against the source documents.
**Current focus:** Phase 1 — RAG Retrieval Core & PDF Ingestion

## Current Position

Phase: 1 of 3 (RAG Retrieval Core & PDF Ingestion)
Plan: 4 of 4 executed; 01-04 E2E checkpoint RUN AND PASSED (2026-07-24, orchestrator-run over live HTTP/SSE against local pgvector + real keys)
Status: PHASE 1 CODE-COMPLETE. E2E results: mode=tools and mode=inline both route to run_rag; correct answer (€1.350.000) with cited sources; judge score 95 with 4/4 source verdicts ok (after verifier fix below); provider flip verified over HTTP for BOTH anthropic and azure-openai (gpt-5.6-luna, gen_ai.* span fields present); system_prompt_len=29357 chars ≈ 8k tokens for 8 chunks vs ~110k before (~93% prompt reduction). Retrieval-miss behavior verified honest (model says passages don't cover it; no fabrication).
Additional commit 06fe9d6: Phase-3 verifier rewire PULLED FORWARD — E2E exposed that verifier.rs read .md from disk while citations reference documents.extracted_text, making every verdict document_not_found (score ~15 on correct answers). verify_sources now loads from the DB (disk fallback kept). Phase 3 scope note: quote/line verification is DONE; remaining Phase-3 scope is DocumentViewer against PDF-extracted text.
Remaining before deploy (user-side, see .planning/DEPLOYMENT-CHECKLIST.md): pgvector on target Postgres, env vars in NextEpoch App settings, seed ingest run against production DB (cargo run --bin ingest).
Last activity: 2026-07-24 — Executed Plan 01-04 + E2E checkpoint + verifier pull-forward. Full suite 80 passed/0 failed.

Progress: [██████████] 100% of Phase 1 (deploy prereqs user-side)

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
- Total plans completed: 4 (01-01, 01-02, 01-03, 01-04)
- Average duration: ~30 min
- Total execution time: ~2 hours

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 01 | 4 (01-01: 3 tasks, 9 files · 01-02: 2 tasks, 5 files · 01-03: 3 tasks, 7 files · 01-04: 4 tasks, 9 files) | ~125 min | ~31 min |

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
- PDF bytes stored in Postgres `bytea`; `mode` field (tools/inline) collapses to one RAG answer path — DONE in 01-04 (both modes → run_rag; claude.rs + inline.rs deleted; image de-Node'd)
- Generation LLM switchable via LLM_PROVIDER=anthropic (default, RAG_MODEL→INLINE_MODEL→CLAUDE_MODEL) | azure-openai (AZURE_OPENAI_CHAT_DEPLOYMENT=gpt-5.6-luna; body uses max_completion_tokens — gpt-5.x REJECTS max_tokens); ai.rag span records gen_ai.system + gen_ai.request.model for Langfuse
- DELIBERATE behavior changes in 01-04 (re-baseline eval runner): judge score-based retry loop removed (single pass + one judge call; transient-error retry kept) and search_depth SNELLE/UITGEBREIDE depth prompt removed for both modes (field still accepted + logged)
- run_rag returns chunk-derived ToolEvidence (option-A); chat.rs passes it to run_judge — verifier.rs stays disk-based until Phase 3

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

Last session: 2026-07-24 (executed Plan 01-04 in the `teun-rust-build` helper image; live smoke tests against `teun-pg-dev` (localhost:15432 from host / host.docker.internal:15432 from containers) with real credentials from .env via --env-file — both LLM providers answered the test question with cited line ranges)
Stopped at: Plan 01-04 auto tasks complete (SUMMARY written). Next: run the 01-04 blocking checkpoint:human-verify (E2E POST /api/teun/chat for both modes, judge event with chunk-derived evidence, docker build Node-free, LLM_PROVIDER flip anthropic↔azure-openai + Langfuse ai.rag span check). After approval: Phase 1 complete.
Resume file: .planning/phases/01-rag-retrieval-core/01-04-SUMMARY.md + this STATE.md.
Env for E2E (values in NextEpoch App settings / .env, keys NEVER in repo): DATABASE_URL (pgvector, seeded) · AZURE_OPENAI_ENDPOINT=https://dmfco-ai-tools-resource.cognitiveservices.azure.com/ · AZURE_OPENAI_DEPLOYMENT=text-embedding-3-large · AZURE_OPENAI_API_VERSION=2024-02-01 · AZURE_OPENAI_API_KEY=<set> · ANTHROPIC_API_KEY=<set, also used by judge> · LLM_PROVIDER=anthropic|azure-openai · AZURE_OPENAI_CHAT_DEPLOYMENT=gpt-5.6-luna (azure generation) · optional RAG_MODEL, LANGFUSE_*.

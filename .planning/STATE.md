# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-07-17)

**Core value:** Grounded, source-cited mortgage-acceptance answers at a fraction of the current per-request cost — by retrieving only relevant policy passages, with citations still verifiable against the source documents.
**Current focus:** Phase 3 — Citation Integrity & Interface Preservation

## Current Position

Phase: 3 of 3 (Citation Integrity & Interface Preservation)
Plan: 1 of 1 executed (03-01 DB-backed document viewer endpoints + regression gate DONE)
Status: 03-01 complete — GET /api/teun/documents (list) and GET /api/teun/documents/{filename} (content) are DB-first: list = indexed docs from RagStore::list_documents merged with legacy resources-dir .md files; content = RagStore::get_extracted_text (the canonical body chunk line numbers cite) with disk fallback, response shape unchanged plus additive content_type ("pdf_text"|"markdown"); DocumentViewer defaults pdf_text docs to the line view. CIT-01 verified as already delivered by 06fe9d6 (verifier reads extracted_text) — no re-implementation. CIT-03 evidence: git diff b4a86f4..HEAD on agent/types.rs, agent/stream.rs, routes/chat.rs, judge/* is EMPTY. Suite 91 passed/0 failed/12 ignored; web build green in the Dockerfile frontend stage. Orchestrator still owes: 02-01 live HTTP smoke, 02-02 tab visual check, and the 03-01 live E2E SSE-shape + citation-open check (running teun-svc has the OLD binary — rebuild/restart needed to exercise the new endpoints live).
Phase 1: COMPLETE (E2E checkpoint passed 2026-07-24; verifier rewire pulled forward in 06fe9d6). Phase 2: plans complete, live smoke pending.
Remaining before deploy (user-side, see .planning/DEPLOYMENT-CHECKLIST.md): pgvector on target Postgres, env vars in NextEpoch App settings, seed ingest run against production DB (cargo run --bin ingest).
Last activity: 2026-07-24 — Executed Plan 03-01 (DB-backed viewer endpoints + CIT-03 regression evidence). Commits dab36d6, ebb262a.

Progress: [██████████] Phase 3: 1/1 plans (Phase 1: 4/4, Phase 2: 2/2 done)

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
- Total plans completed: 7 (01-01, 01-02, 01-03, 01-04, 02-01, 02-02, 03-01)
- Average duration: ~26 min
- Total execution time: ~2h35m

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 01 | 4 (01-01: 3 tasks, 9 files · 01-02: 2 tasks, 5 files · 01-03: 3 tasks, 7 files · 01-04: 4 tasks, 9 files) | ~125 min | ~31 min |
| 02 | 2 (02-01: 2 tasks, 7 files · 02-02: 1 task, 3 files) | ~35 min | ~18 min |
| 03 | 1 (03-01: 3 tasks, 3 files) | ~15 min | ~15 min |

**Recent Trend:**
- Last 5 plans: 01-04, 02-01, 02-02, 03-01 — all green, no checkpoints hit
- Trend: stable; Phase 3 done in a single plan as planned

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
- Document viewer endpoints DB-first (03-01): GET /documents lists indexed DB docs (line_count from extracted_text so it matches the content endpoint's total_lines) merged with legacy .md dir files; GET /documents/{filename} serves extracted_text (empty text = pending row, treated as absent) with disk fallback, unchanged shape + additive content_type ("pdf_text"|"markdown"); DocumentViewer defaults pdf_text to the "lines" view (extracted text is not markdown). DB errors degrade to disk, 500 only if both sources fail
- Root .dockerignore added (03-01, Rule 3): host node_modules holds Linux symlinks from 02-02's in-container npm ci which broke docker build context transfer on Windows; both Dockerfile stages install their own deps in-image, so excluding **/node_modules + .git is strictly more correct (frontend stage's COPY previously overlaid host node_modules onto clean npm ci output)

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

Last session: 2026-07-24 (executed Plan 03-01: documents.rs endpoints DB-first + DocumentViewer pdf_text default view + CIT-03 zero-diff evidence; cargo via teun-rust-build helper, web build via the Dockerfile frontend stage after adding a root .dockerignore to fix the Linux-symlink context-transfer break)
Stopped at: Plan 03-01 complete (SUMMARY written). ALL milestone plans executed (Phase 1: 4/4, Phase 2: 2/2, Phase 3: 1/1). Next: orchestrator live verification — (1) rebuild the teun image and restart teun-svc (it runs the OLD binary; new endpoints not live yet), (2) 02-01 HTTP smoke (upload→indexed→cite→serve /pdf→delete), (3) 02-02 Documentatie tab visual check, (4) 03-01 live E2E: ask a citing question, open a citation, confirm DocumentViewer shows DB extracted_text in line view with the cited range highlighted, and re-run the SSE ChatEvent shape check (CIT-03). Then milestone close-out.
Resume file: .planning/phases/03-citation-integrity/03-01-SUMMARY.md + this STATE.md.
Env for E2E (values in NextEpoch App settings / .env, keys NEVER in repo): DATABASE_URL (pgvector, seeded) · AZURE_OPENAI_ENDPOINT=https://dmfco-ai-tools-resource.cognitiveservices.azure.com/ · AZURE_OPENAI_DEPLOYMENT=text-embedding-3-large · AZURE_OPENAI_API_VERSION=2024-02-01 · AZURE_OPENAI_API_KEY=<set> · ANTHROPIC_API_KEY=<set, also used by judge> · LLM_PROVIDER=anthropic|azure-openai · AZURE_OPENAI_CHAT_DEPLOYMENT=gpt-5.6-luna (azure generation) · optional RAG_MODEL, LANGFUSE_*.

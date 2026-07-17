# Teun — RAG Backend Rebuild

## What This Is

Teun is a Dutch-language AI assistant that answers mortgage-acceptance ("acceptatie") questions for MUNT staff, citing the exact policy passages behind each answer. This milestone rebuilds Teun's answer backend into a real Retrieval-Augmented Generation (RAG) system — modeled on the `consumenten_chatbot` architecture — while keeping the existing HTTP/SSE interface, structured answer format, judge, sessions, eval, and web frontend unchanged. It also adds PDF document ingestion and admin-managed document upload via the web interface.

## Core Value

Teun answers a mortgage-acceptance question with a grounded, source-cited answer at a **fraction of the current per-request cost**, by retrieving only the relevant policy passages instead of feeding the whole corpus (or spawning a full agent) on every request — with citations still verifiable against the source documents.

## Requirements

### Validated

<!-- Existing capabilities inferred from the codebase (see .planning/codebase/TEUN_MAP.md) -->

- ✓ SSE chat endpoint `POST /api/teun/chat` with `ChatEvent` stream — existing
- ✓ Structured `MortgageAnswer` (answer, rationale, sources, category) two-phase output — existing
- ✓ Judge scoring + retry loop (quality gate ≈ CC's QAAgent) — existing
- ✓ Source citation verification: literal quote + line_range checked against docs (`judge/verifier.rs`) — existing
- ✓ Session persistence, eval runner, feedback capture (Postgres via sqlx) — existing
- ✓ Langfuse/OTLP cost + performance tracing — existing
- ✓ React+Vite+TS frontend: chat UI + admin tabs (Evaluatie/Vragen/Feedback/Documentatie/Instellingen) + line-highlight DocumentViewer — existing

### Active

<!-- New scope for this milestone. Hypotheses until shipped. -->

**Retrieval (RAG core — the cost fix)**
- [ ] pgvector store + schema (documents + chunks with embedding vectors)
- [ ] OpenAI `text-embedding-3-small` embedding client (via reqwest)
- [ ] Chunker producing chunks tagged with `{document, line_start, line_end, page}`
- [ ] Retriever: embed query → top-K similarity search over chunks
- [ ] New answer path: prompt built from top-K chunks → single Anthropic Messages call → `MortgageAnswer` (replaces `run_claude` subprocess + `run_inline` whole-corpus dump)
- [ ] `mode` field (tools/inline) repointed to the single RAG path (interface preserved)

**PDF ingestion**
- [ ] pdfium-render text extraction → line-numbered canonical body + page map
- [ ] Ingest pipeline: extract → chunk → embed → store (mirrors CC `_process_document`)
- [ ] Store original PDF bytes in Postgres `bytea`
- [ ] Ingest the existing `resources/acceptatie/` PDFs (seed corpus)

**Admin document management (web interface)**
- [ ] Upload PDF via web (multipart endpoint, MIME + size validation)
- [ ] List documents with ingestion status (DB-backed)
- [ ] Delete document (wipe chunks + bytes + row)
- [ ] New admin doc-manager UI tab (upload / list / delete / status)
- [ ] Serve original PDF bytes for viewing / reference render

**Citation integrity (preserve interface)**
- [ ] `verifier.rs` verifies quote + line_range against stored extracted text
- [ ] DocumentViewer renders extracted text with line highlight for PDF-sourced docs

### Out of Scope

- Web scraping / BFS crawler — Teun's corpus is admin-uploaded static policy PDFs, not crawled sites
- Tiered pdf/table indexes + stub-discovery pattern (CC-specific complexity) — Teun uses one flat chunk index; policy docs are prose, not table-heavy
- Azure AI Search / Azure Blob Storage — replaced by pgvector + Postgres bytea to reuse the existing stack
- Non-PDF upload formats (docx, xlsx) — PDF first; others can follow
- Auth/authorization rework — reuse the existing portal auth gating admin routes
- LLM-generated stub descriptions — not needed without the tiered/stub architecture

## Context

- **Existing analysis:** `.planning/codebase/TEUN_MAP.md` (full diagnosis of current answer engine + interface contract). READ FIRST.
- **Reference architecture:** `consumenten_chatbot/.planning/codebase/` (Python/FastAPI/Azure real-RAG, already GSD-mapped). Teun ports the *concepts*, not the code (different language/stack).
- **Why now:** current Teun is too expensive per request. `mode=tools` spawns a full `claude` CLI subprocess per turn (+ judge + retries); `mode=inline` dumps ALL policy docs (~100K tokens) into the system prompt every request. Neither retrieves.
- **Corpus:** `resources/acceptatie/` — MUNT policy docs, now available as PDFs (5 PDFs incl. two `handboek` variants where `_definitief` is the final; filenames contain spaces) plus the older `.md` versions.
- **Citation model:** Teun's verify/highlight system is line-number based. PDFs are page-based, so on ingest each PDF is converted to a line-numbered canonical text body used by the verifier and viewer; chunks additionally carry the source page.

## Constraints

- **Tech stack**: Rust — axum 0.8, tokio, sqlx(Postgres), reqwest, `pdfium-render`, `pgvector`. Frontend React+Vite+TS. — Must stay within existing stack.
- **Interface compatibility**: SSE `ChatEvent` contract + `MortgageAnswer`/`SourceReference` + existing REST endpoints MUST stay backward-compatible — the shipped web frontend, judge, eval, and sessions depend on them.
- **Native dependency**: `pdfium-render` needs the PDFium native library bundled into the Docker image.
- **Embeddings provider**: OpenAI `text-embedding-3-small` — requires an OpenAI (or Azure OpenAI) API key as an env/App setting; cross-provider from the Anthropic answer model.
- **Postgres + pgvector**: the `vector` extension must be enabled via migration before the app serves traffic.
- **Security**: admin upload/delete must sit behind the existing portal auth (`AUTH_USERNAME`/`AUTH_PASSWORD`); PDF path/filename handling must keep the existing path-traversal guards.

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Vector store = pgvector | Reuse existing sqlx + Postgres; no new external service | — Pending |
| Embeddings = OpenAI text-embedding-3-small | Same as reference (CC); cheap, strong, simple REST | — Pending |
| `mode` field → single RAG path | One cheap backend; keep field only for frontend compat | — Pending |
| PDF extraction = pdfium-render | Best text/layout fidelity in Rust for complex policy PDFs | — Pending |
| Citations = line-based over extracted text | Keeps `verifier.rs` + DocumentViewer + quote/line_range working unchanged; tag chunks with page | — Pending |
| PDF bytes = Postgres bytea | No Azure blob; transactional with metadata; fine for a handful of PDFs | — Pending |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition:**
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone:**
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-07-17 after initialization*

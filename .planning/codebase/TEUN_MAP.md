# Teun — Codebase Map & RAG Rebuild Analysis

**Analysis Date:** 2026-07-17
**Goal:** Rebuild Teun's answer backend as a real RAG (like `consumenten_chatbot`), keeping the existing interface unchanged.

---

## What Teun is

Rust monorepo. AI mortgage-advice assistant ("acceptatie" = mortgage acceptance policy) over 4 static MUNT policy markdown docs.

```
Teun/
├── Cargo.toml                    # workspace, member: apps/teun/service
├── apps/teun/
│   ├── service/                  # Rust axum backend (the part to rebuild)
│   │   ├── src/
│   │   │   ├── main.rs           # axum app, AppState, router wiring
│   │   │   ├── agent/            # ANSWER ENGINE (the expensive part)
│   │   │   │   ├── claude.rs     # "tools" mode: spawn `claude` CLI subprocess
│   │   │   │   ├── inline.rs     # "inline" mode: dump ALL docs into system prompt
│   │   │   │   └── types.rs      # MortgageAnswer, SourceReference, ChatEvent, ToolEvidence
│   │   │   ├── routes/           # chat, sessions, documents, admin, settings, health, scrub
│   │   │   ├── judge/            # post-answer LLM judge + deterministic source verifier
│   │   │   ├── eval/             # eval runner/store (regression question sets)
│   │   │   ├── session/          # session store (postgres via sqlx)
│   │   │   └── telemetry.rs      # Langfuse OTLP tracing (cost tracking)
│   │   └── config/acceptatie-beleid.md   # skill/system prompt (146 lines, NL)
│   └── web/                      # React + Vite + TS frontend (KEEP AS-IS)
└── resources/acceptatie/         # 4 MUNT .md docs, ~5391 lines / ~400KB / ~100K tokens
```

**Stack:** Rust, axum 0.8, tokio, sqlx (**postgres**), reqwest, tower-http, Langfuse/OTLP.
**No vector DB, no embeddings, no chunking, no retrieval** — confirmed by grep. This is the core gap.

---

## How Teun answers today (WHY it costs too much)

`POST /api/teun/chat` (SSE) → `routes/chat.rs`. Two modes, both expensive:

### Mode `tools` (default)
`agent/claude.rs::run_claude` spawns the **`claude` CLI as a subprocess** per turn:
- `--add-dir resources/acceptatie` + skill prompt; Claude Code agent uses **Grep/Read tools live** to search the MUNT docs.
- Then `judge::run_judge` (another LLM call) scores the answer; if score < `retry_threshold`, the **whole agent is re-run** (up to `max_retries`) with feedback appended.
- Cost = full Claude Code agent session (Sonnet) × (1 + retries) + judge calls. Every question. No caching of retrieval.

### Mode `inline`
`agent/inline.rs::run_inline` calls Anthropic Messages API directly:
- `load_inline_documents()` reads **ALL 4 markdown docs (~100K tokens), line-numbered, concatenated into the system prompt every request**.
- Uses `cache_control: ephemeral` prompt caching to soften repeat cost, but base input is still ~100K tokens/request on **Opus** (`claude-opus-4-6`) + judge.

Neither mode retrieves. Both push the entire corpus (or grep it live) per query → the cost problem.

---

## Interface contract (MUST preserve when rebuilding backend)

The web frontend + eval + judge + sessions all depend on these. Do not change:

- **Endpoint:** `POST /api/teun/chat`, SSE response. Request: `{message, session_id?, mode, language, search_depth, skip_persist}`.
- **SSE events** (`ChatEvent`, tag=`type`, content=`data`, snake_case):
  `thinking`, `tool_use{tool,input,content?}`, `partial{content}`,
  `result{structured_output, session_id, message_id}`, `error{message}`, `judge{result}`.
- **Structured answer** (`MortgageAnswer`): `answer`, `rationale`, `sources[]`, `category` (`standard` | `mandaat_uitzondering` | `doorverwijzen_speciale_afhandeling`).
- **SourceReference**: `document`, `section`, `quote?`, `line_range?`.
  → **Citations carry literal quote + line numbers into the source doc.** `judge/verifier.rs` re-checks each quote against the doc (fuzzy word-LCS, ≥0.7) and marks unreliable ones.
- Two-phase streaming protocol (inline): answer text, then `\n---JSON---\n{rationale,sources,category}`.
- Session persistence, feedback, eval runner, Langfuse cost spans — all keep working.

---

## Target architecture: consumenten_chatbot (CC)

Real RAG (Python/FastAPI/Azure). See `consumenten_chatbot/.planning/codebase/`:
- **Ingestion (once):** scrape/load → clean → chunk (tiktoken, overlap) → embed → upsert to vector index + metadata DB.
- **Query time:** embed query → **hybrid search top-K chunks** → feed ONLY those to LLM → tool-calling for on-demand tables/pdfs/web → QA agent loop (PASS/RETRY/DECLINE) → `[N]` cited answer.
- **Tiered indexes** (primary/pdf/table) + stub-discovery pattern.
- Only retrieved chunks hit the LLM context → cheap per request.

CC's QAAgent ↔ Teun's judge: same idea (post-answer quality gate + retry). Teun already has this half.

---

## The gap → rebuild plan (backend only)

Teun needs the retrieval half CC has and Teun lacks. Teun's corpus is 4 static docs (no scraping/scheduler needed — ingestion is far simpler than CC's).

**Recommended (fits existing stack): pgvector.** Teun already uses `sqlx` + Postgres. Add pgvector extension instead of a new store (LanceDB/Azure). Rust: `sqlx` + `pgvector` crate; embeddings via reqwest (Anthropic has none — use OpenAI `text-embedding-3-large`, or Voyage, or a local model). Decision needed: which embedding provider.

**New pieces:**
1. **Ingest step** (offline/startup binary): chunk the 4 MUNT `.md` files (preserve source filename + **line ranges per chunk** so citations still resolve — critical for `verifier.rs`), embed, store in pgvector.
2. **Retriever**: embed query → top-K similarity search → return chunks with `{document, line_start, line_end, text}`.
3. **New answer path**: build prompt = skill + top-K chunks (with line numbers) → single Messages API call → same two-phase `MortgageAnswer` output. Replaces both `run_claude` (subprocess) and `run_inline` (whole-corpus dump).
4. Keep: judge + verifier (quote/line check still works since chunks carry line numbers), sessions, eval, SSE events, frontend.

**Cost win:** ~100K tokens/request (inline) or full agent session (tools) → ~2–8K tokens/request (top-K chunks). Judge retries optionally kept for "uitgebreid" search_depth.

**Open decisions before planning:**
- Embedding provider (OpenAI / Voyage / local)?
- Vector store: pgvector (recommended, reuses stack) vs LanceDB (in .env.example but unused by service)?
- Keep both `mode` values as interface no-ops, or repoint them to the new single RAG path?
- Chunking strategy for policy docs (section-aware by markdown headings vs fixed-token windows).

---

## Added scope: PDF ingestion + admin document upload

**Requirement:** docs are `.md` today but must be ingestable as **PDFs**, and the **admin must add docs via the interface**.

### CC pattern to port (reference)
- `infrastructure/pdf/extractor.py` — `PDFExtractor` (pdfplumber). `extract_from_bytes(bytes)` → `ExtractedPDF{full_text, page_count, pages[], metadata}`.
- `presentation/api/v1/documents.py`:
  - `POST /sources/{id}/documents/upload` — multipart, validate MIME=`application/pdf` + 50MB cap → store bytes in blob → `SourceDocument` row (status PENDING) → `BackgroundTasks` enqueue ingest.
  - Ingest `_process_document`: bytes → `extract_from_bytes` → `chunk_flat(full_text)` → embed → upsert pdf-index; stub → primary-index. Tracks `chunk_count`, `indexed_at`, `error_message`.
  - `GET /sources/documents/{id}/file` — stream bytes inline for `[N]` reference render.
  - `DELETE /sources/{id}/documents/{id}` — wipe index chunks + blob + row.

### Teun current doc surface (gap)
- Storage = static `resources/acceptatie/*.md` on disk, no DB rows.
- `routes/documents.rs` = READ-ONLY (`GET /documents` lists `.md`; `GET /documents/{filename}` returns content + line highlight). No upload/delete.
- Web "Documentatie" tab (`DocsTab.tsx`) = static help text, NOT a manager. Admin tabs (`AdminLayout.tsx`) = Evaluatie/Vragen/Feedback/Documentatie/Instellingen.
- `routes/admin.rs` = questions/eval/feedback only. **No source/document CRUD.**
- Admin auth = portal `AUTH_USERNAME`/`AUTH_PASSWORD` (verify how admin routes are gated — likely middleware/portal; confirm before adding upload).

### Decisions locked (2026-07-17)
- **Vector store:** pgvector (reuse sqlx+postgres).
- **Embeddings:** OpenAI `text-embedding-3-large`.
- **`mode` field:** repoint tools/inline → single RAG path.
- **PDF extraction:** `pdfium-render` (bundle PDFium native lib in Docker).
- **Citations:** line-based over extracted text — extract PDF→text, store line-numbered body as canonical document so `verifier.rs` + `DocumentViewer` + quote/line_range keep working; tag chunks with page.
- **PDF byte storage:** Postgres `bytea` (no Azure blob).

### New pieces this adds
- PDF extract module (pdfium) → text + page map.
- Ingest a `SourceDocument` (upload) instead of only startup .md load: extract → line-number body → chunk (w/ line+page) → embed → pgvector.
- Endpoints: `POST /api/teun/documents` (multipart upload), `DELETE /api/teun/documents/{id}`, keep `GET` list/view (now DB-backed); optional `GET /documents/{id}/file` to serve original PDF.
- DB: `documents` (bytea + extracted text + status + chunk_count) + `chunks` (embedding vector + document_id + line_start/end + page).
- Web: new admin doc-manager tab (upload/list/delete/status), preserve help content; `web/src/lib/api.ts` upload/delete fns.

## Status of maps
- `consumenten_chatbot`: fully mapped by GSD — `consumenten_chatbot/.planning/codebase/` (ARCHITECTURE, STACK, STRUCTURE, CONVENTIONS, INTEGRATIONS, TESTING, CONCERNS). Reuse as the reference design.
- `Teun`: NOT previously mapped. This file is the first map.

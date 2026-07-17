# Phase 1: RAG Retrieval Core & PDF Ingestion - Research

**Researched:** 2026-07-17
**Domain:** Rust RAG backend — PDF extraction (pdfium), pgvector over sqlx, OpenAI embeddings, token chunking, single-call Anthropic answer path
**Confidence:** HIGH for crate versions & APIs; MEDIUM for chunking/answer-path integration; LOW (spike required) for PDFium Docker bundling

## Summary

Phase 1 replaces Teun's two expensive answer paths (`run_claude` CLI subprocess and `run_inline` whole-corpus dump) with a real retrieval pipeline: PDFs are ingested (extract → line-number → chunk → embed → store in pgvector), and each query embeds → top-K cosine search → single Anthropic Messages call producing the same two-phase `MortgageAnswer`. Every core dependency already fits the existing stack: `pgvector` rides on the current `sqlx 0.8 + Postgres`, embeddings go through the existing `reqwest` client, and the answer path reuses `inline.rs`'s proven SSE streaming + two-phase parser almost verbatim.

The four verified building blocks are: `pdfium-render 0.9.3` (per-page text via `page.text()?.all()`), `pgvector 0.4.2` (crate feature `sqlx`, supports sqlx ≥ 0.8, `Vector` type binds/decodes directly, cosine via the `<=>` operator), `tiktoken-rs 0.12.0` + `text-splitter 0.32.0` (token-aware chunking with byte-offset indices to recover line/page metadata), and OpenAI's `text-embedding-3-small` (1536 dims, batched `input` array). Migrations follow the existing embedded-`sqlx::migrate!("./migrations")` numbered-SQL pattern — add `003_*.sql` creating the `vector` extension plus `documents`/`chunks` tables.

**Primary recommendation:** Add a new `agent/rag.rs` that mirrors `inline.rs` (reuse its streaming loop + `parse_two_phase_response`), build the system prompt from top-K retrieved chunks formatted with the existing `"<line>: text"` convention, populate `ToolEvidence` from the retrieved chunks' line ranges so the judge/verifier narrow-search path keeps working, delete `claude.rs` wholesale, and repoint both `mode=tools`/`mode=inline` at `run_rag`. The one genuine unknown is bundling the PDFium native library into the `debian:bookworm-slim` runtime image — spike this first.

<user_constraints>
## User Constraints (from PROJECT.md / TEUN_MAP.md — locked 2026-07-17)

### Locked Decisions (do NOT reconsider)
- **Vector store:** pgvector — reuse existing sqlx + Postgres, no new external service.
- **Embeddings:** OpenAI `text-embedding-3-small` via `reqwest` (Azure OpenAI acceptable variant).
- **PDF extraction:** `pdfium-render` — bundle the PDFium native library into the Docker image.
- **Citations:** line-based over extracted text. On ingest, convert each PDF → line-numbered canonical text body used by `verifier.rs` + DocumentViewer; chunks additionally carry the source `page`.
- **PDF byte storage:** Postgres `bytea` (no Azure blob).
- **`mode` field:** repoint `tools`/`inline` → the single RAG answer path (field kept only for frontend compatibility).
- **Single RAG answer path** replaces both `run_claude` (subprocess) and `run_inline` (whole-corpus dump).
- **Interface compatibility (HARD):** SSE `ChatEvent` contract + `MortgageAnswer`/`SourceReference` shapes + existing REST endpoints MUST stay backward-compatible. Web frontend, judge, eval runner, and sessions depend on them.

### Claude's Discretion
- Chunk size / overlap and tokenizer choice for Dutch policy prose.
- Top-K value and similarity threshold.
- Index choice (HNSW / IVFFlat / none) given the small corpus.
- Internal module layout of the new retrieval/ingestion code.
- Whether the ingest step is a startup routine, a separate binary, or triggered by the (Phase 2) upload endpoint — Phase 1 only needs the seed corpus ingested.

### Deferred / Out of Scope (ignore for Phase 1)
- Web scraping / BFS crawler (Teun's corpus is admin-uploaded static PDFs).
- Tiered pdf/table indexes + stub-discovery pattern (CC-specific; Teun uses one flat chunk index).
- Azure AI Search / Azure Blob Storage.
- Non-PDF upload formats (docx, xlsx).
- Auth/authorization rework.
- LLM-generated stub descriptions.
- **Admin web upload/list/delete UI → Phase 2.**
- **`verifier.rs` rewiring to PDF-extracted text + DocumentViewer for PDFs → Phase 3** (see Open Questions — this creates a Phase-1 judge gap that must be acknowledged).
</user_constraints>

<phase_requirements>
## Phase Requirements

> Exact `REQUIREMENTS.md` text was not available at research time; descriptions below are inferred from ROADMAP.md Phase 1 + PROJECT.md Active. Planner should reconcile IDs against REQUIREMENTS.md.

| ID | Inferred Description | Research Support |
|----|-------------|------------------|
| RET-01 | pgvector store + schema (`documents` + `chunks` with embedding vectors) | § pgvector + sqlx; § Migrations; migration `003_*.sql` sketch |
| RET-02 | OpenAI `text-embedding-3-small` embedding client via reqwest | § OpenAI Embeddings; request/response + env config |
| RET-03 | Chunker producing chunks tagged with `{document, line_start, line_end, page}` | § Chunking in Rust; byte-offset → line/page mapping |
| RET-04 | Retriever: embed query → top-K cosine similarity search | § pgvector query (`<=>`); top-K SQL |
| RET-05 | New single-call Anthropic answer path → `MortgageAnswer` | § Answer Path Integration; reuse `inline.rs` |
| RET-06 | `mode` field repointed to single RAG path (interface preserved) | § Answer Path Integration; `chat.rs` dispatch |
| ING-01 | pdfium-render extraction → line-numbered canonical body + page map | § pdfium-render; canonical body builder |
| ING-02 | Ingest pipeline extract → chunk → embed → store (mirrors CC `_process_document`) | § Architecture Patterns; CC reference |
| ING-03 | Store original PDF bytes in Postgres `bytea` | § Migrations; `documents.original_bytes bytea` |
| ING-04 | Ingest existing `resources/acceptatie/` PDFs (seed corpus) | § Environment Availability; 5 seed PDFs present |
</phase_requirements>

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| PDF text extraction | API/Backend (Rust, ingest) | — | Native PDFium binding runs server-side only |
| Line-numbered canonical body + page map | API/Backend (ingest) | Database (stored `extracted_text`) | Produced once at ingest; persisted for retrieval + citation |
| Chunking + metadata tagging | API/Backend (ingest) | — | Pure CPU transform over extracted text |
| Embedding generation | API/Backend → OpenAI API | External (OpenAI) | Cross-provider REST call via existing reqwest client |
| Vector storage + similarity search | Database (Postgres + pgvector) | API/Backend (query builder) | pgvector owns ANN; backend issues `<=>` query |
| Answer generation | API/Backend → Anthropic API | External (Anthropic) | Single Messages call; prompt built from retrieved chunks |
| SSE streaming + two-phase parse | API/Backend (`agent/rag.rs`) | Browser (existing frontend, unchanged) | Contract preserved; frontend untouched |
| Citation verification | API/Backend (`judge/verifier.rs`) | Database (canonical text) | **Currently reads disk; Phase 3 rewires to DB — Phase 1 gap** |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `pdfium-render` | 0.9.3 | PDF text extraction (idiomatic Rust wrapper over Google's PDFium C++ lib) | `[VERIFIED: crates.io 2026-07-14]` The de-facto high-fidelity Rust PDF library; locked by PROJECT.md |
| `pgvector` | 0.4.2 | `Vector` type + sqlx encode/decode for Postgres vector columns | `[VERIFIED: crates.io 2026-05-22]` Official `pgvector/pgvector-rust`; v0.4 supports sqlx ≥ 0.8 |
| `tiktoken-rs` | 0.12.0 | Token counting with OpenAI BPE (`cl100k_base`) | `[VERIFIED: crates.io 2026-06-02]` ~12M downloads; the maintained Rust tiktoken port |
| `text-splitter` | 0.32.0 | Token-aware chunking with byte-offset indices | `[VERIFIED: crates.io 2026-06-16]` Integrates tiktoken-rs; returns char/byte indices to recover line metadata |

### Supporting (already in Cargo.toml — reuse)
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `sqlx` | 0.8 (postgres, chrono, json, uuid, tls-rustls) | DB access + embedded migrations | Store/query chunks; `pgvector` plugs into this |
| `reqwest` | 0.12 (json, stream) | OpenAI embeddings call + Anthropic streaming | Reuse `state.http_client`; build a no-total-timeout client for streaming (as `inline.rs` does) |
| `futures` / `tokio-stream` | 0.3 / 0.1 | SSE stream processing | Already used by `inline.rs` streaming loop |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `text-splitter` | Hand-rolled sliding-window over tokens | text-splitter handles UTF-8/semantic boundaries + gives byte indices for free — see Don't Hand-Roll |
| `pgvector` HNSW index | No index (flat scan) | At ~thousands of chunks a table scan is faster than building an index (pgvector README) — recommend **no index** for Phase 1 |
| `pdfium-render` dynamic load | `static` feature (compile PDFium in) | Static avoids shipping `.so` but requires sourcing a static archive + `PDFIUM_STATIC_LIB_PATH`; dynamic is simpler for the seed corpus |
| OpenAI direct | Azure OpenAI | Azure needs `endpoint/openai/deployments/{name}/embeddings?api-version=...` + `api-key` header; CC uses Azure. Support via env branch. |

**Installation (add to `apps/teun/service/Cargo.toml`):**
```toml
pdfium-render = "0.9"
pgvector = { version = "0.4", features = ["sqlx"] }
tiktoken-rs = "0.12"
text-splitter = { version = "0.32", features = ["tiktoken-rs"] }
```

**Version verification (run at implementation time — versions above confirmed 2026-07-17):**
```bash
cargo search pdfium-render pgvector tiktoken-rs text-splitter
cargo add pgvector --features sqlx --dry-run   # confirm sqlx 0.8 compatibility resolves
```

## Package Legitimacy Audit

> slopcheck is a Python/npm tool; these are Rust crates verified directly against crates.io + official GitHub orgs. All four are long-established, high-download, source-backed crates — none newly published, none typosquat candidates.

| Package | Registry | Age / Signal | Source Repo | Verdict | Disposition |
|---------|----------|--------------|-------------|---------|-------------|
| `pdfium-render` | crates.io | Mature, actively maintained (updated 2026-07-14) | github.com/ajrcarey/pdfium-render | OK | Approved |
| `pgvector` | crates.io | Official pgvector org; updated 2026-05-22 | github.com/pgvector/pgvector-rust | OK | Approved |
| `tiktoken-rs` | crates.io | ~12M downloads; updated 2026-06-02 | github.com/zurawiki/tiktoken-rs | OK | Approved |
| `text-splitter` | crates.io | Widely used; updated 2026-06-16 | github.com/benbrandt/text-splitter | OK | Approved |

**Packages removed due to [SLOP]:** none
**Packages flagged [SUS]:** none
**Native dependency (not a crate):** PDFium shared library from `github.com/bblanchon/pdfium-binaries` — verify the release tag/checksum at build time (see Common Pitfalls). This is trusted-but-unpinned; pin a specific release.

## Architecture Patterns

### System Architecture Diagram

```
INGEST (once for seed corpus; Phase 2 wires the upload endpoint)
  resources/acceptatie/*.pdf
        │  read bytes
        ▼
  pdfium-render: document.pages().iter() → page.text()?.all()
        │  per-page text
        ▼
  Canonical body builder ─────────────► line-numbered text + Vec<page per line>
        │ (join pages; track cumulative line numbers → page map)
        ▼
  text-splitter (tiktoken cl100k_base) → chunks + byte-offset indices
        │  map byte offset → line_start/line_end → page
        ▼
  OpenAI /v1/embeddings (batched input[])  ──► Vec<[f32;1536]>
        │
        ▼
  Postgres:  documents(bytea, extracted_text, page_count, status, chunk_count)
             chunks(document_id, content, line_start, line_end, page, embedding vector(1536))

QUERY (per chat turn — replaces run_claude & run_inline)
  POST /api/teun/chat {message, mode, ...}
        │
        ▼
  embed(query) → [f32;1536]
        │
        ▼
  SELECT ... ORDER BY embedding <=> $1 LIMIT K   (cosine)
        │  top-K chunks {content, document, line_start, line_end, page}
        ▼
  build system prompt = skill + chunks formatted "<line>: text" (+ doc/page)
        │
        ▼
  Anthropic /v1/messages (stream=true)  ── two-phase: answer \n---JSON---\n {rationale,sources,category}
        │  reuse inline.rs streaming loop + parse_two_phase_response
        ▼
  ChatEvent::Partial* → ChatEvent::Result{MortgageAnswer}   (SSE, unchanged)
        │
        ▼
  judge::run_judge(answer, ToolEvidence built from retrieved chunk ranges)
        └─► ChatEvent::Judge   (verifier: see Phase-1 gap)
```

### Recommended Project Structure
```
apps/teun/service/src/
├── agent/
│   ├── rag.rs          # NEW: run_rag() — retrieval + single Anthropic call + two-phase stream
│   ├── inline.rs       # keep parse_two_phase_response (extract to shared) + streaming helper; drop whole-corpus loader
│   ├── claude.rs       # DELETE (subprocess path removed); relocate find_project_root
│   └── types.rs        # KEEP unchanged (MortgageAnswer/SourceReference/ChatEvent/ToolEvidence)
├── rag/                # NEW module
│   ├── mod.rs
│   ├── extract.rs      # pdfium → per-page text → line-numbered canonical body + page map
│   ├── chunk.rs        # text-splitter + tiktoken; attach {line_start,line_end,page}
│   ├── embed.rs        # OpenAI (+Azure) embeddings client over reqwest
│   ├── store.rs        # documents/chunks CRUD; top-K cosine query (sqlx + pgvector)
│   └── ingest.rs       # orchestrates extract→chunk→embed→store (mirrors CC _process_document)
└── migrations/
    └── 003_rag.sql     # CREATE EXTENSION vector; documents + chunks tables
```

### Pattern 1: Line-numbered canonical body + page map (the citation bridge)
**What:** Concatenate per-page extracted text into one canonical body whose 1-indexed lines match what `verifier.rs` (`content.lines()`) and DocumentViewer expect; record which page each line came from.
**When to use:** Every ingested PDF.
**Example:**
```rust
// Source: derived from pdfium-render README API + Teun verifier.rs line model
struct Canonical { text: String, page_of_line: Vec<u32> } // page_of_line[i] = source page of line i (0-indexed)

fn build_canonical(doc: &PdfDocument) -> anyhow::Result<Canonical> {
    let mut text = String::new();
    let mut page_of_line = Vec::new();
    for (pidx, page) in doc.pages().iter().enumerate() {
        let page_text = page.text()?.all();          // PdfPageText::all()
        for line in page_text.lines() {
            text.push_str(line);
            text.push('\n');
            page_of_line.push(pidx as u32 + 1);       // 1-indexed page
        }
    }
    Ok(Canonical { text, page_of_line })
}
```

### Pattern 2: Byte-offset → line/page during chunking
**What:** `text-splitter` yields each chunk with its byte offset into the canonical body; convert that to line numbers (count `\n` up to the offset) and read the page from `page_of_line`.
**Example:**
```rust
// Source: text-splitter chunk_indices() API (byte offset + chunk str)
use text_splitter::{TextSplitter, ChunkConfig};
let splitter = TextSplitter::new(ChunkConfig::new(700).with_overlap(120)?.with_sizer(tokenizer));
for (byte_off, chunk) in splitter.chunk_indices(&canonical.text) {
    let line_start = canonical.text[..byte_off].bytes().filter(|&b| b == b'\n').count() + 1;
    let line_end = line_start + chunk.bytes().filter(|&b| b == b'\n').count();
    let page = canonical.page_of_line[line_start - 1];
    // → store {content: chunk, line_start, line_end, page, embedding}
}
```
> Confirm the exact `text-splitter 0.32` API name (`chunk_indices` vs `chunks_with_indices`) and `ChunkConfig`/sizer signature against docs.rs at implementation — MEDIUM confidence on exact method names.

### Pattern 3: Reuse the two-phase streaming answer path
**What:** `run_rag` is structurally `run_inline` with the system prompt built from retrieved chunks instead of the whole corpus. The entire SSE `bytes_stream` loop, the `\n---JSON---` separator handling, char-boundary safe slicing, and `parse_two_phase_response` are reused unchanged.
**When to use:** The single answer path for both modes.

### Anti-Patterns to Avoid
- **Re-implementing SSE parsing / two-phase split** — copy `inline.rs`'s battle-tested loop; do not rewrite.
- **Storing raw per-page text without a stable line model** — the entire citation/verifier/viewer chain is line-number based; the canonical body IS the contract.
- **Using `sqlx::query!` compile-time macros** — Teun uses runtime `sqlx::query()` (no build-time DB). Keep that; `query!` would require `DATABASE_URL`/offline data at build (breaks the Docker builder stage).
- **Building an ANN index prematurely** — at this corpus size a flat scan is faster (pgvector README).

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Token-aware chunking | Manual token windowing | `text-splitter` + `tiktoken-rs` | Handles UTF-8 boundaries, overlap, semantic splits, and returns byte indices |
| Token counting | Char/word heuristics | `tiktoken-rs` `cl100k_base` | `text-embedding-3-small` uses cl100k BPE; heuristics drift from real token limits |
| Vector encode/decode for Postgres | Serialize floats to text | `pgvector::Vector` + `sqlx` feature | Correct binary wire format, `<=>`/`<->`/`<#>` operators, no manual parsing |
| PDF text extraction | Byte-level PDF parsing | `pdfium-render` | PDF is a layout format; text order, encodings, and fonts are non-trivial (locked decision anyway) |
| Similarity ranking | Cosine in Rust over all rows | pgvector `ORDER BY embedding <=> $1 LIMIT K` | Push the ANN/scan to the DB; single round-trip |

**Key insight:** Every "simple" piece here (tokenization, vector wire format, PDF text order) has deep edge cases; the ecosystem crates are mature and directly supported.

## Runtime State Inventory

> This phase is a **rebuild that removes** the two answer paths and changes the Docker image, so runtime-state impact is documented.

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data | `sessions`, `messages`, `test_questions`, `eval_runs`, `eval_results`, `settings` tables — all **unaffected** (chat contract unchanged). New: `documents`, `chunks` tables added by migration `003`. | Add migration only; no data migration of existing tables. |
| Live service config | None external to Postgres. Langfuse spans continue (rename the agent span from `ai.claude_cli`/`ai.anthropic_messages` to a RAG span; cost now on the embeddings + single Messages call). | Update tracing span names/fields in new `rag.rs`. |
| OS-registered state | **claude CLI** installed globally in the Docker image (`npm install -g @anthropic-ai/claude-code`) and the `~/.claude/.credentials.json` mount — **no longer needed** once `run_claude` is removed. | Remove Node.js + claude CLI install from Dockerfile; remove OAuth-token fallback in config. Add PDFium `.so`. |
| Secrets / env vars | `JUDGE_API_KEY`/`ANTHROPIC_API_KEY` (kept, used for answer + judge). **New:** `OPENAI_API_KEY` (+ optional `AZURE_OPENAI_*`), `EMBEDDING_MODEL`. `DATABASE_URL` unchanged. | Add embedding env vars; document in `.env.example`. Code reads OAuth token from disk — can be dropped with claude CLI. |
| Build artifacts | Docker runtime image (`debian:bookworm-slim`) currently ships Node + claude CLI; must instead ship `libpdfium.so` (+ likely `libstdc++6`). | Rework Dockerfile runtime stage (see Common Pitfalls / spike). |

**Verified explicitly:** No ChromaDB/Mem0/n8n/Redis/Task-Scheduler state — Teun's only datastore is Postgres via sqlx (confirmed by full source read). The `.md` files in `resources/acceptatie/` remain on disk and are still readable by the existing verifier for `.md`-named citations.

## Common Pitfalls

### Pitfall 1: PDFium native library not found at runtime (the primary spike)
**What goes wrong:** `Pdfium::bind_to_system_library()` fails in `debian:bookworm-slim` because no `libpdfium.so` is installed; or the `.so` loads but fails on a missing `libstdc++6`.
**Why it happens:** `pdfium-render` does **not** build or bundle PDFium — you must ship the native lib yourself. Slim Debian images omit `libstdc++6`.
**How to avoid:** In the Docker runtime stage, download a pinned release from `github.com/bblanchon/pdfium-binaries` (e.g. `pdfium-linux-x64.tgz`), extract `lib/libpdfium.so` to `/app`, and load with:
```rust
let pdfium = Pdfium::new(
    Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("/app"))
        .or_else(|_| Pdfium::bind_to_system_library())?,
);
```
Add `apt-get install -y libstdc++6` (and verify `libgcc-s1`). **Pin the pdfium-binaries release tag and verify its checksum** — it is an unversioned external binary. Confirm the chosen PDFium build is compatible with `pdfium-render 0.9.3` (the crate documents a compatible PDFium version range).
**Warning signs:** `PdfiumError::LoadLibraryError` at startup, or `symbol lookup error` referencing `libstdc++`.
**Confidence:** LOW — **spike before planning tasks depend on it.** Validate a local `docker build` + ingest of one seed PDF end-to-end.

### Pitfall 2: PDF text extraction quality on Dutch policy prose
**What goes wrong:** `page.text().all()` may merge columns, reorder text, or split sentences oddly; line breaks reflect layout, not logical lines. This directly affects citation line ranges.
**Why it happens:** PDF has no notion of "lines"; extraction infers them from glyph positions.
**How to avoid:** Spike extraction on all 5 seed PDFs; eyeball the canonical body. If quality is poor, PROJECT.md already ships parallel `.md` versions of the same docs — a fallback is to chunk/cite the `.md` bodies while still storing PDF bytes (keeps the verifier's existing disk path working). Decide during the spike.
**Warning signs:** Garbled canonical text; quotes that never fuzzy-match in the verifier.
**Confidence:** MEDIUM.

### Pitfall 3: The verifier still reads from disk — PDF citations won't verify in Phase 1
**What goes wrong:** `judge/verifier.rs::check_source` does `std::fs::read_to_string("{resources_dir}/{document}")`. If the answer cites a PDF filename, the verifier reads PDF **bytes as text** → quote mismatch → sources marked unreliable.
**Why it happens:** Citation-integrity rewiring is scoped to **Phase 3** (CIT-01), not Phase 1.
**How to avoid (Phase 1 options — pick one; see Open Questions):**
- **(A) Recommended:** In `run_rag`, set each `SourceReference.line_range` directly from the retrieved chunk's stored `line_start/line_end`, and build `ToolEvidence` from those ranges so `verify_sources` uses its evidence-narrowed path. Still needs the doc text to confirm the quote → also persist canonical text and give the verifier a DB read (small, contained change) OR accept LLM-faithfulness-only judging for PDF sources this phase.
- **(B) Minimal:** Chunk/cite the existing `.md` bodies (whose filenames the verifier can already read from disk) while storing PDF bytes + page metadata — defers all verifier changes to Phase 3.
**Warning signs:** `SourceStatus::QuoteMismatch` on every PDF-sourced citation.
**Confidence:** MEDIUM (codebase-verified behavior).

### Pitfall 4: `text-embedding-3-small` dimension / batch limits
**What goes wrong:** Column declared `vector(N)` with the wrong N; or a batch exceeds OpenAI's per-request token cap.
**How to avoid:** `text-embedding-3-small` returns **1536** dims by default → `embedding vector(1536)`. Batch the `input` array but cap batch token totals (OpenAI limit ~300k tokens / ≤2048 inputs per request); chunk the batches accordingly. `[CITED: platform.openai.com/docs/guides/embeddings]`
**Confidence:** HIGH.

### Pitfall 5: pgvector extension must exist before any vector query
**What goes wrong:** `type "vector" does not exist` if the extension isn't created first, or Postgres image lacks pgvector.
**How to avoid:** `CREATE EXTENSION IF NOT EXISTS vector;` as the first statement of migration `003` (runs before table creation; `sqlx::migrate!` runs files in order). **Verify the Postgres deployment has the pgvector extension available** (needs the `pgvector` server extension installed — e.g. `pgvector/pgvector:pg16` image or a managed PG with pgvector enabled). `CREATE EXTENSION` requires sufficient privileges.
**Confidence:** HIGH (mechanism); MEDIUM (deployment has pgvector — verify).

## Code Examples

### OpenAI embeddings request/response
```jsonc
// Source: platform.openai.com/docs/api-reference/embeddings  [CITED]
// POST https://api.openai.com/v1/embeddings
// Headers: Authorization: Bearer $OPENAI_API_KEY ; Content-Type: application/json
{ "model": "text-embedding-3-small", "input": ["chunk 1 text", "chunk 2 text"] }
// Response:
{ "data": [ { "index": 0, "embedding": [0.0012, ...1536 floats] }, { "index": 1, "embedding": [...] } ],
  "model": "text-embedding-3-small", "usage": { "prompt_tokens": 42, "total_tokens": 42 } }
```
Azure variant `[CITED: learn.microsoft.com/azure/ai-services/openai]`:
`POST {AZURE_OPENAI_ENDPOINT}/openai/deployments/{deployment}/embeddings?api-version=2024-02-01`, header `api-key: {key}`, same body without `model`.

### pgvector + sqlx: store and top-K cosine query
```rust
// Source: pgvector-rust README (sqlx feature)  [CITED: github.com/pgvector/pgvector-rust]
use pgvector::Vector;

// INSERT a chunk embedding
sqlx::query("INSERT INTO chunks (id, document_id, content, line_start, line_end, page, embedding)
             VALUES ($1,$2,$3,$4,$5,$6,$7)")
    .bind(&id).bind(doc_id).bind(&content)
    .bind(line_start).bind(line_end).bind(page)
    .bind(Vector::from(embedding_vec))     // Vec<f32> -> Vector
    .execute(&pool).await?;

// Top-K similarity (cosine distance <=>)
let q = Vector::from(query_embedding);
let rows = sqlx::query(
    "SELECT c.content, d.filename AS document, c.line_start, c.line_end, c.page
     FROM chunks c JOIN documents d ON d.id = c.document_id
     ORDER BY c.embedding <=> $1 LIMIT $2")
    .bind(&q).bind(k)
    .fetch_all(&pool).await?;
let embedding: Vector = rows[0].try_get("embedding")?;  // decode when needed
```

### Migration 003 (sketch)
```sql
-- migrations/003_rag.sql
CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE IF NOT EXISTS documents (
    id             VARCHAR(36) PRIMARY KEY,
    filename       VARCHAR(255) NOT NULL,
    original_bytes BYTEA NOT NULL,
    extracted_text TEXT NOT NULL,          -- line-numbered canonical body (the citation source of truth)
    page_count     INTEGER NOT NULL DEFAULT 0,
    status         VARCHAR(20) NOT NULL DEFAULT 'pending',  -- pending|indexing|indexed|error
    chunk_count    INTEGER NOT NULL DEFAULT 0,
    error_message  TEXT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    indexed_at     TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS chunks (
    id          VARCHAR(36) PRIMARY KEY,
    document_id VARCHAR(36) NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    content     TEXT NOT NULL,
    line_start  INTEGER NOT NULL,
    line_end    INTEGER NOT NULL,
    page        INTEGER NOT NULL,
    embedding   VECTOR(1536) NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_chunks_document_id ON chunks (document_id);
-- No ANN index for Phase 1: flat scan is faster at this corpus size (pgvector README).
```

## State of the Art

| Old Approach (current Teun) | Current Approach (this phase) | Impact |
|--------------|------------------|--------|
| `mode=tools`: spawn `claude` CLI subprocess + Grep/Read live | Embed query → pgvector top-K → single Messages call | Removes subprocess, Node, CLI install; drastic cost cut |
| `mode=inline`: dump ~100K-token corpus into system prompt every request | Prompt = top-K retrieved chunks (~2–8K tokens) | ~10–50× smaller input per request |
| Citations resolved via live tool-read line numbers | Citations = chunk-attached `line_start/line_end` over canonical body | Deterministic; no agent needed |

**Deprecated/removed:** `agent/claude.rs` (whole file), the whole-corpus loader in `inline.rs`, the OAuth-token-from-disk fallback, Node.js + `@anthropic-ai/claude-code` in the Docker image.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `text-splitter 0.32` exposes chunk byte offsets via `chunk_indices()` and `ChunkConfig::with_overlap/with_sizer` | Chunking patterns | Method names differ → chunk metadata mapping needs a different API call (verify docs.rs) |
| A2 | Chunk size 700 tokens / overlap 120 suits Dutch policy prose | Chunking | Retrieval quality; tunable, low structural risk |
| A3 | `pdfium-render 0.9.3` API is `document.pages().iter()` + `page.text()?.all()` with `.lines()` iteration | Extraction | Exact iterator/borrow signatures may differ; verify on docs.rs |
| A4 | PDFium binaries from bblanchon are compatible with pdfium-render 0.9.3 and load on bookworm-slim with `libstdc++6` | Docker pitfall | **Build/runtime failure — spike required** |
| A5 | The target Postgres deployment has the pgvector server extension installed/available | Migrations pitfall | `CREATE EXTENSION` fails → need pgvector-enabled PG image |
| A6 | `text-embedding-3-small` default output dimension is 1536 | Embeddings | Wrong `vector(N)` → insert errors (well-documented, low risk) |
| A7 | Reusing `inline.rs` streaming + `parse_two_phase_response` keeps the SSE contract byte-identical | Answer path | Frontend/eval regressions if drift; mitigated by reusing code verbatim |
| A8 | Phase 1 may leave PDF-source citation verification degraded (deferred to Phase 3) without violating success criteria | Verifier gap | If Phase 1 must show verified sources, needs the (A) DB-read verifier change now |

## Open Questions

1. **Verifier bridge for PDF sources in Phase 1 (highest priority).**
   - Known: `verifier.rs` reads doc text from `resources_dir` on disk; canonical PDF text will live in `documents.extracted_text`. Phase 3 owns the rewire.
   - Unclear: Does Phase 1 need verified PDF citations, or is LLM-faithfulness-only judging acceptable this phase?
   - Recommendation: Go with option (A) — set `line_range` from the retrieved chunk and build `ToolEvidence` from chunk ranges; add a small verifier DB-text read. Confirm scope with planner/discuss-phase.

2. **PDF extraction quality gate.**
   - Known: pdfium is high-fidelity but PDF text order/lines are inferred.
   - Recommendation: Spike all 5 seed PDFs; if poor, fall back to chunking the existing `.md` bodies (which also keeps the disk-based verifier working) while still storing PDF bytes + page map. Decide before committing the chunker design.

3. **Ingestion trigger for the seed corpus.**
   - Known: Phase 2 owns the upload endpoint.
   - Recommendation: A one-shot ingest (startup routine gated by an env flag, or a small `cargo run --bin ingest`) that upserts the 5 PDFs; idempotent by filename/content hash so re-runs don't duplicate chunks.

4. **Duplicate seed docs.** `handboek_accept_versie_2026_4.pdf`, `handboek_acceptatie_versie_2026_4_definitief.pdf`, and `MUNT_Handboek_acceptatie_versie_2026_4.md` overlap; PROJECT.md says `_definitief` is final. Recommendation: ingest only `_definitief` to avoid near-duplicate chunks polluting top-K.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust toolchain | Build | ✓ | 1.92 (workspace `rust-version`) | — |
| Postgres | Store | ✓ (existing `DATABASE_URL`) | — | — |
| **pgvector server extension** | Vector column/query | ✗ (unverified) | — | Use `pgvector/pgvector:pgNN` image or enable on managed PG — **blocking if absent** |
| **PDFium native lib** (`libpdfium.so`) | pdfium-render at runtime | ✗ (must bundle) | bblanchon release (pin) | `static` feature (needs prebuilt archive) — **blocking if absent** |
| `libstdc++6` in runtime image | PDFium `.so` load | ✗ (slim image) | — | `apt-get install libstdc++6` |
| OpenAI API key | Embeddings | ✗ (new env var) | — | Azure OpenAI variant |
| Anthropic API key | Answer + judge | ✓ (`ANTHROPIC_API_KEY`/`JUDGE_API_KEY`) | — | — |
| Seed PDFs | ING-04 | ✓ | 5 PDFs in `resources/acceptatie/` | — |
| `protobuf-compiler` | Build (existing OTLP) | ✓ (in builder image) | — | — |

**Missing with no fallback (must resolve in planning):** pgvector server extension; PDFium native library bundling.
**Missing with fallback:** OpenAI key (Azure); `libstdc++6` (apt install).

## Validation Architecture

> `.planning/config.json` was not present at research time; treating `nyquist_validation` as enabled (default).

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[cfg(test)]` / `#[test]` (unit) — pattern already used in `verifier.rs`, `documents.rs` |
| Config file | none — cargo default |
| Quick run command | `cargo test --package teun <module>::` |
| Full suite command | `cargo test --package teun` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| ING-01 | Byte offset → line/page mapping is correct | unit | `cargo test --package teun rag::extract` | ❌ Wave 0 |
| RET-03 | Chunk carries correct line_start/line_end/page | unit | `cargo test --package teun rag::chunk` | ❌ Wave 0 |
| RET-02 | Embedding request/response parse (mocked JSON) | unit | `cargo test --package teun rag::embed` | ❌ Wave 0 |
| RET-04 | Top-K query returns rows ordered by cosine | integration (needs PG+pgvector) | `cargo test --package teun rag::store -- --ignored` | ❌ Wave 0 |
| RET-05 | `parse_two_phase_response` unchanged output shape | unit | `cargo test --package teun agent::` | partial (exists in inline.rs) |
| ING-01 quality | Seed PDF extracts non-empty canonical body | integration/manual | manual spike + `--ignored` test | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test --package teun <touched module>` + `cargo build --release`
- **Per wave merge:** `cargo test --package teun`
- **Phase gate:** Full suite green + a manual end-to-end (ingest one seed PDF, ask one question, inspect `MortgageAnswer` sources) before `/gsd:verify-work`.

### Wave 0 Gaps
- [ ] `rag/extract.rs` tests — byte/line/page mapping (ING-01)
- [ ] `rag/chunk.rs` tests — metadata attachment (RET-03)
- [ ] `rag/embed.rs` tests — mocked OpenAI response decode (RET-02)
- [ ] `rag/store.rs` `--ignored` integration test — requires a pgvector-enabled test Postgres (RET-04)
- [ ] Fixture: one small text-only PDF for extraction tests
- [ ] Decision: how integration tests get a pgvector Postgres in CI (docker service)

## Security Domain

> `security_enforcement` config not found; treating as enabled. Phase 1 is backend-only (upload UI is Phase 2), so the surface is narrow.

### Applicable ASVS Categories
| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no (Phase 2 for upload) | reuse existing portal auth |
| V4 Access Control | partial | ingest is server-side/admin-triggered only in Phase 1 |
| V5 Input Validation | yes | validate PDF bytes before extraction; keep `document`-name path-traversal guards (already in `verifier.rs`/`documents.rs`) |
| V6 Cryptography | no | no new crypto; TLS via existing rustls |
| V12 Files/Resources | yes | cap PDF size (CC uses 50 MB); guard against huge/zip-bomb PDFs; `max_pages` cap like CC's extractor |
| — Secrets | yes | new `OPENAI_API_KEY` from env only; never log key or full embeddings |

### Known Threat Patterns for Rust/axum/sqlx/pgvector
| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| SQL injection via chunk/query text | Tampering | Parameterized `sqlx::query().bind()` only (already the codebase norm) |
| Path traversal via cited `document` name | Tampering | Keep existing `..`/`/`/`\\` rejection in verifier + document routes |
| Malicious/oversized PDF (DoS, parser exploit) | DoS | Size + page caps before extraction; treat pdfium errors gracefully (don't crash the request) |
| Prompt injection from PDF content into the answer LLM | Tampering | Retrieved chunks are untrusted text; keep skill instructions authoritative; the judge/verifier remain the guardrail |
| Secret leakage in logs/traces | Info disclosure | Log lengths/counts, not keys or embedding vectors (matches existing `inline.rs` logging discipline) |

## Sources

### Primary (HIGH confidence)
- crates.io API — `pdfium-render` 0.9.3 (2026-07-14), `pgvector` 0.4.2 (2026-05-22), `tiktoken-rs` 0.12.0 (2026-06-02), `text-splitter` 0.32.0 (2026-06-16)
- github.com/pgvector/pgvector-rust README — sqlx feature, `Vector` bind/decode
- github.com/pgvector/pgvector README — `<=>`/`<->`/`<#>` operators; index vs flat-scan guidance
- github.com/ajrcarey/pdfium-render README — `bind_to_library`/`bind_to_system_library`, `page.text().all()`, bblanchon binaries
- Teun codebase (read in full for this phase): `main.rs`, `agent/{inline,claude,types}.rs`, `routes/{chat,documents}.rs`, `judge/{mod,verifier}.rs`, `session/store.rs`, `migrations/00{1,2}_*.sql`, `Dockerfile`, `eval/runner.rs`
- CC reference: `infrastructure/pdf/extractor.py`, `application/services/ingestion_service.py::_process_document`

### Secondary (MEDIUM confidence)
- OpenAI Embeddings API shape (endpoint/dims/batching) — training + docs `[CITED]`, not re-fetched this session
- Azure OpenAI embeddings endpoint shape — training + docs `[CITED]`

### Tertiary (LOW confidence — validate)
- Exact `text-splitter 0.32` method names (`chunk_indices`, `ChunkConfig` builders) — confirm on docs.rs
- pdfium-render ↔ bblanchon PDFium version compatibility + bookworm-slim runtime deps — **spike**

## Metadata

**Confidence breakdown:**
- Standard stack (versions, crate choice): HIGH — crates.io verified 2026-07-17
- pgvector + sqlx API: HIGH — official README
- pdfium text API: HIGH — official README; exact borrow signatures MEDIUM
- Chunking metadata mapping: MEDIUM — approach sound, exact text-splitter API to confirm
- Answer-path integration: MEDIUM-HIGH — grounded in full read of `inline.rs`/`chat.rs`
- PDFium Docker bundling: LOW — **spike required**
- Verifier/citation Phase-1 bridge: MEDIUM — codebase-verified behavior, scoping decision open

**Research date:** 2026-07-17
**Valid until:** ~2026-08-16 (crate versions move fast; re-verify pdfium-render + text-splitter before implementation)

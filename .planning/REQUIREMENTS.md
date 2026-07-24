# Requirements: Teun RAG Backend Rebuild

**Defined:** 2026-07-17
**Core Value:** Teun answers a mortgage-acceptance question with a grounded, source-cited answer at a fraction of the current per-request cost, by retrieving only relevant policy passages instead of feeding the whole corpus per request — citations still verifiable against source documents.

## v1 Requirements

### Retrieval (RAG core)

- [ ] **RET-01**: pgvector extension enabled and schema created (documents + chunks-with-embedding tables) via sqlx migration
- [ ] **RET-02**: Embedding client calls OpenAI `text-embedding-3-large` (batch + single query) over reqwest, keyed by env/App setting
- [x] **RET-03**: Chunker splits a document's text into chunks, each tagged with `document`, `line_start`, `line_end`, and source `page`
- [x] **RET-04**: Retriever embeds a user query and returns the top-K most similar chunks from pgvector
- [x] **RET-05**: New answer path builds the prompt from top-K retrieved chunks and makes a single Anthropic Messages call, emitting the existing two-phase `MortgageAnswer`
- [x] **RET-06**: `mode` field values (tools/inline) both route to the single RAG answer path; old `run_claude` subprocess and `run_inline` whole-corpus dump are removed

### Ingestion & PDF

- [ ] **ING-01**: pdfium-render extracts text from a PDF into a line-numbered canonical body plus a per-line→page map
- [x] **ING-02**: Ingest pipeline runs extract → chunk → embed → store for a single document, tracking status (pending/indexed/error) and chunk count
- [ ] **ING-03**: Original uploaded PDF bytes are stored in a Postgres `bytea` column
- [x] **ING-04**: The existing `resources/acceptatie/` PDFs can be ingested as the seed corpus (one-shot command or startup seed)

### Admin Document Management

- [ ] **ADM-01**: Admin can upload one or more PDFs via a web endpoint (multipart) with MIME-type and size validation, behind existing portal auth
- [ ] **ADM-02**: Admin can list ingested documents with their status, chunk count, and size (DB-backed)
- [ ] **ADM-03**: Admin can delete a document, removing its chunks, stored bytes, and metadata row
- [ ] **ADM-04**: Web admin has a document-manager tab (upload / list / delete / status), replacing the current static "Documentatie" help placeholder (help content preserved)
- [ ] **ADM-05**: Backend serves the original PDF bytes for a document (inline) so a citation `[N]`/source can open the source

### Citation Integrity (interface preservation)

- [ ] **CIT-01**: `verifier.rs` verifies each source's literal quote + line_range against the stored extracted text of PDF-sourced documents
- [ ] **CIT-02**: DocumentViewer renders a PDF-sourced document's extracted text with the correct line highlight for a citation
- [ ] **CIT-03**: SSE `ChatEvent` stream and `MortgageAnswer`/`SourceReference` shapes are unchanged; existing frontend, judge, eval, and sessions work without modification

## v2 Requirements

### Retrieval quality

- **RQ-01**: Section-aware chunking by markdown/PDF headings (v1 uses fixed-token windows)
- **RQ-02**: Hybrid keyword + vector search
- **RQ-03**: Re-ranking of retrieved chunks before prompt assembly
- **RQ-04**: Configurable per-`search_depth` top-K and judge-retry behavior

### Documents

- **DQ-01**: Non-PDF upload formats (docx, xlsx, html)
- **DQ-02**: Re-ingest / update an existing document in place

## Out of Scope

| Feature | Reason |
|---------|--------|
| Web scraping / BFS crawler | Teun's corpus is admin-uploaded static PDFs, not crawled sites |
| Tiered pdf/table indexes + stub discovery | CC-specific complexity; Teun uses one flat chunk index; policy docs are prose |
| Azure AI Search / Azure Blob | Replaced by pgvector + Postgres bytea to reuse existing stack |
| LLM-generated stub descriptions | Not needed without the tiered/stub architecture |
| Auth/authorization rework | Reuse existing portal auth gating admin routes |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| RET-01 | Phase 1 | Pending |
| RET-02 | Phase 1 | Pending |
| RET-03 | Phase 1 | Complete (01-03) |
| RET-04 | Phase 1 | Complete |
| RET-05 | Phase 1 | Complete |
| RET-06 | Phase 1 | Complete |
| ING-01 | Phase 1 | Pending |
| ING-02 | Phase 1 | Complete (01-03) |
| ING-03 | Phase 1 | Pending |
| ING-04 | Phase 1 | Complete (01-03) |
| ADM-01 | Phase 2 | Pending |
| ADM-02 | Phase 2 | Pending |
| ADM-03 | Phase 2 | Pending |
| ADM-04 | Phase 2 | Pending |
| ADM-05 | Phase 2 | Pending |
| CIT-01 | Phase 3 | Pending |
| CIT-02 | Phase 3 | Pending |
| CIT-03 | Phase 3 | Pending |

**Coverage:**
- v1 requirements: 18 total
- Mapped to phases: 18 ✓
- Unmapped: 0

---
*Requirements defined: 2026-07-17*
*Last updated: 2026-07-17 after roadmap creation*

# Roadmap: Teun — RAG Backend Rebuild

## Overview

Teun already ships an SSE chat backend, judge, verifier, sessions, eval runner, and React frontend — but it has no retrieval, so every answer either spawns a full `claude` CLI agent or dumps the entire ~100K-token corpus into the prompt. This milestone rebuilds the answer backend into a real RAG system while keeping the existing HTTP/SSE interface backward-compatible. We move goal-backward from the core value ("grounded, cited answers at a fraction of the per-request cost") through three end-to-end slices: (1) stand up the retrieval core and PDF ingestion so Teun answers from retrieved passages via a single cheap LLM call; (2) let admins manage the corpus through the web; (3) prove citation integrity and interface compatibility now that PDFs are the source of truth.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [ ] **Phase 1: RAG Retrieval Core & PDF Ingestion** - Replace the whole-corpus/subprocess answer paths with retrieval over ingested PDF passages via a single Anthropic call
- [ ] **Phase 2: Admin Document Management (Web)** - Admins upload, list, delete, and serve source PDFs through the web interface, behind portal auth
- [ ] **Phase 3: Citation Integrity & Interface Preservation** - Quote/line verification and DocumentViewer work against PDF-extracted text; SSE/answer contracts stay unchanged

## Phase Details

### Phase 1: RAG Retrieval Core & PDF Ingestion
**Goal**: Teun answers a mortgage-acceptance question from the top-K retrieved policy passages via a single Anthropic Messages call, replacing both the `run_claude` subprocess and the whole-corpus `run_inline` dump — cutting per-request cost while keeping the `MortgageAnswer` output shape.
**Depends on**: Nothing (first phase)
**Requirements**: RET-01, RET-02, RET-03, RET-04, RET-05, RET-06, ING-01, ING-02, ING-03, ING-04
**Success Criteria** (what must be TRUE):
  1. A PDF from `resources/acceptatie/` can be ingested end-to-end (extract → chunk → embed → store) and lands with status=indexed, a chunk count, and its original bytes retained in Postgres.
  2. Each stored chunk carries `document`, `line_start`, `line_end`, and source `page`, resolvable against the line-numbered canonical text extracted from the PDF.
  3. Asking a question returns a two-phase `MortgageAnswer` whose sources cite passages drawn from the top-K chunks retrieved for that query (embed query → pgvector similarity search).
  4. Both `mode=tools` and `mode=inline` route to the single RAG answer path; the `claude` CLI subprocess and whole-corpus inline dump are removed.
  5. Per-request prompt size drops from ~100K tokens (or a full agent session) to only the top-K retrieved chunks.
**Plans**: TBD

### Phase 2: Admin Document Management (Web)
**Goal**: An authenticated admin can grow and curate Teun's corpus entirely through the web — uploading PDFs, seeing ingestion status, deleting documents, and opening the original source — without touching the filesystem.
**Depends on**: Phase 1
**Requirements**: ADM-01, ADM-02, ADM-03, ADM-04, ADM-05
**Success Criteria** (what must be TRUE):
  1. An admin behind existing portal auth can upload one or more PDFs via a multipart web endpoint; non-PDF MIME types and oversized files are rejected.
  2. An uploaded PDF appears in the document-manager list with its status, chunk count, and size, and its passages become citable in subsequent answers.
  3. An admin can delete a document, which removes its chunks, stored bytes, and metadata row — after which it disappears from the list and no longer appears in answers.
  4. The web admin has a document-manager tab (upload / list / delete / status) that replaces the static "Documentatie" placeholder while preserving the existing help content.
  5. The backend serves a document's original PDF bytes inline so a citation `[N]` / source can open the source.
**Plans**: TBD
**UI hint**: yes

### Phase 3: Citation Integrity & Interface Preservation
**Goal**: With PDFs as the source of truth, every citation stays verifiable and viewable, and the entire existing surface (frontend, judge, eval, sessions) keeps working against the unchanged SSE/answer contracts.
**Depends on**: Phase 2
**Requirements**: CIT-01, CIT-02, CIT-03
**Success Criteria** (what must be TRUE):
  1. `verifier.rs` verifies each source's literal quote + `line_range` against the stored extracted text of a PDF-sourced document, marking unreliable citations as it does today.
  2. DocumentViewer opens a PDF-sourced document's extracted text with the correct line highlighted for a given citation `[N]`.
  3. The SSE `ChatEvent` stream and `MortgageAnswer`/`SourceReference` shapes are unchanged; the existing frontend, judge, eval runner, and sessions run without modification (regression verified).
**Plans**: TBD
**UI hint**: yes

## Progress

**Execution Order:**
Phases execute in numeric order: 1 → 2 → 3

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. RAG Retrieval Core & PDF Ingestion | 0/TBD | Not started | - |
| 2. Admin Document Management (Web) | 0/TBD | Not started | - |
| 3. Citation Integrity & Interface Preservation | 0/TBD | Not started | - |

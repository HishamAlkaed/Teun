---
phase: 01-rag-retrieval-core
plan: 03
subsystem: rag-ingestion
tags: [chunking, tiktoken, text-splitter, ingest-pipeline, seed-corpus, idempotency]
requires:
  - "01-01: RagStore CRUD + embed_batch (Azure text-embedding-3-large)"
  - "01-02: extract_from_bytes -> Canonical + teun-pdfium-spike runtime image"
provides:
  - "rag::chunk::chunk_document: token-bounded chunks tagged {document_id, line_start, line_end, page} (RET-03)"
  - "rag::ingest::ingest_document: extract->chunk->embed->store with status tracking + chunk_count (ING-02)"
  - "bin/ingest: idempotent one-shot seed of the 4 resources/acceptatie PDFs (ING-04)"
  - "rag::store::delete_chunks: stale-chunk cleanup for re-ingest"
affects: [01-04]
tech-stack:
  added: []
  patterns:
    - "text_splitter::TextSplitter + ChunkConfig::new(700).with_overlap(120).with_sizer(cl100k_base) via chunk_indices byte offsets"
    - "PDFium bindings bound once per process; PdfiumLibraryBindingsAlreadyInitialized -> Pdfium::default() reuse"
    - "CPU-bound extract/chunk on tokio spawn_blocking; embed/store async"
key-files:
  created:
    - apps/teun/service/src/rag/chunk.rs
    - apps/teun/service/src/rag/ingest.rs
    - apps/teun/service/src/bin/ingest.rs
  modified:
    - apps/teun/service/src/rag/mod.rs
    - apps/teun/service/src/rag/store.rs
    - apps/teun/service/src/rag/extract.rs
    - apps/teun/service/Cargo.toml
decisions:
  - "Idempotency enforced INSIDE ingest_document (delete_chunks after the filename upsert), not only in the seed bin — any future caller (Phase-2 upload) is re-run safe by construction"
  - "text-splitter 0.32 API verified against vendored source: TextSplitter::new(ChunkConfig::new(700).with_overlap(120)?.with_sizer(CoreBPE)), chunk_indices() -> (byte_offset, &str)"
  - ".md fallback list compiled in as an empty const (quality gate verdict GOOD on all 4 PDFs); ingest_document accepts a canonical override param so a flagged doc can still be ingested from its .md body"
metrics:
  duration: "~20 min"
  completed: "2026-07-24"
---

# Phase 1 Plan 03: Chunker + Ingest Pipeline + Seed Command Summary

**One-liner:** cl100k token-aware chunker (700/120) with byte-offset-derived line/page tags, a status-tracked extract->chunk->embed->store pipeline, and an idempotent seed command that indexed all 4 acceptatie PDFs into 210 chunks against live pgvector + Azure embeddings.

## What Was Built

### Task 1 — Token-aware chunker (commit c9583e4)
- `src/rag/chunk.rs`: `chunk_document(&Canonical, document_id) -> Vec<Chunk>` using `TextSplitter` with `ChunkConfig::new(700).with_overlap(120)` and a `tiktoken_rs::cl100k_base()` sizer (API confirmed against the vendored 0.32.0 source: `chunk_indices()` yields `(byte_offset, &str)`).
- `line_start` = newline count before the chunk's byte offset + 1; `line_end` = `line_start` + newlines inside the (boundary-trimmed) chunk; `page` = `page_of_line[line_start-1]`, clamped to the last page defensively.
- 5 offline unit tests against hand-built `Canonical` bodies: coverage modulo overlap, byte-offset/line agreement (independently recomputed per chunk), page attribution across a 10-page fixture, short-body single-chunk, empty-body.

### Task 2 — Ingest orchestration (commit 79730c7)
- `src/rag/ingest.rs`: `ingest_document(client, store, embed_cfg, filename, bytes, canonical_override) -> Result<String>` mirroring CC's `_process_document`: extract (spawn_blocking; PDFium is mutex-serialized) → upsert document → `delete_chunks` (idempotency) → status 'indexing' → chunk (spawn_blocking) → `embed_batch` → `insert_chunks` (one tx) → status 'indexed' + chunk_count. Any post-row failure records status='error' with the message before propagating.
- `canonical_override` supports the Plan-02 `.md` fallback (override lines attributed to page 1; PDF bytes still stored). Unused this phase — gate verdict GOOD.
- `store::delete_chunks(document_id)` added (the upserted document keeps its id, so stale chunks must be dropped before re-chunking).
- Logging: filename + chunk_count only (T-03-03).

### Task 3 — Seed command (commits a925510 fix, 41802f8 feat)
- `Cargo.toml` `[[bin]] name = "ingest"` + `src/bin/ingest.rs`: connects via `DATABASE_URL` (PgPoolOptions, migrations applied), `EmbedConfig::from_env()`, reads the four allow-listed seed PDFs from `RESOURCES_DIR` (default: walk-up `find_resources_dir` clone), skips near-duplicate `handboek_accept_versie_2026_4.pdf`, prints a per-document summary, exits non-zero if any document fails.
- `teun` is a bin-only crate, so the bin mounts the self-contained rag tree via `#[path = "../rag/mod.rs"]` (same rationale as the Plan-02 spike harness).

## Verification Results

All cargo runs in `teun-rust-build`; the seed ran in the `teun-pdfium-spike` runtime image (libpdfium.so at /app, seed PDFs in-image) against live `teun-pg-dev` with the real Azure credentials via `--env-file` (never printed).

- `cargo test --package teun rag::chunk`: **5 passed, 0 failed**.
- `cargo test --package teun rag::`: **18 passed, 0 failed, 5 ignored** (no regressions after the extract.rs fix).
- `cargo build --release` (teun + ingest bins): success; only dead_code warnings (Plan 04 consumes the remaining APIs).

### Seed run (real, first successful run)

```
=== Seed ingest summary ===
document                                                status     chunks
handboek_acceptatie_versie_2026_4_definitief.pdf        indexed        58
MUNT Beheergids 2026.pdf                                indexed        32
MUNT Hypotheekgids 2026-2.pdf                           indexed        79
MUNT Voorleggids 2026_002.pdf                           indexed        41
TOTAL                                                                 210
```

### Idempotency re-run — identical summary, TOTAL still 210 (no duplicates)

### Database cross-check (psql in teun-pg-dev)

- `documents`: exactly 4 rows, all `status=indexed`, `chunk_count` 58/32/79/41, `page_count` 64/17/35/34, `indexed_at` stamped.
- `chunks`: `total_chunks=210`, `docs=4`; `line_start >= 1`, `max(line_end)=2575` (= hypotheekgids line count), pages 1..62 (<= 64).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] PDFium re-bind failed for every document after the first**
- **Found during:** Task 3 verify (first real seed run): docs 2-4 errored with `PdfiumLibraryBindingsAlreadyInitialized`.
- **Issue:** pdfium-render 0.9 stores dynamic bindings in a process-global OnceLock — only the FIRST `bind_to_library` succeeds. `extract_from_bytes` re-bound per call, so multi-document ingest broke. Masked in Plan 01-02: its second native test *asserted* `is_err()`, which the re-bind error satisfied.
- **Fix:** Match `PdfiumLibraryBindingsAlreadyInitialized` and reuse the existing global bindings via `Pdfium::default()` (which checks the global before any dlopen, so it never panics in this branch).
- **Files modified:** `src/rag/extract.rs`
- **Commit:** a925510

**2. [Rule 2 - Idempotency placement] `delete_chunks` added in Task 2 and called inside `ingest_document`**
- The plan located stale-chunk deletion in Task 3 (the seed bin). Putting it inside the pipeline (right after the filename upsert) makes EVERY caller re-run safe — including the Phase-2 upload endpoint — instead of only the seed command. `store.rs` change explicitly permitted by the plan text.

### Notes
- **TDD RED/GREEN collapsed into one commit** (Task 1, `tdd="true"`) — same rationale/precedent as Plans 01-01/01-02: Rust tests in the same module cannot compile before the code under test; orchestrator instructed atomic per-task commits. All behavior-block bullets are covered by the 5 tests.
- **`ingest_document` signature has a 6th parameter** (`canonical_override: Option<&str>`): the plan's prose required the `.md` fallback override but its signature snippet omitted the parameter.
- **Seed bin mounts `rag` via `#[path]`** — `teun` has no lib target; documented in the bin header.
- **Expected dead_code warnings remain** (embed_query, get_document fields, etc.) — consumed by Plan 04.

## Known Stubs

None blocking. `MD_FALLBACK` in `bin/ingest.rs` is an intentionally empty compile-time list (quality-gate verdict: GOOD on all 4 PDFs, no fallback needed); the mechanism is fully wired via `ingest_document`'s override parameter if the human gate later flags a document.

## Threat Flags

None beyond the plan's threat model. T-03-01 (fixed allow-listed filenames; `RESOURCES_DIR`/walk-up resolution, no user-supplied paths), T-03-02 (embed_batch caps requests at ≤2048 inputs; largest doc = 79 inputs in 1 request; admin-triggered offline command), T-03-03 (filename + chunk_count logging only; keys via env, never printed) all implemented.

## Commits

| Task | Commit | Message |
|------|--------|---------|
| 1 | c9583e4 | feat(01-03): add token-aware chunker with line/page metadata |
| 2 | 79730c7 | feat(01-03): add ingest pipeline extract->chunk->embed->store |
| 3 (fix) | a925510 | fix(01-03): reuse process-global PDFium bindings on re-bind |
| 3 | 41802f8 | feat(01-03): add one-shot seed command for resources/acceptatie PDFs |

## Self-Check: PASSED

- Files exist: `src/rag/chunk.rs` (233 lines ≥ min 40), `src/rag/ingest.rs` (153 lines ≥ min 40), `src/bin/ingest.rs` (162 lines); key-link patterns present (`chunk_indices`/`TextSplitter` in chunk.rs; extract/chunk/embed/store orchestration in ingest.rs).
- Commits c9583e4, 79730c7, a925510, 41802f8 present on `gsd/rag-rebuild`.
- Must-have truths verified live: chunks carry document/line_start/line_end/page (DB cross-check); one-document ingest ends status=indexed with non-zero chunk_count (all 4 docs); seed run ingests exactly the 4 PDFs (near-duplicate absent from `documents`) and the re-run left total chunks at 210.

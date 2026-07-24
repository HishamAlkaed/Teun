---
phase: 01-rag-retrieval-core
plan: 01
subsystem: rag-data-layer
tags: [pgvector, sqlx, embeddings, azure-openai, migration]
requires: []
provides:
  - "migrations/003_rag.sql: vector extension + documents/chunks schema (RET-01, ING-03)"
  - "rag::store::RagStore: insert_document/set_document_status/set_document_error/insert_chunks/get_document/count_chunks"
  - "rag::embed: EmbedConfig::from_env + embed_batch/embed_query (RET-02)"
affects: [01-02, 01-03, 01-04]
tech-stack:
  added: [pdfium-render 0.9, pgvector 0.4 (sqlx), tiktoken-rs 0.12, text-splitter 0.32]
  patterns: [runtime sqlx::query().bind() (no query! macros), VARCHAR(36) uuid ids, pgvector::Vector binary binding]
key-files:
  created:
    - apps/teun/service/migrations/003_rag.sql
    - apps/teun/service/src/rag/mod.rs
    - apps/teun/service/src/rag/store.rs
    - apps/teun/service/src/rag/embed.rs
  modified:
    - apps/teun/service/Cargo.toml
    - Cargo.lock
    - apps/teun/service/src/main.rs
    - .env.example
decisions:
  - "pgvector 0.4.2 confirmed to resolve against sqlx 0.8 (cargo add --dry-run)"
  - "insert_document upserts ON CONFLICT(filename), resetting bytes/text/status to 'pending' and returning the existing id — seed re-runs idempotent"
  - "Azure branch is primary in EmbedConfig::from_env; body omits model AND dimensions (native 3072 dims)"
metrics:
  duration: "~25 min"
  completed: "2026-07-24"
---

# Phase 1 Plan 01: RAG Data Layer + Embeddings Client Summary

**One-liner:** pgvector-backed documents/chunks schema (migration 003, vector(3072)) plus a RagStore CRUD module and an Azure-primary OpenAI text-embedding-3-large client with batch-split + index-reorder semantics.

## What Was Built

### Task 1 — Crates, migration 003, env vars (commit 789e793)
- `Cargo.toml`: added `pdfium-render 0.9`, `pgvector 0.4` (feature `sqlx`), `tiktoken-rs 0.12`, `text-splitter 0.32` (feature `tiktoken-rs`). `cargo add pgvector --features sqlx --dry-run` confirmed pgvector 0.4.2 resolves against the sqlx 0.8 pin.
- `migrations/003_rag.sql`: first statement `CREATE EXTENSION IF NOT EXISTS vector;`, then `documents` (bytea `original_bytes`, line-numbered `extracted_text`, `page_count`, `status`, `chunk_count`, `error_message`, `indexed_at`) and `chunks` (`vector(3072)` embedding + `line_start`/`line_end`/`page`). Unique index on `documents(filename)` for idempotent seeding; `idx_chunks_document_id`; comment documents the deliberate no-ANN-index (flat scan) choice.
- `.env.example`: Azure OpenAI primary block (endpoint prefilled, blank key placeholder, deployment `text-embedding-3-large`, api-version `2024-02-01`) + commented plain-OpenAI fallback. No secrets committed.

### Task 2 — rag store module (commit 254824c)
- `src/rag/{mod,store}.rs` + `mod rag;` in `main.rs`. `RagStore` mirrors `session/store.rs`/`eval/store.rs` style exactly: runtime `sqlx::query().bind()`, `sqlx::Row`, uuid-v4 string ids.
- `insert_document` upserts by filename (`ON CONFLICT ... DO UPDATE ... RETURNING id`), `set_document_status` stamps `indexed_at` when status = 'indexed', `insert_chunks` writes all chunks + `pgvector::Vector` embeddings in one transaction, `get_document`, `count_chunks`. Top-K query deliberately NOT here (Plan 04).
- Two `#[ignore]`d integration tests (need a pgvector Postgres via `DATABASE_URL`): document round-trip + idempotent-id assertion, and N-chunk insert/count + 3072-dim embedding decode via `pgvector::Vector`.

### Task 3 — embeddings client (commit b00c53a)
- `src/rag/embed.rs`: `EmbedConfig::from_env()` checks the Azure branch FIRST (all four `AZURE_OPENAI_*` vars) → deployment URL + `api-key` header, body omits `model` and `dimensions`; fallback plain OpenAI Bearer auth with `EMBEDDING_MODEL` defaulting to `text-embedding-3-large`.
- `embed_batch` splits >2048 inputs into capped sub-requests (T-01-03), reorders results by the response `index` field, concatenates in input order. `embed_query` returns exactly one 3072-dim vector.
- Logging: provider name, input counts, char totals, vector counts/dims only — never the key or vectors (T-01-01, matches inline.rs discipline).
- Pure helpers (`parse_embedding_response`, `split_into_batches`, `take_single`) unit-tested offline: **6 tests pass, no network**.

## Verification Results

All cargo runs executed inside the project's own builder image (`rust:1.92-bookworm` + protobuf-compiler) via Docker — see Deviations.

- `cargo build --release`: `Finished 'release' profile [optimized] target(s)` — success (twice: after Task 1/2 and after Task 3).
- `cargo test --package teun rag::store`: `test result: ok. 0 passed; 0 failed; 2 ignored` (integration tests compile, gated on pgvector DB).
- `cargo test --package teun rag::`: `test result: ok. 6 passed; 0 failed; 2 ignored`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] No Rust toolchain on the host — cargo run via Docker**
- **Found during:** Task 1 verify
- **Issue:** No `cargo`/`rustc` anywhere on this Windows machine (PATH, ~/.cargo, Program Files, AppData, chocolatey, msys64, WSL all checked), despite STATE.md assuming cargo was available.
- **Fix:** Built a one-off helper image `teun-rust-build` (= `rust:1.92-bookworm` + `protobuf-compiler libprotobuf-dev`, exactly matching the repo Dockerfile's builder stage) and ran every build/test inside it with the repo mounted and named volumes for the cargo registry + target dir (incremental builds).
- **Files modified:** none in-repo (scratchpad Dockerfile only)
- **No new packages were installed from unverified sources** — only the official `rust:1.92-bookworm` Docker image and Debian's protobuf packages.

**2. [Note] TDD RED/GREEN commit split collapsed into one commit per task**
- Tasks 2 and 3 are `tdd="true"`, but Rust tests in the same module cannot compile before the code under test exists, and the store tests are `#[ignore]`d (no DB available). Per the orchestrator's explicit instruction ("commit atomically per task"), each task landed as a single `feat` commit containing tests + implementation. Test-first behavior blocks were honored in content (tests cover every behavior bullet).

**3. [Note] Expected dead_code warnings**
- `cargo build` emits ~15 `dead_code` warnings for the new store/embed APIs — nothing consumes them yet by design; Plans 01-02/03/04 wire them up. No warnings existed for pre-existing code.

## Known Stubs

None — no placeholder values, empty-data wirings, or TODO markers were introduced. The `rag` module is intentionally not yet consumed by any route/agent (that is Plans 02-04's job), which is scaffolding, not stubbing.

## Threat Flags

None beyond the plan's own threat model. T-01-01 (no key/vector logging), T-01-02 (parameterized binds + `pgvector::Vector` wire format), T-01-03 (≤2048 batch cap) all implemented as specified.

## Pending Human Checkpoint (BLOCKING — plan not finished)

The plan's final task is `checkpoint:human-verify`. Before Plan 01-02 executes:

1. Confirm pgvector is AVAILABLE on the `DATABASE_URL` Postgres:
   `psql "$DATABASE_URL" -c "SELECT * FROM pg_available_extensions WHERE name='vector';"` — expect one row. If empty, switch to a `pgvector/pgvector:pgNN` image or enable it on the managed instance.
2. Start the service (`cd apps/teun/service && cargo run`) and confirm the log line `Migrations applied` with NO `type "vector" does not exist` error.
3. Confirm the schema: `psql "$DATABASE_URL" -c "\d documents" -c "\d chunks"` — `documents.original_bytes` is `bytea`, `documents.extracted_text` is `text`, `chunks.embedding` is `vector(3072)`.
4. Confirm `AZURE_OPENAI_ENDPOINT` / `AZURE_OPENAI_API_KEY` / `AZURE_OPENAI_DEPLOYMENT` / `AZURE_OPENAI_API_VERSION` are set in the NextEpoch App settings (runtime env) — never committed.

Resume signal: "approved" (or describe the failure).

## Commits

| Task | Commit | Message |
|------|--------|---------|
| 1 | 789e793 | feat(01-01): add RAG crates, pgvector migration 003, and embedding env vars |
| 2 | 254824c | feat(01-01): add rag store module with Document/Chunk CRUD |
| 3 | b00c53a | feat(01-01): add Azure/OpenAI embeddings client (batch + query) |

## Self-Check: PASSED

All created files exist on disk (003_rag.sql, rag/{mod,store,embed}.rs, this SUMMARY); all three task commits (789e793, 254824c, b00c53a) present on `gsd/rag-rebuild`; store.rs is 286 lines (min_lines 60 satisfied).

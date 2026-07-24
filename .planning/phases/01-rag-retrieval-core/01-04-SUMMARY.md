---
phase: 01-rag-retrieval-core
plan: 04
subsystem: rag-answer-path
tags: [pgvector, top-k, sse-streaming, anthropic, azure-openai, provider-switch, tool-evidence, dockerfile]
requires:
  - "01-01: RagStore + EmbedConfig/embed_query (Azure text-embedding-3-large, 3072 dims)"
  - "01-03: seeded corpus live in pgvector (4 docs, 210 chunks)"
provides:
  - "rag::store::search: top-K cosine retriever (RET-04)"
  - "agent/stream.rs: shared SSE two-phase loop (stream_two_phase + SseDialect decode) + parse_two_phase_response"
  - "agent/rag.rs: run_rag = embed -> top-8 retrieve -> single streaming LLM call -> (Option<MortgageAnswer>, ToolEvidence) (RET-05)"
  - "chat.rs: both modes route to run_rag; chunk-derived evidence into judge (RET-06)"
  - "LLM_PROVIDER=anthropic|azure-openai generation switch with gen_ai.* span attrs"
  - "Node-free Docker runtime image (PDFium bundling kept)"
affects: [phase-2-upload, phase-3-verifier-rewire, eval-runner-baseline]
tech-stack:
  added: []
  patterns:
    - "provider-specific SSE decode (SseDialect enum) feeding a shared two-phase accumulate/emit loop"
    - "provider_from_lookup(&dyn Fn) so env switching is unit-testable offline"
    - "no Debug derive on secret-bearing enums (GenProvider carries API keys)"
key-files:
  created:
    - apps/teun/service/src/agent/stream.rs
    - apps/teun/service/src/agent/rag.rs
  modified:
    - apps/teun/service/src/rag/store.rs
    - apps/teun/service/src/agent/mod.rs
    - apps/teun/service/src/routes/chat.rs
    - apps/teun/Dockerfile
    - .env.example
  deleted:
    - apps/teun/service/src/agent/claude.rs
    - apps/teun/service/src/agent/inline.rs
decisions:
  - "Azure chat body uses max_completion_tokens (8192), NOT max_tokens — gpt-5.6-luna rejects max_tokens with HTTP 400 (live-verified); larger budget because reasoning models spend hidden reasoning tokens first"
  - "Azure SSE decode: chunk id -> session_id, choices[0].delta.content -> text, non-null finish_reason -> flush; data: [DONE] handled by the shared loop"
  - "RagConfig has NO OAuth-from-disk fallback (T-04-03); anthropic key = JUDGE_API_KEY || ANTHROPIC_API_KEY; model = RAG_MODEL -> INLINE_MODEL -> CLAUDE_MODEL -> claude-opus-4-6"
  - "GenProvider deliberately has no Debug derive so API keys can never leak via debug formatting"
metrics:
  duration: "~35 min"
  completed: "2026-07-24"
---

# Phase 1 Plan 04: Top-K Retriever + run_rag Answer Path Summary

**One-liner:** pgvector top-8 cosine retriever feeding a single provider-switchable (Anthropic / Azure OpenAI gpt-5.6-luna) streaming generation call that emits the byte-identical two-phase MortgageAnswer and returns chunk-derived ToolEvidence to the judge — both legacy chat modes repointed, claude CLI + whole-corpus inline dump deleted, Docker image de-Node'd. Both providers verified live against the seeded corpus.

## DELIBERATE BEHAVIOR CHANGES (re-baseline the eval runner)

Per the plan's execution_context — these are intentional, not regressions:

1. **Judge score-based retry loop REMOVED.** The old tools-mode
   `search_depth=uitgebreid` behavior re-ran the whole agent up to
   `JUDGE_MAX_RETRIES` times when the judge score was below
   `JUDGE_RETRY_THRESHOLD` (old chat.rs:232-321), keeping the best-scoring
   answer. That loop depended on the re-runnable claude subprocess. Every
   request is now a SINGLE run_rag pass + ONE judge call. Only the
   transient-error retry wrapper survives (MAX_ERROR_RETRIES=2 with 2s pause,
   same as the old inline path). `JudgeConfig.retry_threshold/max_retries`
   are still populated but no longer drive any retry.
   **Eval impact:** answers that previously improved on retry will now keep
   their first-pass score; latency/cost per uitgebreid question drops to one
   pass. Eval expectations keyed to retry behavior must be re-baselined.

2. **`search_depth` depth instruction REMOVED for BOTH modes.** The
   "[BELANGRIJK - SNELLE MODUS: max 3 zoekopdrachten…]" /
   "[UITGEBREIDE MODUS…]" prompt prepend (old chat.rs:111-120) is meaningless
   for a single-call no-tools prompt. The field is still accepted and logged
   (frontend compat) but has NO effect on the prompt or behavior.
   The language prepend ("[Antwoord in het Engels…]") is KEPT.
   **Eval impact:** quick vs uitgebreid now produce identical requests.

## What Was Built

### Task 1 — Top-K cosine retriever (commit a65efd7)
- `rag/store.rs`: `RetrievedChunk { content, document, line_start, line_end, page }` + `search(query_embedding, k)` using the research query verbatim: `SELECT c.content, d.filename AS document, c.line_start, c.line_end, c.page FROM chunks c JOIN documents d ON d.id = c.document_id ORDER BY c.embedding <=> $1 LIMIT $2`, binding `pgvector::Vector`.
- `#[ignore]`d integration tests, all run PASSING against live teun-pg-dev: closest-first ordering with crafted embeddings (exact-match embedding ranks #1 globally; relative order A<B<C by cosine distance; metadata carried), k-limit, and empty-table → empty Vec (proved via a scratch database created/migrated/dropped inside the test, so the seeded DB is untouched).
- Live semantic smoke test (`live_semantic_search_smoke`, gated on `TEUN_SMOKE_QUERY`): "Wat is de maximale hypotheek?" → top hit `MUNT Hypotheekgids 2026-2.pdf (p21 l1509-1548)` "…Bij een hypotheekbedrag boven € 1.000.000 is financiering alleen mogelijk als dit past volgens een annuïtaire lastentoets…" — semantically on target.

### Task 2 — Shared streaming module (commit 7f76c61)
- `agent/stream.rs`: `stream_two_phase` (SSE loop with `\n---JSON---` separator handling + char-boundary-safe partial slicing) and `parse_two_phase_response` MOVED from inline.rs, logic byte-for-byte; `run_inline` delegated to them (still compiled at this point; deleted in T4). 5 offline parser unit tests added.

### Task 3 — run_rag + generation-provider abstraction (commit ab218fe)
- `agent/rag.rs`: `run_rag(client, store, embed_cfg, config, message, tx) -> Result<(Option<MortgageAnswer>, ToolEvidence)>`:
  embed_query → `store.search(embedding, 8)` → `ToolEvidence::add_range(document, line_start, line_end)` per chunk (option-A) → fragment-aware NL system prompt → streaming generation → shared `stream_two_phase` → `parse_two_phase_response` → `ChatEvent::Result` (identical shape).
- **Prompt is fragment-aware, NOT inline's whole-corpus wording:** instructs (NL) that these are de meest relevante fragmenten, NIET het volledige corpus; absence from fragments ≠ absence from policy (say so honestly, invent nothing); cite the visible line number as `line_range`; copy quotes LETTERLIJK without the line-number prefix. Chunks formatted `### Document: {filename} (pagina {page})` with each line prefixed `{n}: ` counting from `chunk.line_start` (inline numbering convention, resolves against the canonical body).
- **Provider abstraction:** `RagConfig::from_env()` reads `LLM_PROVIDER` (`anthropic` default | `azure-openai`).
  - anthropic: Messages API exactly like the old inline path (dedicated client with connect 30s / read 120s / NO total timeout, `x-api-key`, `cache_control: ephemeral` on the system block, `max_tokens: 4096`). Model: `RAG_MODEL` → `INLINE_MODEL` → `CLAUDE_MODEL` → `claude-opus-4-6`.
  - azure-openai: `POST {AZURE_OPENAI_ENDPOINT}/openai/deployments/{AZURE_OPENAI_CHAT_DEPLOYMENT}/chat/completions?api-version={AZURE_OPENAI_API_VERSION}`, `api-key` header, system prompt as the `system` message, **`max_completion_tokens: 8192` (gpt-5.6-luna rejects `max_tokens` with HTTP 400 — live-verified by the orchestrator and encoded in a unit test)**.
  - `stream.rs` gained `SseDialect { Anthropic, AzureOpenAi }`: the SSE `data:` decode is the only provider-specific step (Anthropic message_start/content_block_delta/message_stop; Azure chunk id / choices[0].delta.content / finish_reason, `[DONE]` sentinel); the two-phase accumulate/emit logic is shared and unchanged.
  - Tracing: span renamed `ai.rag`, records `gen_ai.system` ("anthropic"/"azure-openai") + `gen_ai.request.model` (model id / deployment name) + `gen_ai.operation.name = "rag_answer"` for Langfuse per-provider cost tracking.
- 10 offline unit tests (provider switch incl. missing-var errors, model fallback chain, chunk formatting from line_start, evidence assembly, both request bodies).
- `.env.example`: `LLM_PROVIDER`, `RAG_MODEL`, `AZURE_OPENAI_CHAT_DEPLOYMENT` (example `gpt-5.6-luna`) documented.

### Task 4 — Repoint + delete + de-Node (commit f46fff7)
- `chat.rs`: ONE pipeline for both modes → `run_rag` (mode + search_depth logged only); `RagStore::new(state.pool.clone())` + `EmbedConfig::from_env()` + `RagConfig::from_env()` constructed in the handler with clear config errors; returned evidence goes into `judge::run_judge(..., &evidence)`; depth prepend + judge retry loop removed (see behavior changes); transient-error retry wrapper kept.
- Deleted `agent/claude.rs` (CLI subprocess, evidence collection from tool_use, session-id validation) and `agent/inline.rs` (whole-corpus loader, OAuth-from-disk fallback, InlineConfig). `find_project_root` relocated to `agent/mod.rs`.
- `Dockerfile`: Node.js install + `npm install -g @anthropic-ai/claude-code` + credentials-mount comment removed; PDFium bundling (chromium/7881, sha256-gated) kept; net -883 lines in the commit.
- `main.rs` unchanged — `AppState.pool` was already exposed.

## Verification Results

All cargo runs in the `teun-rust-build` helper image (no host toolchain); live tests against `teun-pg-dev` + real `.env` credentials via `--env-file` (keys never printed).

- Task 1: `cargo test --package teun rag::store` compiles, ignored by default; against live DB: `test result: ok. 2 passed; 0 failed` (ordering + empty-table).
- Task 2: `cargo test --package teun agent::` → `5 passed; 0 failed`.
- Task 3: `cargo build --release` OK; `cargo test --package teun agent::` → `18 passed; 0 failed`.
- Task 4 (full suite): `cargo build --release --package teun` → `Finished 'release' profile [optimized]`; `cargo test --package teun` → **`test result: ok. 80 passed; 0 failed; 9 ignored`**.
- Remaining teun-bin warnings are only the not-yet-consumed ingest/extract APIs (Phase-2 upload) and `ChatEvent::ToolUse` "never constructed" (nothing emits tool events anymore; variant kept for the SSE contract).

### Live end-to-end run_rag smoke (both providers, seeded corpus, real creds)

`agent::rag::tests::live_rag_answer_smoke` (env-gated harness added this plan):

- **LLM_PROVIDER=anthropic** (claude-opus-4-6, 23.2s): streamed Partial events + exactly 1 Result; parsed MortgageAnswer with answer "…Het totale maximale hypotheekbedrag is **€ 1.350.000**…", category Standard, **8 sources** all citing `MUNT Hypotheekgids 2026-2.pdf` with numeric line_ranges (e.g. `1508-1510`, quote "Het totale maximale hypotheekbedrag is € 1.350.000. …annuïtaire lastentoets.") drawn from the retrieved chunks; evidence = 8 ranges over 3 documents (e.g. `"MUNT Hypotheekgids 2026-2.pdf": [(1509,1548),(1546,1579),(29,77),(1470,1513),(1058,1092)]`).
- **LLM_PROVIDER=azure-openai** (deployment gpt-5.6-luna, 4.2s): `provider: azure-openai / model: gpt-5.6-luna`, `events: 173 partial, 1 result`; same question → answer "…de maximale hypotheek is **€ 1.350.000**, exclusief een eventuele overbruggingshypotheek…", category Standard, 3 sources with line_ranges (`36-43`, `1550-1553`, `1560-1562`), identical chunk-derived evidence. Two-phase contract honored by both providers.

## Deviations from Plan

### Auto-fixed / adjusted

**1. [Interfaces-mandated] T3 touched `stream.rs` + `inline.rs` beyond T3's file list**
- The interfaces block requires factoring `stream_two_phase` so the SSE decode is provider-specific; that lives in stream.rs (`SseDialect` + `decode_sse_data` + 3 decode unit tests), and inline.rs's call site gained the dialect argument (T2's byte-for-byte extraction was preserved first, then factored). No behavior change for the Anthropic path.

**2. [Rule 2 - live-verified constraint] Azure body uses `max_completion_tokens: 8192`, not `max_tokens`**
- Orchestrator live-verified gpt-5.6-luna rejects `max_tokens` (HTTP 400). Budget set to 8192 (vs Anthropic 4096) because reasoning models consume hidden reasoning tokens from the same budget. Locked in by unit test `azure_body_uses_max_completion_tokens_not_max_tokens`.

**3. [Note] No `Debug` derive on `GenProvider`**
- It carries API keys; a derived Debug could leak them into logs. A test was rewritten to avoid `expect_err` (which needs Debug on the Ok type).

**4. [Note] Live smoke harnesses added (not in plan file list)**
- `rag/store.rs::live_semantic_search_smoke` and `agent/rag.rs::live_rag_answer_smoke`, both `#[ignore]`d and env-gated (`TEUN_SMOKE_QUERY`) — same precedent as the Plan-02 spike harness; they document how to re-verify retrieval/generation and were used for the live evidence above.

**5. [Note] TDD RED/GREEN collapsed into one commit (Task 1, tdd="true")**
- Same rationale/precedent as Plans 01-01/02/03: Rust tests in the same module cannot compile before the code under test; the retrieval tests additionally need a live pgvector DB. All behavior-block bullets are covered (and were RUN, not just compiled).

**6. [Note] `gsd-sdk query` unavailable** — on-PATH gsd-sdk is `@gsd-build/sdk` v0.1.0 without the `query` glue (known since 01-01); STATE.md/summary updates done manually.

### Not deviations (explicitly planned)
- The two behavior changes in the section above.
- `judge/verifier.rs` untouched (CONTEXT-locked; still disk-based — Phase-3 rewires it). Degraded PDF-quote verification remains the accepted Phase-1 gap; the option-A evidence wiring is in place and live-verified.

## Known Stubs

None. Both modes are fully wired to live retrieval + generation. `ChatEvent::Thinking`/`ToolUse` variants are no longer emitted by the answer path (Thinking still used by the retry wrapper; ToolUse kept only for SSE-contract/persistence compatibility) — this is contract preservation, not stubbing.

## Threat Flags

None beyond the plan's threat model. T-04-01 (skill instructions authoritative in the system block; chunks delimited as reference fragments under their own header), T-04-02 (run_rag logs chunk count + prompt length only; smoke harness prints answer content, never keys), T-04-03 (OAuth-from-disk read + Node/claude CLI removed; env API keys only) all implemented. T-04-04 accepted as planned.

## Pending Human Checkpoint (BLOCKING — plan not finished)

Final task is `checkpoint:human-verify` — E2E chat verification (orchestrator-run). Required env for the service:

- `DATABASE_URL` → pgvector Postgres with the seeded corpus (dev: `postgres://postgres:test@localhost:15432/teun`; from a container: `host.docker.internal:15432`)
- Embeddings (needed for BOTH providers): `AZURE_OPENAI_ENDPOINT`, `AZURE_OPENAI_API_KEY`, `AZURE_OPENAI_DEPLOYMENT=text-embedding-3-large`, `AZURE_OPENAI_API_VERSION=2024-02-01`
- Generation: `LLM_PROVIDER=anthropic` (default) + `ANTHROPIC_API_KEY` (optional `RAG_MODEL`), or `LLM_PROVIDER=azure-openai` + `AZURE_OPENAI_CHAT_DEPLOYMENT=gpt-5.6-luna`
- Judge (always Anthropic): `ANTHROPIC_API_KEY` (or `JUDGE_API_KEY`); verifier still reads `.md` files from `RESOURCES_DIR` (walk-up default)
- Optional: `LANGFUSE_ENABLED=true` + `LANGFUSE_PUBLIC_KEY`/`LANGFUSE_SECRET_KEY`/`LANGFUSE_HOST` to check the `ai.rag` span attributes

## Commits

| Task | Commit | Message |
|------|--------|---------|
| 1 | a65efd7 | feat(01-04): add top-K cosine retriever to rag store |
| 2 | 7f76c61 | refactor(01-04): extract shared SSE streaming module from inline.rs |
| 3 | ab218fe | feat(01-04): add run_rag answer path with provider-switchable generation |
| 4 | f46fff7 | feat(01-04): route both chat modes to run_rag; delete claude CLI + inline corpus paths; de-Node image |

## Self-Check: PASSED

- Files exist: `agent/stream.rs` (exports parse_two_phase_response + stream_two_phase), `agent/rag.rs` (exports run_rag; 562 lines ≥ min 80), `rag/store.rs` (exports search with `<=>` + LIMIT); `agent/claude.rs` + `agent/inline.rs` deleted; Dockerfile contains no `nodejs`/`claude-code`, still contains the PDFium block.
- Key-link patterns present: `<=>` in store.rs; `run_rag` dispatch in chat.rs with evidence into run_judge; `parse_two_phase_response` reused in rag.rs; `add_range` populated from chunk line ranges.
- Commits a65efd7, 7f76c61, ab218fe, f46fff7 present on `gsd/rag-rebuild`.
- Must-have truths verified live: top-K returns document/line/page metadata (2 DB tests + smoke); two-phase MortgageAnswer cites retrieved passages (both providers); run_rag returns evidence and chat.rs feeds it to the judge (code-wired, judge E2E pending checkpoint); both modes route to run_rag; CLI/inline-dump/Node removed; SSE + MortgageAnswer shapes untouched (types.rs unchanged this plan); provider switchable via env with gen_ai.* span attrs.

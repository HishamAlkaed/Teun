# Phase 1 — Planning Context

**Phase:** 01-rag-retrieval-core
**Captured:** 2026-07-17
**Source artifacts:** RESEARCH.md, ROADMAP.md, REQUIREMENTS.md, PROJECT.md, codebase/TEUN_MAP.md

This file captures the load-bearing context, locked decisions, and scope
boundaries that every plan in this phase must honor. Plans reference it; it is
not itself executable.

## Phase Goal (from ROADMAP)

Teun answers a mortgage-acceptance question from the top-K retrieved policy
passages via a single Anthropic Messages call, replacing both the `run_claude`
subprocess and the whole-corpus `run_inline` dump — cutting per-request cost
while keeping the `MortgageAnswer` output shape byte-identical.

## Requirements in scope

| ID | Summary | Plan |
|----|---------|------|
| RET-01 | pgvector extension + `documents`/`chunks` schema via sqlx migration | 01 |
| ING-03 | Original PDF bytes stored in Postgres `bytea` | 01 |
| RET-02 | OpenAI `text-embedding-3-large` embedding client over reqwest (batch + single) | 01 |
| ING-01 | pdfium-render extraction → line-numbered canonical body + line→page map | 02 |
| RET-03 | Chunker tags each chunk `{document, line_start, line_end, page}` | 03 |
| ING-02 | Ingest pipeline extract→chunk→embed→store with status + chunk_count | 03 |
| ING-04 | Seed the `resources/acceptatie/` PDFs | 03 |
| RET-04 | Retriever: embed query → top-K cosine similarity | 04 |
| RET-05 | New single-call answer path emitting `MortgageAnswer` | 04 |
| RET-06 | `mode` field → single RAG path; delete `run_claude` + whole-corpus `run_inline` | 04 |

## Locked decisions (do NOT reconsider)

- **Vector store:** pgvector on the existing sqlx + Postgres. No new external service. No ANN index at this corpus size (flat scan).
- **Crates (pinned by research):** `pdfium-render 0.9`, `pgvector 0.4` (feature `sqlx`), `tiktoken-rs 0.12`, `text-splitter 0.32` (feature `tiktoken-rs`). All four passed the Package Legitimacy Audit (crates.io, source-backed) — no install checkpoint required.
- **Embeddings:** OpenAI `text-embedding-3-large` (3072 dims) via `reqwest`. Azure OpenAI is an accepted variant behind env branch.
- **PDF extraction:** `pdfium-render`; bundle the PDFium native `.so` (pinned bblanchon release + checksum) into the Docker image.
- **Migrations:** the existing embedded `sqlx::migrate!("./migrations")` + runtime `sqlx::query()` pattern. NO `sqlx::query!` compile-time macros (no build-time DB). New file `003_rag.sql`; `CREATE EXTENSION IF NOT EXISTS vector` is its first statement.
- **PDF bytes:** Postgres `bytea` (no Azure blob).
- **`mode` field:** both `tools` and `inline` repoint to the single `run_rag` path; field kept only for frontend compat.
- **Answer path:** `agent/rag.rs` reuses `inline.rs`'s SSE streaming loop, the `\n---JSON---` two-phase split, char-boundary-safe slicing, and `parse_two_phase_response`. **These are currently INLINED inside `run_inline` (not callable helpers)** — they must first be EXTRACTED into a shared `agent/stream.rs`, logic byte-for-byte unchanged, then called from `run_rag`. Do not re-implement SSE/two-phase parsing.
- **`run_rag` return contract:** `run_rag(...) -> Result<(Option<MortgageAnswer>, ToolEvidence)>` — mirrors `run_claude`'s return so `chat.rs` can feed the assembled `ToolEvidence` into `judge::run_judge`. run_rag builds that evidence from each retrieved chunk's `(document, line_start, line_end)`. This is the option-A verifier mitigation — it must stay wired.
- **RAG prompt wording is NOT inline's verbatim:** inline's system-prompt substitution asserts "De volledige beleidsdocumenten staan hieronder" (the COMPLETE corpus is present). The RAG prompt contains only top-K fragments, so the instruction text MUST be adapted — present the chunks as the meest relevante fragmenten (not the full corpus) and tell the model that absence from the fragments ≠ absence from policy (say so honestly, invent nothing).
- **Interface compatibility (HARD):** SSE `ChatEvent` contract + `MortgageAnswer`/`SourceReference` shapes + existing REST endpoints stay backward-compatible. Frontend, judge, eval runner, and sessions depend on them.

## Claude's discretion (choices made for this phase)

- Chunk size **700 tokens**, overlap **120**, tokenizer **cl100k_base** (research A2). Tunable.
- Top-K = **8** retrieved chunks. No similarity threshold in Phase 1.
- No ANN index (flat scan) — fastest at this corpus size.
- Internal module layout: new `src/rag/` module (`extract`, `chunk`, `embed`, `store`, `ingest`) + `src/agent/rag.rs` + `src/agent/stream.rs` (shared streaming helpers extracted from inline.rs).
- Seed ingestion trigger: a one-shot `--bin ingest` command, idempotent by filename (re-run does not duplicate chunks).

## Deliberate behavior changes (Plan 04 — record in SUMMARY)

- **Judge score-based retry loop dropped.** Today, tools-mode `search_depth=uitgebreid`
  re-runs the whole agent up to `max_retries` when the judge score is low
  (chat.rs:232-321). That loop depended on the re-runnable claude subprocess.
  On the single-call RAG path it is REMOVED — every request is a single pass +
  one judge call (only the transient-error retry wrapper is kept). Intentional
  simplification; the eval-runner baseline should be re-set accordingly.
- **`search_depth` depth instructions removed from the RAG path.** The
  "[BELANGRIJK - SNELLE MODUS: max 3 zoekopdrachten…]" / "[UITGEBREIDE MODUS…]"
  prepend (chat.rs:111-120) is meaningless for a no-tools single-call prompt and
  is dropped for BOTH modes. The language ("Antwoord in het Engels") prepend stays.

## Scope boundaries (explicit)

- **Verifier / DocumentViewer rewiring is Phase 3 (CIT-01/02), NOT this phase.** Phase 1 MUST persist the line-numbered `extracted_text` per document in `documents.extracted_text` so Phase 3 can point `verifier.rs` at it. Phase 1 does NOT change `judge/verifier.rs` (still reads from disk).
- **Acknowledged Phase-1 judge gap:** because `verifier.rs` still reads the on-disk `resources_dir` and answers now cite PDF filenames, literal-quote verification for PDF-sourced citations is degraded this phase. Mitigation (research option A): `run_rag` builds `ToolEvidence` from the retrieved chunks' stored `line_start/line_end` ranges (returned to chat.rs, passed to the judge) and formats prompt chunks with visible canonical line numbers so the model cites real line ranges. LLM-faithfulness judging still runs. This gap is expected and closed in Phase 3.
- **Admin upload/list/delete web UI → Phase 2.** Phase 1 only needs the seed corpus ingested (one-shot command).
- Out of scope (do not build): web scraper, tiered pdf/table indexes, Azure AI Search/Blob, non-PDF formats, auth rework, LLM stub descriptions.

## Seed corpus (ING-04)

`resources/acceptatie/` PDFs to ingest:
- `handboek_acceptatie_versie_2026_4_definitief.pdf` (the final handboek)
- `MUNT Beheergids 2026.pdf`
- `MUNT Hypotheekgids 2026-2.pdf`
- `MUNT Voorleggids 2026_002.pdf`

**Skip** `handboek_accept_versie_2026_4.pdf` (near-duplicate of `_definitief`) to avoid polluting top-K with duplicate chunks. Filenames contain spaces — handle quoting. The parallel `.md` files remain on disk (fallback for the disk-based verifier and the PDF-extraction quality gate).

## Two blocking infra prerequisites

1. **pgvector server extension** — the Postgres targeted by `DATABASE_URL` must have the `vector` server extension available, or `CREATE EXTENSION` in migration `003` fails at startup (`type "vector" does not exist`). There is no `docker-compose` in this repo (deployment is external), so this is a **human infra prerequisite** → checkpoint in Plan 01. Use a `pgvector/pgvector:pgNN` image or enable the extension on the managed instance.
2. **PDFium native library in the Docker image** — `pdfium-render` does not bundle PDFium. The `debian:bookworm-slim` runtime stage must ship `libpdfium.so` (pinned bblanchon release) plus `libstdc++6`. Confidence is LOW → **spike + checkpoint in Plan 02** (local `docker build` + load + extract one seed PDF end-to-end).

## Interface contracts to preserve (from types.rs / chat.rs)

- `ChatEvent` (tag `type`, content `data`, snake_case): `thinking`, `tool_use`, `partial`, `result{structured_output, session_id, message_id}`, `error`, `judge`.
- `MortgageAnswer { answer, rationale, sources[], category }`; `category` ∈ `standard | mandaat_uitzondering | doorverwijzen_speciale_afhandeling`.
- `SourceReference { document, section, quote?, line_range? }`.
- Two-phase stream: answer text, then `\n---JSON---\n{rationale,sources,category}`.
- `judge::run_judge(client, cfg, question, &MortgageAnswer, &ToolEvidence)` unchanged signature — Phase 1 supplies real chunk-derived `ToolEvidence`, not `ToolEvidence::default()`.

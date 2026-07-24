# NextEpoch Deployment Checklist — RAG Backend (gsd/rag-rebuild)

Purpose: when this branch is ready, a Claude session running INSIDE NextEpoch works through this checklist to verify the environment and tell ops what to set. Written 2026-07-24.

## 1. Postgres: pgvector availability (verify first, blocks everything)

Run against the app's `DATABASE_URL`:

```sql
SELECT name, default_version FROM pg_available_extensions WHERE name = 'vector';
```

- **1 row** → OK. Migration `003_rag.sql` runs `CREATE EXTENSION IF NOT EXISTS vector;` itself at startup (needs superuser or the extension pre-enabled; if `CREATE EXTENSION` is denied on managed PG, ask ops to enable it once).
- **0 rows** → pgvector not installed on the server. Ask ops to switch the DB to a `pgvector/pgvector:pgNN` image or enable the extension on the managed instance. Nothing else can proceed.

Verified locally 2026-07-24 on `pgvector/pgvector:pg16` (vector 0.8.5): migrations 001→003 apply clean, `chunks.embedding` = `vector(3072)`, insert + `<=>` cosine search work.

## 2. Required env vars (NextEpoch App settings — secrets never in repo)

### Embeddings (Azure OpenAI — required for ingestion AND every query)
| Var | Value |
|-----|-------|
| `AZURE_OPENAI_ENDPOINT` | `https://dmfco-ai-tools-resource.cognitiveservices.azure.com/` |
| `AZURE_OPENAI_API_KEY` | secret (Azure portal → resource → Keys and Endpoint) |
| `AZURE_OPENAI_DEPLOYMENT` | `text-embedding-3-large` |
| `AZURE_OPENAI_API_VERSION` | `2024-02-01` |

### Generation (provider-switchable, plan 01-04)
| Var | Value |
|-----|-------|
| `LLM_PROVIDER` | `anthropic` (default) or `azure-openai` |
| `RAG_MODEL` | Anthropic model id when provider=anthropic (falls back to `INLINE_MODEL` → `CLAUDE_MODEL`) |
| `ANTHROPIC_API_KEY` | secret — required when provider=anthropic |
| `AZURE_OPENAI_CHAT_DEPLOYMENT` | Azure chat deployment name (e.g. `gpt-5.6-luna`) — required when provider=azure-openai; separate from the embeddings deployment |

Cost A/B test = flip `LLM_PROVIDER` + set the matching model var, restart. Compare in Langfuse (`gen_ai.system` / `gen_ai.request.model` on the `ai.rag` span).

### Judge (existing, unchanged)
| Var | Value |
|-----|-------|
| `JUDGE_API_KEY` (or reuse `ANTHROPIC_API_KEY`) | secret |
| `JUDGE_MODEL` | default `claude-haiku-4-5-20251001` |

### Existing vars that stay as-is
`DATABASE_URL`, Langfuse/OTLP tracing vars, `RESOURCES_DIR`, `SKILL_PATH`, `CHAT_RETENTION_DAYS`, `RATE_LIMIT_PER_SECOND`.

### Vars that DISAPPEAR after plan 01-04
Claude CLI / OAuth-from-disk path is deleted (no more Node/claude CLI in the image, no `~/.claude/.credentials.json` fallback).

## 3. Post-deploy smoke test

1. Startup log shows migrations applied, no `type "vector" does not exist`.
2. Ingestion ran / seed corpus present: `SELECT filename, status, chunk_count FROM documents;` — 4 seed PDFs indexed (skip near-duplicate `handboek_accept_versie_2026_4.pdf`).
3. `curl -N -X POST .../api/teun/chat -H 'content-type: application/json' -d '{"message":"Wat is de maximale hypotheek?"}'` → SSE `partial` events then `result` with MortgageAnswer (answer/rationale/sources/category), then `judge` event with non-empty source verdicts.
4. Prompt size in logs = top-K chunks (a few K tokens), not ~100K.
5. Provider switch test (step 7 of plan 01-04 checkpoint).

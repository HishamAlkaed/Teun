-- RAG data layer: pgvector extension + documents/chunks schema (RET-01, ING-03).
-- Requires the pgvector server extension to be AVAILABLE on the target Postgres
-- (e.g. pgvector/pgvector:pgNN image, or enabled on the managed instance).
CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE IF NOT EXISTS documents (
    id             VARCHAR(36) PRIMARY KEY,
    filename       VARCHAR(255) NOT NULL,
    original_bytes BYTEA NOT NULL,
    extracted_text TEXT NOT NULL,          -- line-numbered canonical body (the citation source of truth; Phase 3 points verifier.rs here)
    page_count     INTEGER NOT NULL DEFAULT 0,
    status         VARCHAR(20) NOT NULL DEFAULT 'pending',  -- pending|indexing|indexed|error
    chunk_count    INTEGER NOT NULL DEFAULT 0,
    error_message  TEXT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    indexed_at     TIMESTAMPTZ
);

-- Unique filename so seed ingest re-runs are idempotent (upsert by filename).
CREATE UNIQUE INDEX IF NOT EXISTS idx_documents_filename ON documents (filename);

CREATE TABLE IF NOT EXISTS chunks (
    id          VARCHAR(36) PRIMARY KEY,
    document_id VARCHAR(36) NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    content     TEXT NOT NULL,
    line_start  INTEGER NOT NULL,
    line_end    INTEGER NOT NULL,
    page        INTEGER NOT NULL,
    embedding   VECTOR(3072) NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_chunks_document_id ON chunks (document_id);

-- No ANN index in Phase 1: at this corpus size a flat scan is faster than
-- building/maintaining an HNSW/IVFFlat index (pgvector README guidance).

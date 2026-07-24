//! `documents` / `chunks` CRUD over sqlx + pgvector (migration 003).
//!
//! Mirrors the runtime-query style of `session/store.rs` and `eval/store.rs`:
//! `sqlx::query("... $1 ...").bind(x)` with `sqlx::Row` — no `sqlx::query!`
//! macros (no build-time DB). IDs are `VARCHAR(36)` uuid-v4 strings.
//!
//! The top-K cosine similarity query deliberately does NOT live here yet —
//! it belongs to Plan 04 (retriever).

use anyhow::{Context, Result};
use pgvector::Vector;
use sqlx::{PgPool, Row};

/// A row in `documents` (without the raw `original_bytes` payload).
#[derive(Debug, Clone)]
pub struct Document {
    pub id: String,
    pub filename: String,
    /// Line-numbered canonical body — the citation source of truth.
    pub extracted_text: String,
    pub page_count: i32,
    /// pending | indexing | indexed | error
    pub status: String,
    pub chunk_count: i32,
}

/// A row in `chunks` (embedding is carried separately as `Vec<f32>`).
#[derive(Debug, Clone)]
pub struct Chunk {
    pub id: String,
    pub document_id: String,
    pub content: String,
    pub line_start: i32,
    pub line_end: i32,
    pub page: i32,
}

#[derive(Clone)]
pub struct RagStore {
    pool: PgPool,
}

impl RagStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Upsert a document by filename (idempotent for seed re-runs) and return
    /// its id. On conflict the existing row keeps its id; bytes, text and
    /// counters are reset so re-ingestion starts from a clean 'pending' state.
    pub async fn insert_document(
        &self,
        filename: &str,
        original_bytes: &[u8],
        extracted_text: &str,
        page_count: i32,
    ) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let row = sqlx::query(
            "INSERT INTO documents (id, filename, original_bytes, extracted_text, page_count, status, chunk_count) \
             VALUES ($1, $2, $3, $4, $5, 'pending', 0) \
             ON CONFLICT (filename) DO UPDATE SET \
                 original_bytes = EXCLUDED.original_bytes, \
                 extracted_text = EXCLUDED.extracted_text, \
                 page_count     = EXCLUDED.page_count, \
                 status         = 'pending', \
                 chunk_count    = 0, \
                 error_message  = NULL, \
                 indexed_at     = NULL \
             RETURNING id",
        )
        .bind(&id)
        .bind(filename)
        .bind(original_bytes)
        .bind(extracted_text)
        .bind(page_count)
        .fetch_one(&self.pool)
        .await
        .context("Failed to insert document")?;
        Ok(row.get("id"))
    }

    /// Update a document's ingestion status and counters. When the status is
    /// 'indexed', `indexed_at` is stamped; an 'error' status stores the message.
    pub async fn set_document_status(
        &self,
        id: &str,
        status: &str,
        chunk_count: i32,
        page_count: i32,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE documents SET \
                 status      = $2, \
                 chunk_count = $3, \
                 page_count  = $4, \
                 indexed_at  = CASE WHEN $2 = 'indexed' THEN now() ELSE indexed_at END \
             WHERE id = $1",
        )
        .bind(id)
        .bind(status)
        .bind(chunk_count)
        .bind(page_count)
        .execute(&self.pool)
        .await
        .context("Failed to set document status")?;
        Ok(())
    }

    /// Record an ingestion failure on the document row.
    pub async fn set_document_error(&self, id: &str, message: &str) -> Result<()> {
        sqlx::query("UPDATE documents SET status = 'error', error_message = $2 WHERE id = $1")
            .bind(id)
            .bind(message)
            .execute(&self.pool)
            .await
            .context("Failed to set document error")?;
        Ok(())
    }

    /// Insert chunks with their embeddings in a single transaction.
    /// Embeddings are bound via `pgvector::Vector` (binary wire format —
    /// never hand-rolled float-to-text).
    pub async fn insert_chunks(&self, chunks: &[(Chunk, Vec<f32>)]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        for (chunk, embedding) in chunks {
            sqlx::query(
                "INSERT INTO chunks (id, document_id, content, line_start, line_end, page, embedding) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(&chunk.id)
            .bind(&chunk.document_id)
            .bind(&chunk.content)
            .bind(chunk.line_start)
            .bind(chunk.line_end)
            .bind(chunk.page)
            .bind(Vector::from(embedding.clone()))
            .execute(&mut *tx)
            .await
            .context("Failed to insert chunk")?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn get_document(&self, id: &str) -> Result<Option<Document>> {
        let row = sqlx::query(
            "SELECT id, filename, extracted_text, page_count, status, chunk_count \
             FROM documents WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to get document")?;

        Ok(row.map(|r| Document {
            id: r.get("id"),
            filename: r.get("filename"),
            extracted_text: r.get("extracted_text"),
            page_count: r.get("page_count"),
            status: r.get("status"),
            chunk_count: r.get("chunk_count"),
        }))
    }

    pub async fn count_chunks(&self, document_id: &str) -> Result<i64> {
        let row = sqlx::query("SELECT COUNT(*) AS n FROM chunks WHERE document_id = $1")
            .bind(document_id)
            .fetch_one(&self.pool)
            .await
            .context("Failed to count chunks")?;
        Ok(row.get("n"))
    }
}

// Integration tests: these hit a real Postgres with the pgvector extension,
// reached via DATABASE_URL, so they are `#[ignore]`d by default. Run with:
//   DATABASE_URL=postgres://... cargo test --package teun rag::store -- --ignored
#[cfg(test)]
mod tests {
    use super::*;

    const EMBED_DIM: usize = 3072;

    async fn test_store() -> RagStore {
        let url = std::env::var("DATABASE_URL")
            .expect("DATABASE_URL must point at a pgvector-enabled Postgres");
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to test database");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("apply migrations");
        RagStore::new(pool)
    }

    fn dummy_embedding(seed: f32) -> Vec<f32> {
        (0..EMBED_DIM).map(|i| seed + i as f32 * 1e-6).collect()
    }

    #[tokio::test]
    #[ignore = "requires a pgvector-enabled Postgres via DATABASE_URL"]
    async fn document_round_trip() {
        let store = test_store().await;
        let filename = format!("test-{}.pdf", uuid::Uuid::new_v4());
        let id = store
            .insert_document(&filename, b"%PDF-1.7 dummy", "1: regel een\n2: regel twee", 3)
            .await
            .expect("insert_document");

        let doc = store
            .get_document(&id)
            .await
            .expect("get_document")
            .expect("document exists");
        assert_eq!(doc.filename, filename);
        assert_eq!(doc.extracted_text, "1: regel een\n2: regel twee");
        assert_eq!(doc.status, "pending");
        assert_eq!(doc.page_count, 3);

        // Idempotent upsert: same filename returns the same id.
        let id2 = store
            .insert_document(&filename, b"%PDF-1.7 dummy", "1: regel een\n2: regel twee", 3)
            .await
            .expect("re-insert_document");
        assert_eq!(id, id2);

        // Cleanup (cascade removes chunks).
        sqlx::query("DELETE FROM documents WHERE id = $1")
            .bind(&id)
            .execute(&store.pool)
            .await
            .expect("cleanup");
    }

    #[tokio::test]
    #[ignore = "requires a pgvector-enabled Postgres via DATABASE_URL"]
    async fn insert_chunks_and_count() {
        let store = test_store().await;
        let filename = format!("test-{}.pdf", uuid::Uuid::new_v4());
        let doc_id = store
            .insert_document(&filename, b"%PDF-1.7 dummy", "body", 1)
            .await
            .expect("insert_document");

        let chunks: Vec<(Chunk, Vec<f32>)> = (0..3)
            .map(|i| {
                (
                    Chunk {
                        id: uuid::Uuid::new_v4().to_string(),
                        document_id: doc_id.clone(),
                        content: format!("chunk {i}"),
                        line_start: i * 10 + 1,
                        line_end: i * 10 + 9,
                        page: i + 1,
                    },
                    dummy_embedding(i as f32),
                )
            })
            .collect();
        let first_chunk_id = chunks[0].0.id.clone();

        store.insert_chunks(&chunks).await.expect("insert_chunks");
        let n = store.count_chunks(&doc_id).await.expect("count_chunks");
        assert_eq!(n, 3);

        // Stored embedding decodes back to a 3072-length Vec<f32>.
        let row = sqlx::query("SELECT embedding FROM chunks WHERE id = $1")
            .bind(&first_chunk_id)
            .fetch_one(&store.pool)
            .await
            .expect("fetch chunk embedding");
        let embedding: Vector = row.try_get("embedding").expect("decode pgvector::Vector");
        assert_eq!(embedding.as_slice().len(), EMBED_DIM);

        // Cleanup (cascade removes chunks).
        sqlx::query("DELETE FROM documents WHERE id = $1")
            .bind(&doc_id)
            .execute(&store.pool)
            .await
            .expect("cleanup");
    }
}

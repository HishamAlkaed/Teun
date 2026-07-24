//! `documents` / `chunks` CRUD over sqlx + pgvector (migration 003).
//!
//! Mirrors the runtime-query style of `session/store.rs` and `eval/store.rs`:
//! `sqlx::query("... $1 ...").bind(x)` with `sqlx::Row` — no `sqlx::query!`
//! macros (no build-time DB). IDs are `VARCHAR(36)` uuid-v4 strings.
//!
//! Includes the top-K cosine similarity retriever (`search`, Plan 04): the
//! ranking is pushed to pgvector via `ORDER BY embedding <=> $1 LIMIT $2`
//! (flat scan — deliberate no-ANN-index choice at this corpus size).

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

/// A row in `documents` for the admin list view (ADM-02): metadata only —
/// the raw `original_bytes` payload is never loaded, its size is computed
/// in SQL via `octet_length`.
#[derive(Debug, Clone)]
pub struct DocumentListItem {
    pub id: String,
    pub filename: String,
    /// pending | indexing | indexed | error
    pub status: String,
    pub chunk_count: i32,
    pub page_count: i32,
    /// Size of the stored original PDF (`octet_length(original_bytes)`).
    pub size_bytes: i64,
    pub error_message: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub indexed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// A chunk returned by the top-K similarity search, carrying everything the
/// answer path needs: prompt content, citation metadata (document filename +
/// canonical line range) and the page for display.
#[derive(Debug, Clone)]
pub struct RetrievedChunk {
    pub content: String,
    /// Source document filename (joined from `documents.filename`).
    pub document: String,
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

    /// Delete all chunks of a document (idempotent re-ingest: the upserted
    /// document keeps its id, so stale chunks must be dropped before
    /// re-chunking to avoid duplicates).
    pub async fn delete_chunks(&self, document_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM chunks WHERE document_id = $1")
            .bind(document_id)
            .execute(&self.pool)
            .await
            .context("Failed to delete chunks")?;
        Ok(())
    }

    /// Fetch the canonical (PDF-extracted) body for a document by filename.
    /// The judge verifier matches cited quotes/line ranges against this text,
    /// since chunk line numbers refer to it rather than to any file on disk.
    pub async fn get_extracted_text(&self, filename: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT extracted_text FROM documents WHERE filename = $1")
            .bind(filename)
            .fetch_optional(&self.pool)
            .await
            .context("Failed to get extracted_text")?;
        Ok(row.map(|r| r.get("extracted_text")))
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

    /// Top-K cosine similarity search (RET-04): returns at most `k` chunks
    /// ordered by ascending cosine distance to the query embedding (closest
    /// first). An empty `chunks` table yields an empty Vec, not an error.
    pub async fn search(&self, query_embedding: Vec<f32>, k: i64) -> Result<Vec<RetrievedChunk>> {
        let rows = sqlx::query(
            "SELECT c.content, d.filename AS document, c.line_start, c.line_end, c.page \
             FROM chunks c JOIN documents d ON d.id = c.document_id \
             ORDER BY c.embedding <=> $1 LIMIT $2",
        )
        .bind(Vector::from(query_embedding))
        .bind(k)
        .fetch_all(&self.pool)
        .await
        .context("Failed to run top-K similarity search")?;

        Ok(rows
            .into_iter()
            .map(|r| RetrievedChunk {
                content: r.get("content"),
                document: r.get("document"),
                line_start: r.get("line_start"),
                line_end: r.get("line_end"),
                page: r.get("page"),
            })
            .collect())
    }

    /// List all documents for the admin view (ADM-02), newest first. The raw
    /// PDF payload is never fetched; its size comes from `octet_length`.
    pub async fn list_documents(&self) -> Result<Vec<DocumentListItem>> {
        let rows = sqlx::query(
            "SELECT id, filename, status, chunk_count, page_count, \
                    octet_length(original_bytes)::BIGINT AS size_bytes, \
                    error_message, created_at, indexed_at \
             FROM documents ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to list documents")?;

        Ok(rows
            .into_iter()
            .map(|r| DocumentListItem {
                id: r.get("id"),
                filename: r.get("filename"),
                status: r.get("status"),
                chunk_count: r.get("chunk_count"),
                page_count: r.get("page_count"),
                size_bytes: r.get("size_bytes"),
                error_message: r.get("error_message"),
                created_at: r.get("created_at"),
                indexed_at: r.get("indexed_at"),
            })
            .collect())
    }

    /// Delete a document by id (ADM-03). Its chunks go with it via the
    /// `ON DELETE CASCADE` foreign key. Returns whether a row was deleted.
    pub async fn delete_document(&self, id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM documents WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .context("Failed to delete document")?;
        Ok(result.rows_affected() > 0)
    }

    /// Fetch the stored original PDF bytes by filename (ADM-05, inline
    /// serving). `None` when no document with that filename exists.
    pub async fn get_pdf_bytes(&self, filename: &str) -> Result<Option<Vec<u8>>> {
        let row = sqlx::query("SELECT original_bytes FROM documents WHERE filename = $1")
            .bind(filename)
            .fetch_optional(&self.pool)
            .await
            .context("Failed to get pdf bytes")?;
        Ok(row.map(|r| r.get("original_bytes")))
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

    /// A (nearly) one-hot vector along dimension `axis`, with an optional
    /// small second component along `axis2` to control cosine distance.
    fn directed_embedding(axis: usize, axis2: Option<(usize, f32)>) -> Vec<f32> {
        let mut v = vec![0.0f32; EMBED_DIM];
        v[axis] = 1.0;
        if let Some((a2, w)) = axis2 {
            v[a2] = w;
        }
        v
    }

    #[tokio::test]
    #[ignore = "requires a pgvector-enabled Postgres via DATABASE_URL"]
    async fn search_returns_topk_ordered_with_metadata() {
        let store = test_store().await;
        let filename = format!("test-search-{}.pdf", uuid::Uuid::new_v4());
        let doc_id = store
            .insert_document(&filename, b"%PDF-1.7 dummy", "body", 1)
            .await
            .expect("insert_document");

        // Three chunks with crafted embeddings relative to the query (= exactly
        // chunk A's embedding): dist(A)=0 < dist(B)≈0.001 < dist(C)=1.
        let emb_a = directed_embedding(0, None);
        let emb_b = directed_embedding(0, Some((1, 0.05)));
        let emb_c = directed_embedding(1, None);
        let mk = |name: &str, i: i32| Chunk {
            id: uuid::Uuid::new_v4().to_string(),
            document_id: doc_id.clone(),
            content: format!("chunk {name}"),
            line_start: i * 10 + 1,
            line_end: i * 10 + 9,
            page: i + 1,
        };
        let chunks = vec![
            (mk("A", 0), emb_a.clone()),
            (mk("B", 1), emb_b),
            (mk("C", 2), emb_c),
        ];
        store.insert_chunks(&chunks).await.expect("insert_chunks");

        // k large enough to cover any pre-existing corpus rows plus ours,
        // so we can assert the RELATIVE order of our three chunks.
        let total: i64 = sqlx::query("SELECT COUNT(*) AS n FROM chunks")
            .fetch_one(&store.pool)
            .await
            .expect("count all chunks")
            .get("n");
        let results = store
            .search(emb_a.clone(), total + 10)
            .await
            .expect("search");
        assert!(results.len() <= (total + 10) as usize);

        // Closest-first: the exact-match embedding must rank first globally.
        assert_eq!(results[0].content, "chunk A");
        assert_eq!(results[0].document, filename);
        assert_eq!(results[0].line_start, 1);
        assert_eq!(results[0].line_end, 9);
        assert_eq!(results[0].page, 1);

        // Relative order of our chunks follows ascending cosine distance.
        let ours: Vec<&str> = results
            .iter()
            .filter(|r| r.document == filename)
            .map(|r| r.content.as_str())
            .collect();
        assert_eq!(ours, vec!["chunk A", "chunk B", "chunk C"]);

        // k limits the result count.
        let top2 = store.search(emb_a, 2).await.expect("search k=2");
        assert_eq!(top2.len(), 2.min(total as usize + 3));

        // Cleanup (cascade removes chunks).
        sqlx::query("DELETE FROM documents WHERE id = $1")
            .bind(&doc_id)
            .execute(&store.pool)
            .await
            .expect("cleanup");
    }

    #[tokio::test]
    #[ignore = "requires a pgvector-enabled Postgres via DATABASE_URL"]
    async fn search_on_empty_chunks_table_returns_empty_vec() {
        // A scratch database guarantees an EMPTY chunks table without touching
        // whatever corpus lives in the main DATABASE_URL database.
        let url = std::env::var("DATABASE_URL")
            .expect("DATABASE_URL must point at a pgvector-enabled Postgres");
        let admin_pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .expect("connect to admin database");
        let scratch_db = format!("teun_search_test_{}", uuid::Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE DATABASE {scratch_db}"))
            .execute(&admin_pool)
            .await
            .expect("create scratch database");

        let (base, _) = url.rsplit_once('/').expect("DATABASE_URL has a db path");
        let scratch_url = format!("{base}/{scratch_db}");
        let result = async {
            let pool = sqlx::postgres::PgPoolOptions::new()
                .max_connections(1)
                .connect(&scratch_url)
                .await
                .expect("connect to scratch database");
            sqlx::migrate!("./migrations")
                .run(&pool)
                .await
                .expect("apply migrations to scratch database");
            let store = RagStore::new(pool);
            let hits = store.search(dummy_embedding(0.0), 8).await.expect("search");
            store.pool.close().await;
            hits
        }
        .await;

        sqlx::query(&format!("DROP DATABASE {scratch_db}"))
            .execute(&admin_pool)
            .await
            .expect("drop scratch database");

        assert!(result.is_empty(), "empty chunks table must yield an empty Vec");
    }

    #[tokio::test]
    #[ignore = "requires a pgvector-enabled Postgres via DATABASE_URL"]
    async fn list_documents_returns_metadata_newest_first() {
        let store = test_store().await;
        let pdf_old = b"%PDF-1.7 oude bytes".to_vec();
        let pdf_new = b"%PDF-1.7 nieuwe upload met meer bytes".to_vec();
        let name_old = format!("test-list-old-{}.pdf", uuid::Uuid::new_v4());
        let name_new = format!("test-list-new-{}.pdf", uuid::Uuid::new_v4());

        let id_old = store
            .insert_document(&name_old, &pdf_old, "body oud", 2)
            .await
            .expect("insert old document");
        // Deterministic ordering: push the first document one day into the past
        // (two autocommit inserts can land on near-identical timestamps).
        sqlx::query("UPDATE documents SET created_at = now() - interval '1 day' WHERE id = $1")
            .bind(&id_old)
            .execute(&store.pool)
            .await
            .expect("backdate old document");
        let id_new = store
            .insert_document(&name_new, &pdf_new, "body nieuw", 5)
            .await
            .expect("insert new document");

        let list = store.list_documents().await.expect("list_documents");

        let item_new = list
            .iter()
            .find(|d| d.id == id_new)
            .expect("new document listed");
        assert_eq!(item_new.filename, name_new);
        assert_eq!(item_new.status, "pending");
        assert_eq!(item_new.chunk_count, 0);
        assert_eq!(item_new.page_count, 5);
        assert_eq!(item_new.size_bytes, pdf_new.len() as i64);
        assert!(item_new.error_message.is_none());
        assert!(item_new.indexed_at.is_none());

        // Newest first: the fresh document must appear before the backdated one.
        let pos_new = list.iter().position(|d| d.id == id_new).unwrap();
        let pos_old = list.iter().position(|d| d.id == id_old).unwrap();
        assert!(pos_new < pos_old, "list must be ordered created_at DESC");
        assert_eq!(
            list.iter().find(|d| d.id == id_old).unwrap().size_bytes,
            pdf_old.len() as i64
        );

        // Cleanup.
        for id in [&id_old, &id_new] {
            sqlx::query("DELETE FROM documents WHERE id = $1")
                .bind(id)
                .execute(&store.pool)
                .await
                .expect("cleanup");
        }
    }

    #[tokio::test]
    #[ignore = "requires a pgvector-enabled Postgres via DATABASE_URL"]
    async fn delete_document_cascades_chunks() {
        let store = test_store().await;
        let filename = format!("test-delete-{}.pdf", uuid::Uuid::new_v4());
        let doc_id = store
            .insert_document(&filename, b"%PDF-1.7 dummy", "body", 1)
            .await
            .expect("insert_document");
        let chunks: Vec<(Chunk, Vec<f32>)> = (0..2)
            .map(|i| {
                (
                    Chunk {
                        id: uuid::Uuid::new_v4().to_string(),
                        document_id: doc_id.clone(),
                        content: format!("chunk {i}"),
                        line_start: i + 1,
                        line_end: i + 1,
                        page: 1,
                    },
                    dummy_embedding(i as f32),
                )
            })
            .collect();
        store.insert_chunks(&chunks).await.expect("insert_chunks");
        assert_eq!(store.count_chunks(&doc_id).await.expect("count"), 2);

        // Delete removes the row; chunks go via ON DELETE CASCADE.
        let deleted = store.delete_document(&doc_id).await.expect("delete_document");
        assert!(deleted, "existing document must report deleted=true");
        assert!(store
            .get_document(&doc_id)
            .await
            .expect("get_document")
            .is_none());
        assert_eq!(store.count_chunks(&doc_id).await.expect("count"), 0);

        // Deleting an unknown id reports false (route maps this to 404).
        let again = store.delete_document(&doc_id).await.expect("re-delete");
        assert!(!again);
    }

    #[tokio::test]
    #[ignore = "requires a pgvector-enabled Postgres via DATABASE_URL"]
    async fn get_pdf_bytes_round_trip_and_unknown() {
        let store = test_store().await;
        let filename = format!("test-pdf-{}.pdf", uuid::Uuid::new_v4());
        let pdf = b"%PDF-1.7\n1 0 obj\n<<>>\nendobj\n%%EOF".to_vec();
        let doc_id = store
            .insert_document(&filename, &pdf, "body", 1)
            .await
            .expect("insert_document");

        let bytes = store
            .get_pdf_bytes(&filename)
            .await
            .expect("get_pdf_bytes")
            .expect("stored document has bytes");
        assert_eq!(bytes, pdf, "returned bytes must equal the stored original");

        let unknown = store
            .get_pdf_bytes("nope-does-not-exist.pdf")
            .await
            .expect("get_pdf_bytes unknown");
        assert!(unknown.is_none());

        // Cleanup.
        sqlx::query("DELETE FROM documents WHERE id = $1")
            .bind(&doc_id)
            .execute(&store.pool)
            .await
            .expect("cleanup");
    }

    /// Live end-to-end retrieval smoke test: embeds a real question via the
    /// configured embeddings provider and searches the seeded corpus. Gated on
    /// TEUN_SMOKE_QUERY (+ DATABASE_URL + AZURE_OPENAI_*/OPENAI_API_KEY).
    /// Run with --nocapture to see the retrieved chunks.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL, embeddings credentials and TEUN_SMOKE_QUERY"]
    async fn live_semantic_search_smoke() {
        let query = std::env::var("TEUN_SMOKE_QUERY").expect("set TEUN_SMOKE_QUERY");
        let store = test_store().await;
        let cfg = crate::rag::embed::EmbedConfig::from_env().expect("embed config");
        let client = reqwest::Client::new();
        let embedding = crate::rag::embed::embed_query(&client, &cfg, &query)
            .await
            .expect("embed query");
        let hits = store.search(embedding, 8).await.expect("search");
        assert!(!hits.is_empty(), "seeded corpus should return chunks");
        println!("query: {query}");
        for (i, hit) in hits.iter().enumerate() {
            let preview: String = hit.content.chars().take(220).collect();
            println!(
                "#{} {} (p{} l{}-{}):\n  {}\n",
                i + 1,
                hit.document,
                hit.page,
                hit.line_start,
                hit.line_end,
                preview.replace('\n', "\n  ")
            );
        }
    }
}

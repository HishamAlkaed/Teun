//! One-shot seed command for the `resources/acceptatie` PDFs (ING-04).
//!
//! Ingests the four seed policy PDFs (skipping the near-duplicate
//! `handboek_accept_versie_2026_4.pdf`) through the full
//! extract → chunk → embed → store pipeline and prints a per-document
//! summary. Idempotent: `insert_document` upserts on filename and the
//! pipeline drops stale chunks before re-chunking, so re-runs never
//! duplicate chunks.
//!
//! Env: `DATABASE_URL` (pgvector Postgres), `AZURE_OPENAI_*` or
//! `OPENAI_API_KEY` (embeddings), optional `RESOURCES_DIR`
//! (default: walk up from CWD to find `resources/acceptatie`).

// `teun` is a bin-only crate, so this second binary mounts the self-contained
// `rag` module tree directly (same pattern rationale as the Plan-02 spike
// harness: there is no lib target to link against).
#[path = "../rag/mod.rs"]
mod rag;

use std::path::PathBuf;

use anyhow::{Context, Result};
use sqlx::postgres::PgPoolOptions;

use crate::rag::embed::EmbedConfig;
use crate::rag::ingest::ingest_document;
use crate::rag::store::RagStore;

/// The seed corpus (CONTEXT ING-04). `handboek_accept_versie_2026_4.pdf` is
/// deliberately absent: it is a near-duplicate of `_definitief` and would
/// pollute top-K with duplicate chunks.
const SEED_FILES: &[&str] = &[
    "handboek_acceptatie_versie_2026_4_definitief.pdf",
    "MUNT Beheergids 2026.pdf",
    "MUNT Hypotheekgids 2026-2.pdf",
    "MUNT Voorleggids 2026_002.pdf",
];

/// Plan-02 quality-gate `.md` fallback list. The gate verdict was GOOD on all
/// four seed PDFs, so no document is ingested from its `.md` body. If the
/// human gate later flags a document, add `(pdf_filename, md_filename)` here.
const MD_FALLBACK: &[(&str, &str)] = &[];

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let database_url = std::env::var("DATABASE_URL")
        .context("DATABASE_URL must point at a pgvector-enabled Postgres")?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .context("Failed to connect to Postgres")?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("Failed to apply migrations")?;

    let embed_cfg = EmbedConfig::from_env()?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .expect("Failed to build HTTP client");
    let store = RagStore::new(pool);

    let resources_dir = std::env::var("RESOURCES_DIR").unwrap_or_else(|_| find_resources_dir());
    tracing::info!(resources_dir = %resources_dir, documents = SEED_FILES.len(), "Seeding RAG corpus");

    let mut failures = 0usize;
    let mut summary: Vec<(String, String, i64)> = Vec::new();

    for filename in SEED_FILES {
        let path = PathBuf::from(&resources_dir).join(filename);
        let result = seed_one(&client, &store, &embed_cfg, filename, &path).await;
        match result {
            Ok((status, chunk_count)) => summary.push((filename.to_string(), status, chunk_count)),
            Err(e) => {
                failures += 1;
                tracing::error!(filename, error = format!("{e:#}"), "Seed ingest failed");
                summary.push((filename.to_string(), "error".to_string(), 0));
            }
        }
    }

    println!("\n=== Seed ingest summary ===");
    println!("{:<55} {:<10} {:>6}", "document", "status", "chunks");
    let mut total_chunks = 0i64;
    for (filename, status, chunk_count) in &summary {
        println!("{filename:<55} {status:<10} {chunk_count:>6}");
        total_chunks += chunk_count;
    }
    println!("{:<55} {:<10} {total_chunks:>6}", "TOTAL", "");

    if failures > 0 {
        anyhow::bail!("{failures} document(s) failed to ingest");
    }
    Ok(())
}

/// Ingest one seed file and return its final (status, chunk_count) from the DB.
async fn seed_one(
    client: &reqwest::Client,
    store: &RagStore,
    embed_cfg: &EmbedConfig,
    filename: &str,
    path: &std::path::Path,
) -> Result<(String, i64)> {
    let bytes =
        std::fs::read(path).with_context(|| format!("Failed to read {}", path.display()))?;

    // Honor the Plan-02 quality-gate fallback list (currently empty).
    let md_override = match MD_FALLBACK.iter().find(|(pdf, _)| pdf == &filename) {
        Some((_, md_file)) => {
            let md_path = path
                .parent()
                .map(|dir| dir.join(md_file))
                .unwrap_or_else(|| PathBuf::from(md_file));
            Some(
                std::fs::read_to_string(&md_path)
                    .with_context(|| format!("Failed to read fallback {}", md_path.display()))?,
            )
        }
        None => None,
    };

    let id = ingest_document(
        client,
        store,
        embed_cfg,
        filename,
        &bytes,
        md_override.as_deref(),
    )
    .await?;

    let doc = store
        .get_document(&id)
        .await?
        .context("ingested document not found")?;
    let chunk_count = store.count_chunks(&id).await?;
    Ok((doc.status, chunk_count))
}

/// Walk up from CWD to find `resources/acceptatie` (same behavior as
/// `judge::find_resources_dir` in the main binary).
fn find_resources_dir() -> String {
    if let Ok(mut dir) = std::env::current_dir() {
        for _ in 0..5 {
            let candidate = dir.join("resources/acceptatie");
            if candidate.is_dir() {
                return candidate.to_string_lossy().to_string();
            }
            if !dir.pop() {
                break;
            }
        }
    }
    "resources/acceptatie".to_string()
}

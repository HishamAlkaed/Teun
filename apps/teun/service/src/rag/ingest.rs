//! Ingest pipeline: extract → chunk → embed → store (ING-02).
//!
//! Mirrors CC's `_process_document`: one document flows through PDF extraction
//! (or an optional pre-extracted canonical override for the `.md` fallback),
//! chunking, batched embedding, and chunk storage, with the `documents.status`
//! column tracking pending → indexing → indexed (or error + message).
//!
//! Logging discipline (T-03-03): log filename + chunk_count only — never
//! embeddings, never document bodies.

use anyhow::{bail, Context, Result};

use crate::rag::chunk::chunk_document;
use crate::rag::embed::{embed_batch, EmbedConfig};
use crate::rag::extract::{extract_from_bytes, Canonical};
use crate::rag::store::{Chunk, RagStore};

/// Ingest one document end-to-end and return its document id.
///
/// Stages:
/// 1. Extract the canonical body from the PDF bytes (PDFium is blocking and
///    process-wide serialized, so it runs on the blocking pool). When
///    `canonical_override` is `Some` (Plan-02 quality-gate `.md` fallback),
///    extraction is skipped: the canonical body is built from the override
///    text (all lines attributed to page 1 — `.md` bodies carry no page
///    information) while the original PDF bytes are still stored.
/// 2. Upsert the `documents` row (idempotent by filename; resets to
///    'pending'), delete any chunks left from a previous ingest of the same
///    document, then mark 'indexing'.
/// 3. Chunk the canonical body (CPU-bound tokenizer → blocking pool).
/// 4. Embed all chunk contents (batched ≤2048 inputs per request).
/// 5. Store chunks + embeddings in one transaction.
/// 6. Mark 'indexed' with the final chunk_count.
///
/// Any failure after the document row exists records status='error' with the
/// message before propagating. A failure during extraction propagates without
/// a row (there is no extracted text to store yet).
pub async fn ingest_document(
    client: &reqwest::Client,
    store: &RagStore,
    embed_cfg: &EmbedConfig,
    filename: &str,
    bytes: &[u8],
    canonical_override: Option<&str>,
) -> Result<String> {
    // Stage 1: extract (or take the .md fallback override).
    let (canonical, page_count) = match canonical_override {
        Some(md_text) => (canonical_from_override(md_text), 0i32),
        None => {
            let owned = bytes.to_vec();
            let extracted = tokio::task::spawn_blocking(move || extract_from_bytes(&owned))
                .await
                .context("extraction task panicked")?
                .with_context(|| format!("extraction failed for {filename}"))?;
            (extracted.canonical, extracted.page_count as i32)
        }
    };

    // Stage 2: upsert the document row (resets status to 'pending').
    let id = store
        .insert_document(filename, bytes, &canonical.text, page_count)
        .await?;

    match run_indexing_stages(client, store, embed_cfg, &id, canonical, page_count).await {
        Ok(chunk_count) => {
            tracing::info!(filename, chunk_count, "document ingested");
            Ok(id)
        }
        Err(e) => {
            let message = format!("{e:#}");
            if let Err(mark_err) = store.set_document_error(&id, &message).await {
                tracing::warn!(filename, error = %mark_err, "failed to record ingest error on document");
            }
            Err(e.context(format!("ingest failed for {filename}")))
        }
    }
}

/// Stages 2b-6: everything after the document row exists. Errors here are
/// recorded on the row by the caller.
async fn run_indexing_stages(
    client: &reqwest::Client,
    store: &RagStore,
    embed_cfg: &EmbedConfig,
    id: &str,
    canonical: Canonical,
    page_count: i32,
) -> Result<i32> {
    // Idempotency: a re-ingest of an existing filename keeps its document id;
    // drop the previous chunks so re-runs never duplicate.
    store.delete_chunks(id).await?;
    store.set_document_status(id, "indexing", 0, page_count).await?;

    // Stage 3: chunk (CPU-bound cl100k tokenizer → blocking pool).
    let id_owned = id.to_string();
    let chunks = tokio::task::spawn_blocking(move || chunk_document(&canonical, &id_owned))
        .await
        .context("chunking task panicked")?;
    if chunks.is_empty() {
        bail!("document produced no chunks (empty canonical body)");
    }

    // Stage 4: embed all chunk contents (embed_batch splits >2048 inputs).
    let contents: Vec<String> = chunks.iter().map(|c| c.content.clone()).collect();
    let embeddings = embed_batch(client, embed_cfg, &contents).await?;
    if embeddings.len() != chunks.len() {
        bail!(
            "embedding count mismatch: {} embeddings for {} chunks",
            embeddings.len(),
            chunks.len()
        );
    }

    // Stage 5: store chunks + embeddings in one transaction.
    let pairs: Vec<(Chunk, Vec<f32>)> = chunks.into_iter().zip(embeddings).collect();
    store.insert_chunks(&pairs).await?;

    // Stage 6: mark indexed.
    let chunk_count = pairs.len() as i32;
    store
        .set_document_status(id, "indexed", chunk_count, page_count)
        .await?;
    Ok(chunk_count)
}

/// Build a canonical body from pre-extracted text (`.md` fallback). Every
/// line is attributed to page 1: markdown bodies carry no page information.
fn canonical_from_override(text: &str) -> Canonical {
    let mut body = String::new();
    let mut page_of_line = Vec::new();
    for line in text.lines() {
        body.push_str(line);
        body.push('\n');
        page_of_line.push(1);
    }
    Canonical {
        text: body,
        page_of_line,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_canonical_maps_every_line_to_page_one() {
        let canonical = canonical_from_override("# Titel\r\nregel twee\nregel drie");
        assert_eq!(canonical.text, "# Titel\nregel twee\nregel drie\n");
        assert_eq!(canonical.page_of_line, vec![1, 1, 1]);
        assert_eq!(canonical.page_of_line.len(), canonical.text.lines().count());
    }
}

//! Admin document management (ADM-01/02/03): multipart PDF upload feeding the
//! existing ingest pipeline, DB-backed listing, and delete-with-cascade.
//!
//! No in-service auth, matching `routes/admin.rs` — portal auth fronts /admin
//! in deployment.
//!
//! Upload flow: validate every file (MIME + `%PDF-` magic bytes + size cap),
//! create the pending `documents` row synchronously so the list reflects the
//! upload immediately, then run `ingest_document` in a spawned task and return
//! 202. Logging discipline: filenames + sizes only — never file bytes.

use std::sync::Arc;

use axum::extract::{DefaultBodyLimit, Multipart, Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Json, Router};

use crate::rag::embed::EmbedConfig;
use crate::rag::ingest::ingest_document;
use crate::rag::store::RagStore;
use crate::AppState;

/// Per-file cap (validation), below the per-route body limit so a single
/// maximal file plus multipart framing still fits in one request.
const MAX_FILE_BYTES: usize = 50 * 1024 * 1024;

/// Per-route override of the global 64 KiB `DefaultBodyLimit` (main.rs).
const UPLOAD_BODY_LIMIT: usize = 60 * 1024 * 1024;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/teun/admin/documents",
            axum::routing::get(list_documents)
                .post(upload_documents)
                // Inner layers overwrite the outer global 64 KiB default.
                .route_layer(DefaultBodyLimit::max(UPLOAD_BODY_LIMIT)),
        )
        .route(
            "/api/teun/admin/documents/{id}",
            axum::routing::delete(delete_document),
        )
}

// ── Upload ─────────────────────────────────────────────────────────

/// Validate one uploaded file: PDF content type, `%PDF-` magic bytes, size
/// cap, and a safe filename (no path separators / traversal).
fn validate_upload(filename: &str, content_type: &str, bytes: &[u8]) -> Result<(), String> {
    if filename.trim().is_empty() {
        return Err("Bestand zonder bestandsnaam geweigerd".to_string());
    }
    if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
        return Err(format!("Ongeldige bestandsnaam: {filename}"));
    }
    if !content_type.to_ascii_lowercase().contains("pdf") {
        return Err(format!(
            "Alleen PDF-bestanden zijn toegestaan (geweigerd: {filename})"
        ));
    }
    if !bytes.starts_with(b"%PDF-") {
        return Err(format!("Bestand is geen geldige PDF: {filename}"));
    }
    if bytes.len() > MAX_FILE_BYTES {
        return Err(format!(
            "Bestand is te groot (max {} MB): {filename}",
            MAX_FILE_BYTES / (1024 * 1024)
        ));
    }
    Ok(())
}

async fn upload_documents(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    // Embeddings config is required before we accept anything: without it the
    // spawned ingestion could never index the document.
    let embed_cfg = match EmbedConfig::from_env() {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::error!(error = %e, "Upload rejected: embeddings provider not configured");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "Embeddings-provider is niet geconfigureerd"
                })),
            )
                .into_response();
        }
    };

    // Read + validate ALL files first; any invalid file rejects the request
    // before a single row is created or ingestion starts.
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to read multipart field");
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "Upload kon niet worden gelezen (te groot of ongeldig)"
                    })),
                )
                    .into_response();
            }
        };
        if field.name() != Some("file") {
            continue;
        }
        let filename = field.file_name().unwrap_or_default().to_string();
        let content_type = field.content_type().unwrap_or_default().to_string();
        let bytes = match field.bytes().await {
            Ok(b) => b.to_vec(),
            Err(e) => {
                tracing::warn!(filename = %filename, error = %e, "Failed to read uploaded file");
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": format!("Bestand kon niet worden gelezen: {filename}")
                    })),
                )
                    .into_response();
            }
        };
        if let Err(message) = validate_upload(&filename, &content_type, &bytes) {
            tracing::warn!(filename = %filename, size = bytes.len(), "Upload rejected: {message}");
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": message })),
            )
                .into_response();
        }
        files.push((filename, bytes));
    }

    if files.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Geen bestanden ontvangen (verwacht multipart veld 'file')"
            })),
        )
            .into_response();
    }

    let store = RagStore::new(state.pool.clone());
    let mut accepted: Vec<serde_json::Value> = Vec::new();

    for (filename, bytes) in files {
        // Create the pending row synchronously so GET list shows the document
        // immediately; ingest_document re-upserts the same filename (same id)
        // with the real extracted text and drives pending→indexing→indexed.
        let id = match store.insert_document(&filename, &bytes, "", 0).await {
            Ok(id) => id,
            Err(e) => {
                tracing::error!(filename = %filename, error = format!("{e:#}"), "Failed to create document row");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": format!("Kon document niet opslaan: {filename}")
                    })),
                )
                    .into_response();
            }
        };

        tracing::info!(filename = %filename, size = bytes.len(), "Upload accepted, ingestion started");
        let client = state.http_client.clone();
        let task_store = store.clone();
        let task_cfg = embed_cfg.clone();
        let task_filename = filename.clone();
        tokio::spawn(async move {
            match ingest_document(&client, &task_store, &task_cfg, &task_filename, &bytes, None)
                .await
            {
                // ingest_document already logs filename + chunk_count.
                Ok(_) => {}
                Err(e) => {
                    let message = format!("{e:#}");
                    tracing::error!(filename = %task_filename, error = %message, "Upload ingestion failed");
                    // Extraction failures propagate before ingest_document can
                    // record them; mark the pre-created row so it never sticks
                    // in 'pending'. (Post-extraction failures were already
                    // marked — this overwrite is harmless.)
                    if let Err(mark_err) = task_store.set_document_error(&id, &message).await {
                        tracing::warn!(filename = %task_filename, error = %mark_err, "Failed to record ingest error");
                    }
                }
            }
        });

        accepted.push(serde_json::json!({
            "filename": filename,
            "status": "pending",
        }));
    }

    (StatusCode::ACCEPTED, Json(serde_json::json!(accepted))).into_response()
}

// ── List ───────────────────────────────────────────────────────────

async fn list_documents(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let store = RagStore::new(state.pool.clone());
    match store.list_documents().await {
        Ok(docs) => {
            let items: Vec<serde_json::Value> = docs
                .into_iter()
                .map(|d| {
                    serde_json::json!({
                        "id": d.id,
                        "filename": d.filename,
                        "status": d.status,
                        "chunk_count": d.chunk_count,
                        "page_count": d.page_count,
                        "size_bytes": d.size_bytes,
                        "error_message": d.error_message,
                        "created_at": d.created_at.to_rfc3339(),
                        "indexed_at": d.indexed_at.map(|t| t.to_rfc3339()),
                    })
                })
                .collect();
            (StatusCode::OK, Json(serde_json::json!(items)))
        }
        Err(e) => {
            tracing::error!(error = format!("{e:#}"), "Failed to list documents");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Kon documenten niet ophalen" })),
            )
        }
    }
}

// ── Delete ─────────────────────────────────────────────────────────

async fn delete_document(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let store = RagStore::new(state.pool.clone());
    match store.delete_document(&id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Document niet gevonden" })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = format!("{e:#}"), "Failed to delete document");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Kon document niet verwijderen" })),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PDF: &[u8] = b"%PDF-1.7 minimal";

    #[test]
    fn valid_pdf_passes() {
        assert!(validate_upload("gids.pdf", "application/pdf", PDF).is_ok());
    }

    #[test]
    fn content_type_case_insensitive_and_with_charset() {
        assert!(validate_upload("gids.pdf", "Application/PDF; charset=binary", PDF).is_ok());
    }

    #[test]
    fn non_pdf_content_type_rejected() {
        let err = validate_upload("gids.pdf", "text/plain", PDF).unwrap_err();
        assert!(err.contains("Alleen PDF"));
    }

    #[test]
    fn missing_magic_bytes_rejected() {
        let err = validate_upload("gids.pdf", "application/pdf", b"MZ not a pdf").unwrap_err();
        assert!(err.contains("geen geldige PDF"));
    }

    #[test]
    fn empty_body_rejected() {
        assert!(validate_upload("gids.pdf", "application/pdf", b"").is_err());
    }

    #[test]
    fn oversized_file_rejected() {
        let mut big = b"%PDF-".to_vec();
        big.resize(MAX_FILE_BYTES + 1, 0u8);
        let err = validate_upload("gids.pdf", "application/pdf", &big).unwrap_err();
        assert!(err.contains("te groot"));
    }

    #[test]
    fn exactly_max_size_passes() {
        let mut big = b"%PDF-".to_vec();
        big.resize(MAX_FILE_BYTES, 0u8);
        assert!(validate_upload("gids.pdf", "application/pdf", &big).is_ok());
    }

    #[test]
    fn empty_filename_rejected() {
        assert!(validate_upload("", "application/pdf", PDF).is_err());
        assert!(validate_upload("   ", "application/pdf", PDF).is_err());
    }

    #[test]
    fn path_traversal_filenames_rejected() {
        for name in ["../evil.pdf", "a/b.pdf", "a\\b.pdf", "..\\up.pdf"] {
            assert!(
                validate_upload(name, "application/pdf", PDF).is_err(),
                "{name} must be rejected"
            );
        }
    }
}

use std::collections::HashSet;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::{Json, Router};
use serde::Deserialize;

use crate::rag::store::RagStore;
use crate::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/teun/documents", axum::routing::get(list_documents))
        .route(
            "/api/teun/documents/{filename}",
            axum::routing::get(get_document),
        )
        .route(
            "/api/teun/documents/{filename}/pdf",
            axum::routing::get(get_document_pdf),
        )
}

async fn list_documents(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // DB-first (CIT-02): the indexed documents ARE the corpus citations point
    // at — their `extracted_text` is what chunk line numbers refer to. The
    // resources-dir .md listing is kept as a merge fallback for legacy files
    // not (yet) in the DB (transition safety).
    let store = RagStore::new(state.pool.clone());

    let mut docs: Vec<serde_json::Value> = Vec::new();
    let mut db_names: HashSet<String> = HashSet::new();

    let db_ok = match store.list_documents().await {
        Ok(items) => {
            for item in items.into_iter().filter(|d| d.status == "indexed") {
                // line_count must match the total_lines the content endpoint
                // reports, i.e. count lines of the canonical extracted text.
                let line_count = match store.get_extracted_text(&item.filename).await {
                    Ok(Some(text)) => text.lines().count(),
                    _ => 0,
                };
                db_names.insert(item.filename.clone());
                docs.push(serde_json::json!({
                    "filename": item.filename,
                    "size_bytes": item.size_bytes,
                    "line_count": line_count,
                }));
            }
            true
        }
        Err(e) => {
            tracing::error!(error = format!("{e:#}"), "Failed to list documents from DB");
            false
        }
    };

    let resources_dir = &state.judge_config.resources_dir;
    let disk_ok = match std::fs::read_dir(resources_dir) {
        Ok(entries) => {
            let disk_docs = entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.path()
                        .extension()
                        .map(|ext| ext == "md")
                        .unwrap_or(false)
                })
                .filter_map(|e| {
                    let path = e.path();
                    let filename = path.file_name()?.to_string_lossy().to_string();
                    if db_names.contains(&filename) {
                        return None;
                    }
                    let metadata = std::fs::metadata(&path).ok()?;
                    let line_count = std::fs::read_to_string(&path)
                        .ok()
                        .map(|c| c.lines().count())
                        .unwrap_or(0);
                    Some(serde_json::json!({
                        "filename": filename,
                        "size_bytes": metadata.len(),
                        "line_count": line_count,
                    }))
                });
            docs.extend(disk_docs);
            true
        }
        Err(e) => {
            tracing::warn!(error = %e, dir = %resources_dir, "Resources dir not readable for document list fallback");
            false
        }
    };

    if !db_ok && !disk_ok {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Kon documenten niet ophalen" })),
        );
    }

    docs.sort_by(|a, b| {
        a["filename"]
            .as_str()
            .unwrap_or("")
            .cmp(b["filename"].as_str().unwrap_or(""))
    });

    (StatusCode::OK, Json(serde_json::json!(docs)))
}

#[derive(Deserialize)]
struct DocumentQuery {
    /// Optional line range to highlight, e.g. "120-135" or "42"
    #[serde(default)]
    line_range: Option<String>,
}

async fn get_document(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
    Query(query): Query<DocumentQuery>,
) -> impl IntoResponse {
    // Path traversal protection
    if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Ongeldige bestandsnaam" })),
        )
            .into_response();
    }

    // DB-first (CIT-02): serve the canonical `extracted_text` — the text the
    // chunk line numbers (and thus citations) refer to. Disk read stays as the
    // fallback for legacy .md files that aren't in the DB. A pending row that
    // hasn't been ingested yet has an empty extracted_text; treat it as absent
    // so it falls through to disk/404 instead of rendering an empty document.
    let store = RagStore::new(state.pool.clone());
    let db_text = match store.get_extracted_text(&filename).await {
        Ok(text) => text.filter(|t| !t.is_empty()),
        Err(e) => {
            tracing::error!(error = format!("{e:#}"), "Failed to get extracted_text; falling back to disk");
            None
        }
    };

    let (content, content_type) = match db_text {
        Some(text) => (text, "pdf_text"),
        None => {
            let resources_dir = &state.judge_config.resources_dir;
            let doc_path = format!("{}/{}", resources_dir, filename);
            match std::fs::read_to_string(&doc_path) {
                Ok(c) => (c, "markdown"),
                Err(_) => {
                    return (
                        StatusCode::NOT_FOUND,
                        Json(serde_json::json!({ "error": "Document niet gevonden" })),
                    )
                        .into_response();
                }
            }
        }
    };

    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    // Parse optional line range for context
    let (parsed_start, parsed_end) = query
        .line_range
        .as_deref()
        .map(parse_line_range)
        .unwrap_or((None, None));

    // Clamp to document bounds. If the requested start is past the end (e.g. stale
    // citation against a shorter, replaced document), drop the highlight so the
    // frontend can show a fallback instead of an empty yellow strip.
    let out_of_bounds = matches!(parsed_start, Some(s) if s > total_lines);
    let (highlight_start, highlight_end) = if out_of_bounds {
        (None, None)
    } else {
        (
            parsed_start.map(|s| s.max(1)),
            parsed_end.map(|e| e.min(total_lines)),
        )
    };

    // Shape-preserving (CIT-03): identical fields as before, with ONE additive
    // field (`content_type`) signalling the source so the viewer can pick a
    // sensible default view mode ("pdf_text" = extracted PDF text, not markdown).
    let response = serde_json::json!({
        "filename": filename,
        "total_lines": total_lines,
        "content": content,
        "highlight_start": highlight_start,
        "highlight_end": highlight_end,
        "highlight_out_of_bounds": out_of_bounds,
        "content_type": content_type,
    });

    (StatusCode::OK, Json(response)).into_response()
}

/// Serve the stored original PDF bytes inline from `documents.original_bytes`
/// (ADM-05). The filename doubles as the lookup key, matching the citation
/// `document` field used elsewhere.
async fn get_document_pdf(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
) -> impl IntoResponse {
    // Path traversal protection (defense in depth: the lookup is a DB equality
    // match, but reject separators outright like GET /documents/{filename}).
    if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Ongeldige bestandsnaam" })),
        )
            .into_response();
    }

    let store = RagStore::new(state.pool.clone());
    match store.get_pdf_bytes(&filename).await {
        Ok(Some(bytes)) => {
            let disposition = format!(
                "inline; filename=\"{}\"",
                sanitize_header_filename(&filename)
            );
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "application/pdf".to_string()),
                    (header::CONTENT_DISPOSITION, disposition),
                ],
                bytes,
            )
                .into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Document niet gevonden" })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = format!("{e:#}"), "Failed to get pdf bytes");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Kon document niet ophalen" })),
            )
                .into_response()
        }
    }
}

/// Make a filename safe for a quoted Content-Disposition value: header values
/// must be visible ASCII, and `"` / `\` would break out of the quoted string.
fn sanitize_header_filename(filename: &str) -> String {
    filename
        .chars()
        .filter(|c| (c.is_ascii_graphic() || *c == ' ') && *c != '"' && *c != '\\')
        .collect()
}

/// Parse a line range string like "120-135" or "42" into (start, end) 1-indexed inclusive.
fn parse_line_range(s: &str) -> (Option<usize>, Option<usize>) {
    if let Some((start, end)) = s.split_once('-') {
        let start = start.trim().parse::<usize>().ok();
        let end = end.trim().parse::<usize>().ok();
        (start, end)
    } else {
        let line = s.trim().parse::<usize>().ok();
        (line, line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_line_range_range() {
        assert_eq!(parse_line_range("120-135"), (Some(120), Some(135)));
    }

    #[test]
    fn parse_line_range_single() {
        assert_eq!(parse_line_range("42"), (Some(42), Some(42)));
    }

    #[test]
    fn parse_line_range_with_spaces() {
        assert_eq!(parse_line_range(" 10 - 20 "), (Some(10), Some(20)));
    }

    #[test]
    fn parse_line_range_invalid() {
        assert_eq!(parse_line_range("abc"), (None, None));
    }

    #[test]
    fn parse_line_range_partial_invalid() {
        assert_eq!(parse_line_range("10-abc"), (Some(10), None));
    }

    #[test]
    fn parse_line_range_empty() {
        assert_eq!(parse_line_range(""), (None, None));
    }

    #[test]
    fn path_traversal_dotdot() {
        let filename = "../etc/passwd";
        assert!(filename.contains(".."));
    }

    #[test]
    fn path_traversal_slash() {
        let filename = "foo/bar.md";
        assert!(filename.contains('/'));
    }

    #[test]
    fn path_traversal_backslash() {
        let filename = "foo\\bar.md";
        assert!(filename.contains('\\'));
    }

    #[test]
    fn valid_filename_passes() {
        let filename = "acceptatiebeleid.md";
        assert!(!filename.contains("..") && !filename.contains('/') && !filename.contains('\\'));
    }

    #[test]
    fn sanitize_header_filename_keeps_normal_names() {
        assert_eq!(
            sanitize_header_filename("MUNT Hypotheekgids 2026-2.pdf"),
            "MUNT Hypotheekgids 2026-2.pdf"
        );
    }

    #[test]
    fn sanitize_header_filename_strips_quotes_and_controls() {
        assert_eq!(
            sanitize_header_filename("a\"b\\c\r\nd\u{1F600}.pdf"),
            "abcd.pdf"
        );
    }
}

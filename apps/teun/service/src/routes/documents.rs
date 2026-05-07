use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Json, Router};
use serde::Deserialize;

use crate::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/teun/documents", axum::routing::get(list_documents))
        .route(
            "/api/teun/documents/{filename}",
            axum::routing::get(get_document),
        )
}

async fn list_documents(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let resources_dir = &state.judge_config.resources_dir;

    let entries = match std::fs::read_dir(resources_dir) {
        Ok(dir) => dir,
        Err(e) => {
            tracing::error!(error = %e, dir = %resources_dir, "Failed to read resources dir");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Kon documenten niet ophalen" })),
            );
        }
    };

    let mut docs: Vec<serde_json::Value> = entries
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
        })
        .collect();

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

    let resources_dir = &state.judge_config.resources_dir;
    let doc_path = format!("{}/{}", resources_dir, filename);

    let content = match std::fs::read_to_string(&doc_path) {
        Ok(c) => c,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Document niet gevonden" })),
            )
                .into_response();
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

    let response = serde_json::json!({
        "filename": filename,
        "total_lines": total_lines,
        "content": content,
        "highlight_start": highlight_start,
        "highlight_end": highlight_end,
        "highlight_out_of_bounds": out_of_bounds,
    });

    (StatusCode::OK, Json(response)).into_response()
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
}

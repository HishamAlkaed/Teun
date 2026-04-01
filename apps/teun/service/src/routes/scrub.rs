use std::sync::Arc;

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::AppState;

#[derive(Deserialize)]
pub struct ScrubRequest {
    text: String,
}

#[derive(Serialize)]
pub struct ScrubResponse {
    scrubbed_text: String,
    entities: Vec<DetectedEntity>,
}

#[derive(Serialize)]
pub struct DetectedEntity {
    entity_type: String,
    original: String,
    placeholder: String,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/teun/scrub", post(scrub))
}

async fn scrub(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ScrubRequest>,
) -> Result<Json<ScrubResponse>, AppError> {
    let scrub_url = state
        .scrub_service_url
        .as_ref()
        .ok_or_else(|| AppError::BadRequest("PII scrub service URL not configured".to_string()))?;

    let mut request = state
        .http_client
        .post(format!("{}/api/v1/scrub", scrub_url))
        .json(&serde_json::json!({ "text": req.text, "language": "nl" }));

    // Attach bearer token if OAuth2 credentials are configured
    if let Some(ref token_client) = state.scrub_token_client {
        let token = token_client.get_token().await.map_err(|e| {
            tracing::error!(error = %e, "Failed to get scrub auth token");
            AppError::Internal(anyhow::anyhow!("Failed to get scrub auth token: {e}"))
        })?;
        request = request.bearer_auth(token);
    }

    let resp = request.send().await.map_err(|e| {
        tracing::error!(error = %e, "Scrub service request failed");
        AppError::Internal(anyhow::anyhow!("Scrub service request failed: {e}"))
    })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        tracing::error!(%status, %body, "Scrub service returned error");
        return Err(AppError::Internal(anyhow::anyhow!(
            "Scrub service returned {status}: {body}"
        )));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Invalid scrub response: {e}")))?;

    let scrubbed_text = body["scrubbed_text"]
        .as_str()
        .unwrap_or(&req.text)
        .to_string();

    let entities: Vec<DetectedEntity> = body["token_map"]
        .as_object()
        .map(|map| {
            map.iter()
                .map(|(placeholder, original)| DetectedEntity {
                    entity_type: extract_entity_type(placeholder),
                    original: original.as_str().unwrap_or("").to_string(),
                    placeholder: placeholder.clone(),
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(Json(ScrubResponse {
        scrubbed_text,
        entities,
    }))
}

/// Extract entity type from placeholder like "[BSN_1]" -> "BSN"
fn extract_entity_type(placeholder: &str) -> String {
    let inner = placeholder.trim_start_matches('[').trim_end_matches(']');
    if let Some(pos) = inner.rfind('_') {
        if inner[pos + 1..].chars().all(|c| c.is_ascii_digit()) {
            return inner[..pos].to_string();
        }
    }
    inner.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_entity_type_bsn() {
        assert_eq!(extract_entity_type("[BSN_1]"), "BSN");
    }

    #[test]
    fn extract_entity_type_phone() {
        assert_eq!(extract_entity_type("[PHONE_42]"), "PHONE");
    }

    #[test]
    fn extract_entity_type_multi_word() {
        assert_eq!(extract_entity_type("[CREDIT_CARD_3]"), "CREDIT_CARD");
    }

    #[test]
    fn extract_entity_type_no_number_suffix() {
        assert_eq!(extract_entity_type("[EMAIL]"), "EMAIL");
    }

    #[test]
    fn extract_entity_type_no_brackets() {
        assert_eq!(extract_entity_type("BSN_1"), "BSN");
    }

    #[test]
    fn extract_entity_type_trailing_non_digit() {
        assert_eq!(extract_entity_type("[FOO_bar]"), "FOO_bar");
    }

    #[test]
    fn extract_entity_type_empty_brackets() {
        assert_eq!(extract_entity_type("[]"), "");
    }
}

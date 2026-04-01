use std::sync::Arc;

use axum::extract::Path;
use axum::http::StatusCode;
use axum::{Json, Router, extract::State, routing::{get, put}};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::error::AppError;
use crate::session::MessageFeedback;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/teun/sessions", get(list_sessions))
        .route("/api/teun/sessions/{id}", get(get_session).delete(delete_session))
        .route(
            "/api/teun/sessions/{session_id}/messages/{message_id}/feedback",
            put(set_feedback),
        )
}

#[derive(Serialize)]
struct SessionListItem {
    id: String,
    title: String,
    created_at: String,
    last_active: String,
}

async fn list_sessions(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<SessionListItem>>, AppError> {
    let sessions = state.sessions.list().await.map_err(|e| AppError::Internal(e))?;

    let items: Vec<SessionListItem> = sessions
        .into_iter()
        .map(|s| SessionListItem {
            id: s.id,
            title: s.title,
            created_at: s.created_at.to_rfc3339(),
            last_active: s.last_active.to_rfc3339(),
        })
        .collect();

    Ok(Json(items))
}

async fn get_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let session = state
        .sessions
        .get(&id)
        .await
        .map_err(|e| AppError::Internal(e))?;

    match session {
        Some(s) => {
            let value = serde_json::json!({
                "id": s.id,
                "title": s.title,
                "created_at": s.created_at.to_rfc3339(),
                "last_active": s.last_active.to_rfc3339(),
                "messages": s.messages,
            });
            Ok(Json(value))
        }
        None => Err(AppError::NotFound),
    }
}

async fn delete_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let deleted = state
        .sessions
        .delete(&id)
        .await
        .map_err(|e| AppError::Internal(e))?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum FeedbackStatus {
    Approved,
    Partial,
    Rejected,
}

#[derive(Deserialize)]
struct FeedbackRequest {
    status: FeedbackStatus,
    comment: Option<String>,
}

async fn set_feedback(
    State(state): State<Arc<AppState>>,
    Path((session_id, message_id)): Path<(String, String)>,
    Json(req): Json<FeedbackRequest>,
) -> Result<StatusCode, AppError> {
    let status_str = match req.status {
        FeedbackStatus::Approved => "approved",
        FeedbackStatus::Partial => "partial",
        FeedbackStatus::Rejected => "rejected",
    };
    let comment = req.comment.map(|c| {
        if c.chars().count() > 2000 {
            c.chars().take(2000).collect()
        } else {
            c
        }
    });

    let feedback = MessageFeedback {
        status: status_str.to_string(),
        comment,
        created_at: Utc::now(),
    };

    let updated = state
        .sessions
        .set_message_feedback(&session_id, &message_id, feedback)
        .await
        .map_err(|e| AppError::Internal(e))?;

    if updated {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}

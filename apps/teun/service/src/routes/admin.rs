use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use sqlx::Row;
use uuid::Uuid;

use crate::eval::judge;
use crate::eval::runner;
use crate::eval::types::{EvalRun, EvalRunStatus};
use crate::AppState;

const APP_ID: &str = "teun";

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        // Questions
        .route(
            "/api/teun/questions",
            axum::routing::get(list_questions).post(create_question),
        )
        .route(
            "/api/teun/questions/{id}",
            axum::routing::put(update_question).delete(delete_question),
        )
        // Eval
        .route(
            "/api/teun/eval/runs",
            axum::routing::post(start_eval).get(list_runs),
        )
        .route("/api/teun/eval/runs/{id}", axum::routing::get(get_run))
        .route(
            "/api/teun/eval/runs/{id}/stop",
            axum::routing::post(stop_eval),
        )
        // Feedback
        .route("/api/teun/feedback/stats", axum::routing::get(get_feedback_stats))
        .route("/api/teun/feedback/recent", axum::routing::get(get_recent_feedback))
}

// ── Questions ──────────────────────────────────────────────────────

#[derive(Deserialize)]
struct QuestionPayload {
    question: String,
    expected_category: Option<String>,
    #[serde(default)]
    expected_key_points: Vec<String>,
    description: Option<String>,
    #[serde(default = "default_points")]
    points: u16,
}

fn default_points() -> u16 {
    1
}

async fn list_questions(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.eval_store.list_questions().await {
        Ok(questions) => {
            let items: Vec<_> = questions
                .into_iter()
                .map(|q| {
                    serde_json::json!({
                        "id": q.id,
                        "question": q.question,
                        "expected_category": q.expected_category,
                        "expected_key_points": q.expected_key_points,
                        "description": q.description,
                        "points": q.points,
                        "created_at": q.created_at.to_rfc3339(),
                        "updated_at": q.updated_at.to_rfc3339(),
                    })
                })
                .collect();
            (StatusCode::OK, Json(serde_json::json!(items)))
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to list questions");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Kon vragen niet ophalen" })),
            )
        }
    }
}

async fn create_question(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<QuestionPayload>,
) -> impl IntoResponse {
    if payload.question.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Vraag mag niet leeg zijn" })),
        )
            .into_response();
    }

    let now = Utc::now();
    let question = crate::eval::types::TestQuestion {
        id: Uuid::new_v4().to_string(),
        app_id: APP_ID.to_string(),
        question: payload.question,
        expected_category: payload.expected_category,
        expected_key_points: payload.expected_key_points,
        description: payload.description,
        points: payload.points,
        created_at: now,
        updated_at: now,
    };

    match state.eval_store.create_question(&question).await {
        Ok(()) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "id": question.id,
                "question": question.question,
                "expected_category": question.expected_category,
                "expected_key_points": question.expected_key_points,
                "description": question.description,
                "points": question.points,
                "created_at": question.created_at.to_rfc3339(),
                "updated_at": question.updated_at.to_rfc3339(),
            })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "Failed to create question");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Kon vraag niet aanmaken" })),
            )
                .into_response()
        }
    }
}

async fn update_question(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(payload): Json<QuestionPayload>,
) -> impl IntoResponse {
    if payload.question.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Vraag mag niet leeg zijn" })),
        )
            .into_response();
    }

    match state
        .eval_store
        .update_question(
            &id,
            &payload.question,
            payload.expected_category.as_deref(),
            &payload.expected_key_points,
            payload.description.as_deref(),
            payload.points,
        )
        .await
    {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Vraag niet gevonden" })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "Failed to update question");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Kon vraag niet bijwerken" })),
            )
                .into_response()
        }
    }
}

async fn delete_question(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.eval_store.delete_question(&id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Vraag niet gevonden" })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "Failed to delete question");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Kon vraag niet verwijderen" })),
            )
                .into_response()
        }
    }
}

// ── Eval ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Default)]
struct StartEvalRequest {
    #[serde(default)]
    mode: Option<String>,
}

async fn start_eval(
    State(state): State<Arc<AppState>>,
    body: Option<Json<StartEvalRequest>>,
) -> impl IntoResponse {
    let mode_override = body.and_then(|b| b.0.mode);

    let anthropic_api_key = match &state.judge_config.anthropic_api_key {
        Some(key) => key.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "ANTHROPIC_API_KEY is niet geconfigureerd"
                })),
            );
        }
    };

    let questions = match state.eval_store.list_questions().await {
        Ok(q) if q.is_empty() => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "Geen evaluatievragen geconfigureerd"
                })),
            );
        }
        Ok(q) => q,
        Err(e) => {
            tracing::error!(error = %e, "Failed to list questions");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Kon vragen niet ophalen" })),
            );
        }
    };

    let mode = mode_override.unwrap_or_else(|| "tools".to_string());

    let run = EvalRun {
        id: Uuid::new_v4().to_string(),
        app_id: APP_ID.to_string(),
        status: EvalRunStatus::Pending,
        started_by: "admin".to_string(),
        created_at: Utc::now(),
        completed_at: None,
        total_questions: questions.len() as u32,
        completed_questions: 0,
        results: vec![],
        summary: None,
        error: None,
    };

    if let Err(e) = state.eval_store.create_run(&run).await {
        tracing::error!(error = %e, "Failed to create eval run");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Kon evaluatie niet starten" })),
        );
    }

    let run_id = run.id.clone();
    let eval_store = state.eval_store.clone();
    let http_client = state.http_client.clone();
    let judge_model = state.judge_config.judge_model.clone();
    let judge_template = judge::DEFAULT_JUDGE_TEMPLATE.to_string();

    // Chat API base = ourselves (localhost on the same port)
    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(3000);
    let chat_api_base = format!("http://127.0.0.1:{port}");

    tokio::spawn(async move {
        runner::run_eval(
            run_id,
            questions,
            eval_store,
            http_client,
            chat_api_base,
            anthropic_api_key,
            judge_model,
            judge_template,
            mode,
        )
        .await;
    });

    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "id": run.id,
            "status": "pending"
        })),
    )
}

async fn list_runs(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.eval_store.list_runs(20).await {
        Ok(runs) => {
            let items: Vec<_> = runs
                .into_iter()
                .map(|r| {
                    serde_json::json!({
                        "id": r.id,
                        "status": r.status,
                        "started_by": r.started_by,
                        "created_at": r.created_at.to_rfc3339(),
                        "completed_at": r.completed_at.map(|d| d.to_rfc3339()),
                        "total_questions": r.total_questions,
                        "completed_questions": r.completed_questions,
                        "summary": r.summary,
                    })
                })
                .collect();
            (StatusCode::OK, Json(serde_json::json!(items)))
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to list eval runs");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Kon evaluaties niet ophalen" })),
            )
        }
    }
}

async fn get_run(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.eval_store.get_run(&id).await {
        Ok(Some(run)) => (StatusCode::OK, Json(serde_json::json!(run))).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Evaluatie niet gevonden" })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "Failed to get eval run");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Kon evaluatie niet ophalen" })),
            )
                .into_response()
        }
    }
}

async fn stop_eval(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.eval_store.stop_run(&id).await {
        Ok(true) => (
            StatusCode::OK,
            Json(serde_json::json!({ "status": "stopped" })),
        )
            .into_response(),
        Ok(false) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "Evaluatie is niet actief" })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "Failed to stop eval run");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Kon evaluatie niet stoppen" })),
            )
                .into_response()
        }
    }
}

// ── Feedback ───────────────────────────────────────────────────────

async fn get_feedback_stats(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let counts = sqlx::query(
        "SELECT feedback_status, COUNT(*) as count \
         FROM messages \
         WHERE role = 'assistant' AND feedback_status IS NOT NULL \
         GROUP BY feedback_status",
    )
    .fetch_all(&state.pool)
    .await;

    let counts = match counts {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "Failed to aggregate feedback stats");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Kon feedback niet ophalen" })),
            );
        }
    };

    let mut approved = 0i64;
    let mut partial = 0i64;
    let mut rejected = 0i64;
    for row in &counts {
        let status: Option<String> = row.get("feedback_status");
        let count: i64 = row.get("count");
        match status.as_deref() {
            Some("approved") => approved = count,
            Some("partial") => partial = count,
            Some("rejected") => rejected = count,
            _ => {}
        }
    }
    let total = approved + partial + rejected;

    // 7-day trend
    let trend_rows = sqlx::query(
        "SELECT DATE(feedback_created_at) as date, feedback_status, COUNT(*) as count \
         FROM messages \
         WHERE role = 'assistant' AND feedback_status IS NOT NULL \
           AND feedback_created_at >= now() - interval '7 days' \
         GROUP BY DATE(feedback_created_at), feedback_status \
         ORDER BY DATE(feedback_created_at)",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let trend: Vec<serde_json::Value> = trend_rows
        .iter()
        .map(|r| {
            let date: chrono::NaiveDate = r.get("date");
            let status: Option<String> = r.get("feedback_status");
            let count: i64 = r.get("count");
            serde_json::json!({
                "date": date.to_string(),
                "status": status,
                "count": count,
            })
        })
        .collect();

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "total": total,
            "approved": approved,
            "partial": partial,
            "rejected": rejected,
            "approval_rate": if total > 0 { approved as f64 / total as f64 * 100.0 } else { 0.0 },
            "partial_rate": if total > 0 { partial as f64 / total as f64 * 100.0 } else { 0.0 },
            "rejection_rate": if total > 0 { rejected as f64 / total as f64 * 100.0 } else { 0.0 },
            "trend": trend
        })),
    )
}

async fn get_recent_feedback(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT m.id as message_id, m.session_id, s.title as session_title, \
                m.content as answer, m.structured_answer, \
                m.feedback_status, m.feedback_comment, m.feedback_created_at, \
                m.timestamp \
         FROM messages m \
         JOIN sessions s ON m.session_id = s.id \
         WHERE m.role = 'assistant' AND m.feedback_status IN ('rejected', 'partial') \
         ORDER BY m.feedback_created_at DESC \
         LIMIT 20",
    )
    .fetch_all(&state.pool)
    .await;

    match rows {
        Ok(rows) => {
            let items: Vec<serde_json::Value> = rows
                .iter()
                .map(|r| {
                    let fb_at: Option<DateTime<Utc>> = r.get("feedback_created_at");
                    let ts: DateTime<Utc> = r.get("timestamp");
                    serde_json::json!({
                        "session_id": r.get::<String, _>("session_id"),
                        "session_title": r.get::<String, _>("session_title"),
                        "message_id": r.get::<String, _>("message_id"),
                        "answer": r.get::<String, _>("answer"),
                        "structured_answer": r.get::<Option<serde_json::Value>, _>("structured_answer"),
                        "feedback_status": r.get::<Option<String>, _>("feedback_status"),
                        "feedback_comment": r.get::<Option<String>, _>("feedback_comment"),
                        "feedback_at": fb_at.map(|d| d.to_rfc3339()),
                        "timestamp": ts.to_rfc3339(),
                    })
                })
                .collect();
            (StatusCode::OK, Json(serde_json::json!(items)))
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to get recent feedback");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Kon feedback niet ophalen" })),
            )
        }
    }
}

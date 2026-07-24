use std::convert::Infallible;
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::State,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::post,
};
use chrono::Utc;
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::Instrument;

use crate::AppState;
use crate::agent::rag::{RagConfig, run_rag};
use crate::agent::types::ChatEvent;
use crate::error::AppError;
use crate::judge;
use crate::rag::embed::EmbedConfig;
use crate::rag::store::RagStore;
use crate::session::StoredMessage;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/teun/chat", post(chat))
}

#[derive(Deserialize)]
struct ChatRequest {
    message: String,
    session_id: Option<String>,
    #[serde(default = "default_mode")]
    mode: String,
    #[serde(default = "default_language")]
    language: String,
    #[serde(default = "default_search_depth")]
    search_depth: String,
    /// When true, skip persisting this conversation to the sessions store.
    /// Used by the eval runner to keep eval questions out of the chat history.
    #[serde(default)]
    skip_persist: bool,
}

fn default_mode() -> String {
    "tools".to_string()
}

fn default_language() -> String {
    "nl".to_string()
}

fn default_search_depth() -> String {
    "quick".to_string()
}

#[tracing::instrument(
    name = "http.chat",
    skip(state, req),
    fields(
        chat.mode = %req.mode,
        chat.message_len = req.message.len(),
    )
)]
async fn chat(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ChatRequest>,
) -> Result<Response, AppError> {
    let message_len = req.message.len();
    tracing::info!(message_len, session_id = ?req.session_id, "Received chat request");

    if req.message.trim().is_empty() {
        return Err(AppError::BadRequest("message is required".to_string()));
    }

    if message_len > 10_000 {
        return Err(AppError::BadRequest(
            "message exceeds maximum length of 10000 characters".to_string(),
        ));
    }

    // The client-provided session id (kept for session persistence and the
    // Langfuse session tag; the single-call RAG path has no resume concept).
    let client_session_id = req.session_id.clone();

    // Generate the assistant message ID before the stream starts so we can
    // send it to the frontend in the Result event for feedback API calls.
    let assistant_msg_id = uuid::Uuid::new_v4().to_string();

    let (tx, rx) = mpsc::channel::<ChatEvent>(32);

    // Both legacy modes ("tools" and "inline") now route to the single RAG
    // answer path; `mode` is only read for backward-compat logging.
    let rag_config = RagConfig::from_env()
        .map_err(|e| AppError::from(e.context("RAG answer path not configured")))?;
    let embed_cfg = EmbedConfig::from_env()
        .map_err(|e| AppError::from(e.context("Embeddings provider not configured")))?;
    let rag_store = RagStore::new(state.pool.clone());

    // If user selected a non-default language, prepend instruction to the agent message
    let message = if req.language != "nl" {
        format!("[Antwoord in het Engels / Respond in English]\n\n{}", req.message)
    } else {
        req.message.clone()
    };

    // NOTE (deliberate behavior change, Plan 01-04): the search_depth
    // "SNELLE MODUS"/"UITGEBREIDE MODUS" prompt prepend and the judge
    // score-based retry loop are REMOVED — the RAG path is a single call
    // with no tools, so depth instructions are meaningless and there is no
    // re-runnable agent to retry. search_depth is still accepted (and
    // logged) for frontend backward compatibility.
    let user_message = req.message.clone();
    let mode = req.mode.clone();
    let skip_persist = req.skip_persist;

    tracing::info!(
        mode = %mode,
        search_depth = %req.search_depth,
        session_id = ?client_session_id,
        skip_persist,
        "Starting chat (RAG path)"
    );

    // Unbounded channel for persistence — never drops events
    let (persist_tx, mut persist_rx) = mpsc::unbounded_channel::<ChatEvent>();

    // Intermediate channel: agent → forwarder → SSE + persistence
    // This decouples the agent from the SSE connection so it continues
    // even if the browser navigates away.
    let (agent_tx, mut agent_rx) = mpsc::channel::<ChatEvent>(32);
    let sse_tx = tx.clone();
    let fwd_persist_tx = persist_tx.clone();
    tokio::spawn(async move {
        while let Some(event) = agent_rx.recv().await {
            // Always persist (unbounded, never fails)
            let _ = fwd_persist_tx.send(event.clone());
            // Best-effort SSE delivery — if browser disconnected, just skip
            let _ = sse_tx.send(event).await;
        }
    });

    // Spawn the agent + judge pipeline in a background task
    let tx_clone = agent_tx;
    let http_client = state.http_client.clone();
    let judge_config = &state.judge_config;
    let judge_model = judge_config.judge_model.clone();
    let judge_resources_dir = judge_config.resources_dir.clone();
    let judge_api_key = judge_config.anthropic_api_key.clone();
    let judge_timeout = judge_config.timeout_secs;
    let judge_retry_threshold = judge_config.retry_threshold;
    let judge_max_retries = judge_config.max_retries;
    // One Langfuse trace per chat turn. Tagged with the app name so it can be
    // told apart from the other apps sharing this Langfuse project. The agent
    // and judge generation spans nest under this via `.instrument`.
    let turn_span = tracing::info_span!(
        "teun.chat_turn",
        langfuse.trace.tags = "[\"teun\"]",
        langfuse.session.id = tracing::field::Empty,
        chat.mode = %mode,
    );
    if let Some(sid) = client_session_id.as_deref() {
        turn_span.record("langfuse.session.id", sid);
    }

    tokio::spawn(
        async move {
        let judge_cfg = judge::JudgeConfig {
            anthropic_api_key: judge_api_key,
            judge_model,
            resources_dir: judge_resources_dir,
            timeout_secs: judge_timeout,
            retry_threshold: judge_retry_threshold,
            max_retries: judge_max_retries,
        };

        // Single RAG pass with a transient-error retry wrapper. The old judge
        // score-based retry loop (search_depth=uitgebreid) is deliberately
        // gone: every request is one run_rag pass + one judge call.
        const MAX_ERROR_RETRIES: u8 = 2;
        let mut attempt = 0u8;
        let rag_result = loop {
            let result = run_rag(
                &http_client,
                &rag_store,
                &embed_cfg,
                &rag_config,
                &message,
                tx_clone.clone(),
            )
            .await;
            match result {
                Ok(r) => break Ok(r),
                Err(e) => {
                    attempt += 1;
                    let error_detail = format!("{e:#}");
                    if attempt <= MAX_ERROR_RETRIES {
                        tracing::warn!(attempt, error = %error_detail, "RAG agent failed, retrying");
                        let _ = tx_clone.send(ChatEvent::Thinking {
                            content: format!("Er ging iets mis, nieuwe poging ({attempt}/{MAX_ERROR_RETRIES})..."),
                        }).await;
                        // Brief pause before retry
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        continue;
                    }
                    break Err(e);
                }
            }
        };

        match rag_result {
            Ok((Some(answer), evidence)) => {
                tracing::info!("Running judge (single pass)");
                // Option-A wiring: the judge verifies citations against the
                // chunk-derived evidence returned by run_rag — never
                // ToolEvidence::default().
                let judge_result =
                    judge::run_judge(&http_client, &judge_cfg, &message, &answer, &evidence).await;
                tracing::info!(
                    score = judge_result.score,
                    sources_verified = judge_result.sources_verified,
                    sources_total = judge_result.sources_total,
                    llm_error = ?judge_result.llm_error,
                    "Judge complete"
                );
                let _ = tx_clone.send(ChatEvent::Judge { result: judge_result }).await;
            }
            Ok((None, _)) => tracing::debug!("No structured answer produced, skipping judge"),
            Err(e) => {
                let error_detail = format!("{e:#}");
                tracing::error!(error = %error_detail, "RAG agent failed after retries");
                let _ = tx_clone.send(ChatEvent::Error {
                    message: format!(
                        "Teun heeft op dit moment een storing. Probeer het later nog eens. Error: {error_detail}"
                    ),
                }).await;
            }
        }
        }
        .instrument(turn_span),
    );

    // Clone for SSE injection before persist task moves the original
    let assistant_msg_id_sse = assistant_msg_id.clone();
    let client_session_id_sse = req.session_id.clone();

    // Spawn a task to persist the session after the stream completes
    let sessions = state.sessions.clone();
    let existing_session_id = req.session_id.clone();
    tokio::spawn(async move {
        let mut events: Vec<ChatEvent> = Vec::new();
        while let Some(event) = persist_rx.recv().await {
            events.push(event);
        }

        // Skip persistence for eval requests — keep eval questions out of chat history
        if skip_persist {
            tracing::debug!("Skipping session persistence (skip_persist=true)");
            return;
        }

        // Prefer the client-provided session id so follow-ups extend the same
        // session record. Inline mode's Result event contains a per-request
        // Anthropic message id, not a stable conversation id, so falling back
        // to it would fork a new session each turn.
        let session_id = existing_session_id
            .or_else(|| {
                events.iter().find_map(|e| {
                    if let ChatEvent::Result { session_id, .. } = e {
                        if !session_id.is_empty() {
                            return Some(session_id.clone());
                        }
                    }
                    None
                })
            })
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        // Build title from user message (first 80 chars)
        let title: String = user_message.chars().take(80).collect();

        // Build user message
        let user_msg = StoredMessage {
            id: uuid::Uuid::new_v4().to_string(),
            role: "user".to_string(),
            content: user_message,
            structured_answer: None,
            timestamp: Utc::now(),
            timeline: None,
            feedback: None,
            judge_result: None,
        };

        // Build assistant message from events
        let mut content = String::new();
        let mut structured_answer: Option<serde_json::Value> = None;
        let mut judge_result: Option<serde_json::Value> = None;
        let mut timeline_entries: Vec<serde_json::Value> = Vec::new();

        for event in &events {
            match event {
                ChatEvent::Thinking { content: c } => {
                    timeline_entries.push(serde_json::json!({
                        "type": "thinking",
                        "data": { "content": c }
                    }));
                }
                ChatEvent::ToolUse { tool, input, content: tool_content } => {
                    timeline_entries.push(serde_json::json!({
                        "type": "tool_use",
                        "data": { "tool": tool, "input": input, "content": tool_content }
                    }));
                }
                ChatEvent::Partial { content: c } => {
                    content = c.clone();
                    timeline_entries.push(serde_json::json!({
                        "type": "partial",
                        "data": { "content": c }
                    }));
                }
                ChatEvent::Result { structured_output, .. } => {
                    structured_answer = Some(structured_output.clone());
                    timeline_entries.push(serde_json::json!({
                        "type": "result",
                        "data": { "structured_output": structured_output }
                    }));
                }
                ChatEvent::Error { message } => {
                    timeline_entries.push(serde_json::json!({
                        "type": "error",
                        "data": { "message": message }
                    }));
                }
                ChatEvent::Judge { result } => {
                    let result_value = serde_json::to_value(result).unwrap_or_default();
                    judge_result = Some(result_value.clone());
                    timeline_entries.push(serde_json::json!({
                        "type": "judge",
                        "data": result_value
                    }));
                }
            }
        }

        let assistant_msg = StoredMessage {
            id: assistant_msg_id,
            role: "assistant".to_string(),
            content,
            structured_answer,
            timestamp: Utc::now(),
            timeline: Some(serde_json::to_value(&timeline_entries).unwrap_or_default()),
            feedback: None,
            judge_result,
        };

        // Atomically append messages — no read-modify-write race condition
        if let Err(e) = sessions
            .append_messages(&session_id, &title, vec![user_msg, assistant_msg])
            .await
        {
            tracing::error!(error = %e, session_id = %session_id, "Failed to persist session");
        } else {
            tracing::info!(session_id = %session_id, "Session persisted");
        }
    });

    // Convert the channel into an SSE stream
    let stream = ReceiverStream::new(rx).map(move |event| {
        let event_type = match &event {
            ChatEvent::Thinking { .. } => "thinking",
            ChatEvent::ToolUse { .. } => "tool_use",
            ChatEvent::Partial { .. } => "partial",
            ChatEvent::Result { .. } => "result",
            ChatEvent::Error { .. } => "error",
            ChatEvent::Judge { .. } => "judge",
        };

        // Inject message_id into Result events so the frontend can use it
        // for feedback API calls without requiring a page refresh. Echo the
        // client's session id when one was provided so follow-ups don't flip
        // the URL — inline mode's Result session_id is a per-request msg id.
        let data = if let ChatEvent::Result { structured_output, session_id } = &event {
            let sid = client_session_id_sse
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| session_id.clone());
            serde_json::json!({
                "type": "result",
                "data": {
                    "structured_output": structured_output,
                    "session_id": sid,
                    "message_id": assistant_msg_id_sse,
                }
            }).to_string()
        } else {
            serde_json::to_string(&event).unwrap_or_default()
        };
        Ok::<_, Infallible>(Event::default().event(event_type).data(data))
    });

    let mut resp = Sse::new(stream).keep_alive(KeepAlive::default()).into_response();
    let headers = resp.headers_mut();
    headers.insert("x-accel-buffering", "no".parse().unwrap());
    headers.insert("cache-control", "no-cache".parse().unwrap());
    Ok(resp)
}

// Helper to use tokio_stream::StreamExt::map
use tokio_stream::StreamExt;

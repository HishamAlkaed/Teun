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

use crate::AppState;
use crate::agent::claude::{ClaudeConfig, run_claude};
use crate::agent::inline::{InlineConfig, run_inline};
use crate::agent::types::{ChatEvent, ToolEvidence};
use crate::error::AppError;
use crate::judge;
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

    // Only pass session_id to --resume if the client provided one (from a previous Claude response)
    let claude_session_id = req.session_id.clone();

    // Generate the assistant message ID before the stream starts so we can
    // send it to the frontend in the Result event for feedback API calls.
    let assistant_msg_id = uuid::Uuid::new_v4().to_string();

    let (tx, rx) = mpsc::channel::<ChatEvent>(32);

    let config = ClaudeConfig::from_env();
    let inline_config = if req.mode == "inline" {
        match InlineConfig::from_env() {
            Ok(c) => Some(c),
            Err(e) => {
                return Err(e.context("Inline mode not available").into());
            }
        }
    } else {
        None
    };
    // If user selected a non-default language, prepend instruction to the agent message
    let mut message = if req.language != "nl" {
        format!("[Antwoord in het Engels / Respond in English]\n\n{}", req.message)
    } else {
        req.message.clone()
    };

    // Prepend search depth instruction for tools mode
    let is_quick_search = req.search_depth == "quick";
    if req.mode != "inline" {
        let depth_instruction = if req.search_depth == "quick" {
            "[BELANGRIJK - SNELLE MODUS: Je mag MAXIMAAL 3 zoekopdrachten uitvoeren. Formuleer daarna direct een antwoord. Doe GEEN verdere zoekopdrachten na je derde search. Als de resultaten onvoldoende zijn, geef dan aan wat je wel hebt gevonden en dat uitgebreider zoeken meer informatie kan opleveren.]"
        } else {
            "[UITGEBREIDE MODUS: Zoek grondig en volledig. Voer meerdere zoekopdrachten uit met verschillende zoektermen om alle relevante informatie te vinden. Controleer je antwoord door aanvullende bronnen te raadplegen. Neem de tijd om een compleet en goed onderbouwd antwoord te geven.]"
        };
        message = format!("{}\n\n{}", depth_instruction, message);
    }
    let user_message = req.message.clone();
    let mode = req.mode.clone();
    let skip_persist = req.skip_persist;

    tracing::info!(mode = %mode, claude_session_id = ?claude_session_id, skip_persist, "Starting chat");

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
    let judge_max_retries = if is_quick_search { 0 } else { judge_config.max_retries };
    tokio::spawn(async move {
        let judge_cfg = judge::JudgeConfig {
            anthropic_api_key: judge_api_key,
            judge_model,
            resources_dir: judge_resources_dir,
            timeout_secs: judge_timeout,
            retry_threshold: judge_retry_threshold,
            max_retries: judge_max_retries,
        };

        let is_inline = inline_config.is_some();

        // Inline mode: single pass, then judge (no retries)
        if is_inline {
            const MAX_INLINE_RETRIES: u8 = 2;
            let mut inline_attempt = 0u8;
            let inline_result = loop {
                let result = run_inline(&http_client, inline_config.as_ref().unwrap(), &message, tx_clone.clone()).await;
                match result {
                    Ok(r) => break Ok(r),
                    Err(e) => {
                        inline_attempt += 1;
                        let error_detail = format!("{e:#}");
                        if inline_attempt <= MAX_INLINE_RETRIES {
                            tracing::warn!(attempt = inline_attempt, error = %error_detail, "Inline agent failed, retrying");
                            let _ = tx_clone.send(ChatEvent::Thinking {
                                content: format!("Er ging iets mis, nieuwe poging ({inline_attempt}/{MAX_INLINE_RETRIES})..."),
                            }).await;
                            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                            continue;
                        }
                        break Err(e);
                    }
                }
            };

            match inline_result {
                Ok(Some(answer)) => {
                    tracing::info!("Inline mode — running judge (no retries)");
                    let no_evidence = ToolEvidence::default();
                    let judge_result =
                        judge::run_judge(&http_client, &judge_cfg, &message, &answer, &no_evidence).await;
                    tracing::info!(
                        score = judge_result.score,
                        sources_verified = judge_result.sources_verified,
                        sources_total = judge_result.sources_total,
                        llm_error = ?judge_result.llm_error,
                        "Inline judge complete"
                    );
                    let _ = tx_clone.send(ChatEvent::Judge { result: judge_result }).await;
                }
                Ok(None) => tracing::debug!("No structured answer produced"),
                Err(e) => {
                    let error_detail = format!("{e:#}");
                    tracing::error!(error = %error_detail, "Inline agent failed after retries");
                    let _ = tx_clone.send(ChatEvent::Error {
                        message: format!(
                            "Teun heeft op dit moment een storing. Probeer het later nog eens. Error: {error_detail}"
                        ),
                    }).await;
                }
            }
        } else {
            // Tools mode: agent → judge with retry loop
            let mut attempt = 0u8;
            let mut best_judge: Option<judge::types::JudgeResult> = None;
            let mut best_score: Option<u8> = None;
            const MAX_ERROR_RETRIES: u8 = 2;

            loop {
                let agent_message = if attempt == 0 {
                    message.clone()
                } else {
                    let prev_reasoning = best_judge
                        .as_ref()
                        .and_then(|j| j.reasoning.as_deref())
                        .unwrap_or("geen");
                    format!(
                        "{}\n\n[Vorige poging scoorde {}/100. Feedback: {}. Verbeter je antwoord op basis van deze feedback.]",
                        message,
                        best_score.unwrap_or(0),
                        prev_reasoning,
                    )
                };

                let result = run_claude(&agent_message, claude_session_id.as_deref(), &config, tx_clone.clone()).await;

                match result {
                    Ok((Some(answer), tool_evidence)) => {
                        tracing::info!(attempt, "Running judge");
                        let judge_result =
                            judge::run_judge(&http_client, &judge_cfg, &agent_message, &answer, &tool_evidence).await;
                        let score = judge_result.score.unwrap_or(0);
                        tracing::info!(
                            attempt,
                            score,
                            sources_verified = judge_result.sources_verified,
                            sources_total = judge_result.sources_total,
                            llm_error = ?judge_result.llm_error,
                            "Judge complete"
                        );

                        // Keep the best-scoring result
                        if score > best_score.unwrap_or(0) {
                            best_judge = Some(judge_result);
                            best_score = Some(score);
                        }

                        attempt += 1;

                        if score >= judge_cfg.retry_threshold as u8 || attempt > judge_cfg.max_retries {
                            break;
                        }

                        // Notify frontend about retry
                        let _ = tx_clone.send(ChatEvent::Thinking {
                            content: format!(
                                "Score {score}/100 — nieuwe poging ({attempt}/{})...",
                                judge_cfg.max_retries
                            ),
                        }).await;
                    }
                    Ok((None, _)) => {
                        tracing::debug!("No structured answer produced, skipping judge");
                        break;
                    }
                    Err(e) => {
                        let error_detail = format!("{e:#}");
                        attempt += 1;
                        if attempt <= MAX_ERROR_RETRIES {
                            tracing::warn!(attempt, error = %error_detail, "Agent failed, retrying");
                            let _ = tx_clone.send(ChatEvent::Thinking {
                                content: format!("Er ging iets mis, nieuwe poging ({attempt}/{MAX_ERROR_RETRIES})..."),
                            }).await;
                            // Brief pause before retry
                            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                            continue;
                        }
                        tracing::error!(error = %error_detail, "Agent failed after {attempt} attempts");
                        let _ = tx_clone.send(ChatEvent::Error {
                            message: format!(
                                "Teun heeft op dit moment een storing. Probeer het later nog eens. Error: {error_detail}"
                            ),
                        }).await;
                        break;
                    }
                }
            }

            // Emit the best judge result
            if let Some(judge_result) = best_judge {
                let _ = tx_clone.send(ChatEvent::Judge { result: judge_result }).await;
            }
        }
    });

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

use anyhow::{Context, Result};
use chrono::Utc;

use super::judge;
use super::store::EvalStore;
use super::types::{EvalResult, EvalRunStatus, EvalSummary, TestQuestion};

pub async fn run_eval(
    run_id: String,
    questions: Vec<TestQuestion>,
    eval_store: EvalStore,
    http_client: reqwest::Client,
    chat_api_base: String,
    anthropic_api_key: String,
    judge_model: String,
    judge_prompt_template: String,
    mode: String,
) {
    if let Err(e) = run_eval_inner(
        &run_id,
        &questions,
        &eval_store,
        &http_client,
        &chat_api_base,
        &anthropic_api_key,
        &judge_model,
        &judge_prompt_template,
        &mode,
    )
    .await
    {
        tracing::error!(run_id = %run_id, error = %e, "Eval run failed");
        let _ = eval_store.fail_run(&run_id, &e.to_string()).await;
    }
}

async fn run_eval_inner(
    run_id: &str,
    questions: &[TestQuestion],
    eval_store: &EvalStore,
    http_client: &reqwest::Client,
    chat_api_base: &str,
    anthropic_api_key: &str,
    judge_model: &str,
    judge_prompt_template: &str,
    mode: &str,
) -> Result<()> {
    eval_store
        .set_status(run_id, EvalRunStatus::Running)
        .await?;

    let mut pass_count = 0u32;
    let mut partial_count = 0u32;
    let mut fail_count = 0u32;
    let mut error_count = 0u32;
    let mut total_score = 0u64;
    let mut scored_count = 0u32;

    for question in questions {
        // Check if stopped
        if let Ok(Some(status)) = eval_store.get_run_status(run_id).await {
            if status == EvalRunStatus::Stopped {
                tracing::info!(run_id = %run_id, "Eval run stopped by user");
                return Ok(());
            }
        }

        let result = evaluate_question(
            http_client,
            chat_api_base,
            anthropic_api_key,
            judge_model,
            judge_prompt_template,
            question,
            mode,
        )
        .await;

        let eval_result = match result {
            Ok(r) => {
                match r.judge_verdict.as_deref() {
                    Some("pass") => pass_count += 1,
                    Some("partial") => partial_count += 1,
                    Some("fail") => fail_count += 1,
                    _ => {}
                }
                if let Some(score) = r.judge_score {
                    total_score += score as u64;
                    scored_count += 1;
                }
                r
            }
            Err(e) => {
                tracing::warn!(
                    run_id = %run_id,
                    question_id = %question.id,
                    error = %e,
                    "Failed to evaluate question"
                );
                error_count += 1;
                EvalResult {
                    question_id: question.id.clone(),
                    question: question.question.clone(),
                    expected_category: question.expected_category.clone(),
                    expected_key_points: question.expected_key_points.clone(),
                    actual_answer: None,
                    actual_rationale: None,
                    actual_category: None,
                    actual_sources: None,
                    chat_session_id: None,
                    judge_score: None,
                    judge_verdict: None,
                    judge_reasoning: None,
                    evaluated_at: Utc::now(),
                    error: Some(e.to_string()),
                }
            }
        };

        eval_store.push_result(run_id, &eval_result).await?;
        tracing::info!(
            run_id = %run_id,
            question_id = %question.id,
            verdict = ?eval_result.judge_verdict,
            score = ?eval_result.judge_score,
            "Evaluated question"
        );
    }

    let total = pass_count + partial_count + fail_count + error_count;
    let summary = EvalSummary {
        pass_count,
        partial_count,
        fail_count,
        error_count,
        average_score: if scored_count > 0 {
            total_score as f64 / scored_count as f64
        } else {
            0.0
        },
        pass_rate: if total > 0 {
            pass_count as f64 / total as f64 * 100.0
        } else {
            0.0
        },
    };

    eval_store.complete_run(run_id, &summary).await?;
    tracing::info!(
        run_id = %run_id,
        pass = pass_count,
        partial = partial_count,
        fail = fail_count,
        errors = error_count,
        "Eval run completed"
    );

    Ok(())
}

async fn evaluate_question(
    http_client: &reqwest::Client,
    chat_api_base: &str,
    anthropic_api_key: &str,
    judge_model: &str,
    judge_prompt_template: &str,
    question: &TestQuestion,
    mode: &str,
) -> Result<EvalResult> {
    let (structured_output, session_id) =
        call_chat_api(http_client, chat_api_base, &question.question, mode).await?;

    let actual_answer = structured_output["answer"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let actual_rationale = structured_output["rationale"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let actual_category = structured_output["category"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let actual_sources = structured_output.get("sources").cloned();
    let sources_str = actual_sources
        .as_ref()
        .map(|s| serde_json::to_string_pretty(s).unwrap_or_default())
        .unwrap_or_else(|| "geen bronnen".to_string());

    let verdict = judge::judge_answer(
        http_client,
        anthropic_api_key,
        judge_model,
        judge_prompt_template,
        &question.question,
        &question.expected_key_points,
        question.expected_category.as_deref(),
        &actual_answer,
        &actual_rationale,
        &sources_str,
        &actual_category,
    )
    .await?;

    Ok(EvalResult {
        question_id: question.id.clone(),
        question: question.question.clone(),
        expected_category: question.expected_category.clone(),
        expected_key_points: question.expected_key_points.clone(),
        actual_answer: Some(actual_answer),
        actual_rationale: Some(actual_rationale),
        actual_category: Some(actual_category),
        actual_sources,
        chat_session_id: Some(session_id),
        judge_score: Some(verdict.score),
        judge_verdict: Some(verdict.verdict),
        judge_reasoning: Some(verdict.reasoning),
        evaluated_at: Utc::now(),
        error: None,
    })
}

async fn call_chat_api(
    _client: &reqwest::Client,
    chat_api_base: &str,
    question: &str,
    mode: &str,
) -> Result<(serde_json::Value, String)> {
    let url = format!("{chat_api_base}/api/teun/chat");

    // Use a dedicated client with NO total timeout — SSE streams stay open for the
    // entire agent run (can be many minutes for tools mode). We only set a connect
    // timeout and rely on the SSE keep-alive to detect dead connections.
    let eval_client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(30))
        .read_timeout(std::time::Duration::from_secs(120))
        .build()
        .context("Failed to build eval HTTP client")?;

    let response = eval_client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({ "message": question, "mode": mode, "skip_persist": true }))
        .send()
        .await
        .context("Failed to call chat API")?;

    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("Chat API returned {status}: {text}");
    }

    // Stream the SSE body chunk by chunk — this avoids the total-timeout issue
    // since each chunk resets the read timeout.
    let mut body = String::new();
    let mut stream = response.bytes_stream();
    use futures::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("Stream read error")?;
        body.push_str(&String::from_utf8_lossy(&chunk));
    }

    parse_sse_result(&body)
}

fn parse_sse_result(body: &str) -> Result<(serde_json::Value, String)> {
    let mut current_event = String::new();
    let mut result_data: Option<String> = None;
    let mut error_message: Option<String> = None;

    for line in body.lines() {
        if let Some(event_type) = line.strip_prefix("event: ") {
            current_event = event_type.trim().to_string();
        } else if let Some(data) = line.strip_prefix("data: ") {
            match current_event.as_str() {
                "result" => result_data = Some(data.to_string()),
                "error" => {
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                        let inner = parsed.get("data").unwrap_or(&parsed);
                        if let Some(msg) = inner.get("message").and_then(|m| m.as_str()) {
                            error_message = Some(msg.to_string());
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // If no result event but we got an error, surface it
    let data_str = match result_data {
        Some(d) => d,
        None => {
            if let Some(err) = error_message {
                anyhow::bail!("Chat API error: {err}");
            }
            anyhow::bail!("No result event in chat API SSE response");
        }
    };

    let data: serde_json::Value =
        serde_json::from_str(&data_str).context("Failed to parse result event JSON")?;

    let inner = data.get("data").unwrap_or(&data);

    let structured_output = inner
        .get("structured_output")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let session_id = inner["session_id"]
        .as_str()
        .unwrap_or("")
        .to_string();

    if structured_output.is_null() {
        let content = inner["content"].as_str().unwrap_or("");
        if !content.is_empty() {
            return Ok((serde_json::json!({"answer": content}), session_id));
        }
        anyhow::bail!("No structured_output in result event. Raw: {}", data_str);
    }

    Ok((structured_output, session_id))
}

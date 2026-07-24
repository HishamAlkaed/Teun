pub mod llm;
pub mod types;
pub mod verifier;

use crate::agent::types::{MortgageAnswer, ToolEvidence};
use crate::rag::store::RagStore;
use types::{JudgeResult, SourceStatus};

pub struct JudgeConfig {
    pub anthropic_api_key: Option<String>,
    pub judge_model: String,
    pub resources_dir: String,
    pub timeout_secs: u64,
    pub retry_threshold: u8,
    pub max_retries: u8,
}

impl JudgeConfig {
    pub fn from_env() -> Self {
        Self {
            anthropic_api_key: std::env::var("JUDGE_API_KEY")
                .or_else(|_| std::env::var("ANTHROPIC_API_KEY"))
                .ok(),
            judge_model: std::env::var("JUDGE_MODEL")
                .unwrap_or_else(|_| "claude-haiku-4-5-20251001".to_string()),
            resources_dir: std::env::var("RESOURCES_DIR")
                .unwrap_or_else(|_| find_resources_dir()),
            timeout_secs: std::env::var("JUDGE_TIMEOUT_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(15),
            retry_threshold: std::env::var("JUDGE_RETRY_THRESHOLD")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(50),
            max_retries: std::env::var("JUDGE_MAX_RETRIES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1),
        }
    }
}

/// Walk up from CWD to find the resources/acceptatie directory.
fn find_resources_dir() -> String {
    if let Ok(mut dir) = std::env::current_dir() {
        for _ in 0..5 {
            let candidate = dir.join("resources/acceptatie");
            if candidate.is_dir() {
                return candidate.to_string_lossy().to_string();
            }
            if !dir.pop() {
                break;
            }
        }
    }
    "resources/acceptatie".to_string()
}

/// Run both judge phases: programmatic source verification, then LLM faithfulness check.
#[tracing::instrument(
    name = "ai.judge_pipeline",
    skip(client, config, store, answer),
    fields(gen_ai.operation.name = "judge")
)]
pub async fn run_judge(
    client: &reqwest::Client,
    config: &JudgeConfig,
    store: Option<&RagStore>,
    question: &str,
    answer: &MortgageAnswer,
    tool_evidence: &ToolEvidence,
) -> JudgeResult {
    // Phase 1: programmatic source verification against the canonical
    // extracted body in Postgres (disk fallback), using chunk-derived
    // evidence for line ranges.
    let source_verdicts =
        verifier::verify_sources(store, &config.resources_dir, &answer.sources, tool_evidence)
            .await;

    let sources_total = source_verdicts.len() as u8;
    let sources_verified = source_verdicts
        .iter()
        .filter(|v| matches!(v.status, SourceStatus::Ok))
        .count() as u8;

    // Phase 2: LLM faithfulness check
    let (score, reasoning, llm_skipped, llm_error) = match &config.anthropic_api_key {
        None => (None, None, true, None),
        Some(api_key) => {
            match tokio::time::timeout(
                std::time::Duration::from_secs(config.timeout_secs),
                llm::call_faithfulness_judge(
                    client,
                    api_key,
                    &config.judge_model,
                    question,
                    answer,
                    &source_verdicts,
                ),
            )
            .await
            {
                Ok(Ok(verdict)) => (Some(verdict.score), Some(verdict.reasoning), false, None),
                Ok(Err(e)) => (None, None, false, Some(format!("{e:#}"))),
                Err(_) => (None, None, false, Some("Judge timed out".to_string())),
            }
        }
    };

    JudgeResult {
        source_verdicts,
        sources_verified,
        sources_total,
        score,
        reasoning,
        llm_skipped,
        llm_error,
    }
}

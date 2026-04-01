use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestQuestion {
    pub id: String,
    pub app_id: String,
    pub question: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_category: Option<String>,
    #[serde(default)]
    pub expected_key_points: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default = "default_points")]
    pub points: u16,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn default_points() -> u16 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalRun {
    pub id: String,
    #[serde(default)]
    pub app_id: String,
    pub status: EvalRunStatus,
    pub started_by: String,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    pub total_questions: u32,
    pub completed_questions: u32,
    #[serde(default)]
    pub results: Vec<EvalResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<EvalSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EvalRunStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Stopped,
}

impl EvalRunStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "running" => Self::Running,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "stopped" => Self::Stopped,
            _ => Self::Pending,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalResult {
    pub question_id: String,
    pub question: String,
    pub expected_category: Option<String>,
    pub expected_key_points: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_answer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_rationale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_sources: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub judge_score: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub judge_verdict: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub judge_reasoning: Option<String>,
    pub evaluated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalSummary {
    pub pass_count: u32,
    pub partial_count: u32,
    pub fail_count: u32,
    pub error_count: u32,
    pub average_score: f64,
    pub pass_rate: f64,
}

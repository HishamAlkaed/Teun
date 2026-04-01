use serde::{Deserialize, Serialize};

/// Per-source verification result from Phase 1 (programmatic check).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceVerdict {
    pub document: String,
    pub section: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_range: Option<String>,
    pub status: SourceStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceStatus {
    Ok,
    DocumentNotFound,
    LineRangeOutOfBounds,
    QuoteMismatch,
}

/// Combined output of both judge phases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgeResult {
    pub source_verdicts: Vec<SourceVerdict>,
    pub sources_verified: u8,
    pub sources_total: u8,
    /// Phase 2 LLM score (1-100). None if Phase 2 was skipped or failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    /// Whether Phase 2 was skipped (e.g. no API key configured).
    pub llm_skipped: bool,
    /// Set when Phase 2 failed (API error, timeout, parse error).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_error: Option<String>,
}

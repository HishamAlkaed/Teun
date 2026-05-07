use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Evidence collected from tool calls during a Claude CLI session.
/// Maps document filenames to the line ranges that were actually read/searched.
#[derive(Debug, Clone, Default)]
pub struct ToolEvidence {
    /// filename → list of (start_line, end_line) ranges that were accessed (1-indexed, inclusive)
    pub accessed_ranges: HashMap<String, Vec<(usize, usize)>>,
}

impl ToolEvidence {
    /// Record that lines start..=end of a file were accessed.
    pub fn add_range(&mut self, file_path: &str, start: usize, end: usize) {
        // Extract just the filename from the path
        let filename = file_path
            .rsplit('/')
            .next()
            .unwrap_or(file_path);
        self.accessed_ranges
            .entry(filename.to_string())
            .or_default()
            .push((start, end));
    }

}

/// Structured answer from the acceptatie chatbot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MortgageAnswer {
    pub answer: String,
    pub rationale: String,
    pub sources: Vec<SourceReference>,
    pub category: AnswerCategory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceReference {
    pub document: String,
    pub section: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_range: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerCategory {
    Standard,
    MandaatUitzondering,
    DoorverwijzenSpecialeAfhandeling,
}

/// SSE event types sent to the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data")]
#[serde(rename_all = "snake_case")]
pub enum ChatEvent {
    Thinking { content: String },
    ToolUse { tool: String, input: serde_json::Value, #[serde(skip_serializing_if = "Option::is_none")] content: Option<String> },
    Partial { content: String },
    Result { structured_output: serde_json::Value, session_id: String },
    Error { message: String },
    Judge { result: crate::judge::types::JudgeResult },
}

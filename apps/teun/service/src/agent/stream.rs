//! Shared SSE streaming machinery for the two-phase answer contract.
//!
//! Extracted from `inline.rs` (Plan 01-04 T2) so multiple answer paths can
//! reuse it — logic preserved byte-for-byte:
//! - `stream_two_phase`: drives an Anthropic Messages SSE response, emitting
//!   `ChatEvent::Partial` events with char-boundary-safe slicing and stopping
//!   the partial stream at the `\n---JSON---` separator.
//! - `parse_two_phase_response`: splits the accumulated text at the separator
//!   and combines the answer text with the parsed JSON metadata.

use anyhow::{Context, Result};
use tokio::sync::mpsc;

use super::types::{ChatEvent, MortgageAnswer};

/// Process an SSE streaming response using the two-phase contract:
/// stream the answer text as `ChatEvent::Partial` events, stop streaming at
/// the `\n---JSON---` separator, and return the full accumulated text plus
/// the provider session id (Anthropic `message_start` message id).
/// Expected format: "<answer text>\n---JSON---\n{...}"
pub(crate) async fn stream_two_phase(
    resp: reqwest::Response,
    tx: &mpsc::Sender<ChatEvent>,
) -> Result<(String, String)> {
    const SEPARATOR: &str = "\n---JSON---";
    let mut full_text = String::new();
    let mut session_id = String::new();
    let mut bytes_stream = resp.bytes_stream();
    let mut last_partial_len = 0usize;
    let mut separator_found = false;

    use futures::StreamExt;
    let mut buffer = String::new();

    while let Some(chunk) = bytes_stream.next().await {
        let chunk = chunk.context("Stream read error")?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        // Process complete SSE lines
        while let Some(line_end) = buffer.find('\n') {
            let line = buffer[..line_end].trim_end().to_string();
            buffer = buffer[line_end + 1..].to_string();

            if line.is_empty() || line.starts_with(':') {
                continue;
            }

            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" {
                    continue;
                }

                if let Ok(event) = serde_json::from_str::<serde_json::Value>(data) {
                    match event.get("type").and_then(|t| t.as_str()) {
                        Some("message_start") => {
                            if let Some(id) = event["message"]["id"].as_str() {
                                session_id = id.to_string();
                            }
                        }
                        Some("content_block_delta") => {
                            if let Some(text) = event["delta"]["text"].as_str() {
                                full_text.push_str(text);

                                if !separator_found {
                                    if let Some(sep_pos) = full_text.find(SEPARATOR) {
                                        // Separator found — emit final answer text and stop streaming
                                        separator_found = true;
                                        let answer_text = full_text[..sep_pos].trim_end().to_string();
                                        if !answer_text.is_empty() {
                                            let _ = tx.send(ChatEvent::Partial { content: answer_text }).await;
                                        }
                                    } else {
                                        // Stream text up to a safe point, leaving room for partial separator.
                                        // Snap down to a char boundary so we never slice mid-UTF-8 sequence.
                                        let mut safe_len = full_text.len().saturating_sub(SEPARATOR.len());
                                        while safe_len > 0 && !full_text.is_char_boundary(safe_len) {
                                            safe_len -= 1;
                                        }
                                        if safe_len > last_partial_len {
                                            last_partial_len = safe_len;
                                            let _ = tx.send(ChatEvent::Partial {
                                                content: full_text[..safe_len].to_string(),
                                            }).await;
                                        }
                                    }
                                }
                            }
                        }
                        Some("message_stop") => {
                            // Stream complete — flush any remaining answer text
                            if !separator_found && full_text.len() > last_partial_len {
                                let _ = tx.send(ChatEvent::Partial {
                                    content: full_text.trim_end().to_string(),
                                }).await;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    Ok((full_text, session_id))
}

/// Parse a two-phase response: plain text answer followed by `---JSON---` and structured data.
/// Returns a MortgageAnswer combining the streamed text with the parsed JSON metadata.
pub(crate) fn parse_two_phase_response(text: &str) -> Option<MortgageAnswer> {
    const SEPARATOR: &str = "\n---JSON---";

    if let Some(sep_pos) = text.find(SEPARATOR) {
        let answer_text = text[..sep_pos].trim().to_string();
        let json_part = text[sep_pos + SEPARATOR.len()..].trim();

        // Parse the JSON metadata (rationale, sources, category — no answer field)
        #[derive(serde::Deserialize)]
        struct StructuredPart {
            #[serde(default)]
            rationale: String,
            #[serde(default)]
            sources: Vec<super::types::SourceReference>,
            #[serde(default)]
            category: Option<super::types::AnswerCategory>,
        }

        // Strip markdown code fences if present
        let json_str = json_part
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        let parts = serde_json::from_str::<StructuredPart>(json_str).unwrap_or_else(|_| StructuredPart {
            rationale: String::new(),
            sources: vec![],
            category: None,
        });

        Some(MortgageAnswer {
            answer: answer_text,
            rationale: parts.rationale,
            sources: parts.sources,
            category: parts.category.unwrap_or(super::types::AnswerCategory::Standard),
        })
    } else {
        // No separator — fall back to treating the whole text as the answer
        if text.trim().is_empty() {
            return None;
        }
        Some(MortgageAnswer {
            answer: text.trim().to_string(),
            rationale: String::new(),
            sources: vec![],
            category: super::types::AnswerCategory::Standard,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::AnswerCategory;

    #[test]
    fn parses_two_phase_answer_with_json_metadata() {
        let text = "Het antwoord is ja.\n---JSON---\n{\"rationale\":\"omdat\",\"sources\":[{\"document\":\"gids.pdf\",\"section\":\"5.1\",\"quote\":\"citaat\",\"line_range\":\"12-14\"}],\"category\":\"standard\"}";
        let answer = parse_two_phase_response(text).expect("parses");
        assert_eq!(answer.answer, "Het antwoord is ja.");
        assert_eq!(answer.rationale, "omdat");
        assert_eq!(answer.sources.len(), 1);
        assert_eq!(answer.sources[0].document, "gids.pdf");
        assert_eq!(answer.sources[0].line_range.as_deref(), Some("12-14"));
        assert!(matches!(answer.category, AnswerCategory::Standard));
    }

    #[test]
    fn parses_json_wrapped_in_code_fences() {
        let text = "Antwoord.\n---JSON---\n```json\n{\"rationale\":\"r\",\"sources\":[],\"category\":\"mandaat_uitzondering\"}\n```";
        let answer = parse_two_phase_response(text).expect("parses");
        assert_eq!(answer.answer, "Antwoord.");
        assert!(matches!(answer.category, AnswerCategory::MandaatUitzondering));
    }

    #[test]
    fn missing_separator_falls_back_to_whole_text() {
        let answer = parse_two_phase_response("alleen tekst").expect("parses");
        assert_eq!(answer.answer, "alleen tekst");
        assert!(answer.sources.is_empty());
        assert!(matches!(answer.category, AnswerCategory::Standard));
    }

    #[test]
    fn empty_text_yields_none() {
        assert!(parse_two_phase_response("   ").is_none());
    }

    #[test]
    fn malformed_json_still_returns_answer_text() {
        let text = "Antwoord.\n---JSON---\n{not json";
        let answer = parse_two_phase_response(text).expect("parses");
        assert_eq!(answer.answer, "Antwoord.");
        assert!(answer.rationale.is_empty());
        assert!(answer.sources.is_empty());
    }
}

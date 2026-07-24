//! Shared SSE streaming machinery for the two-phase answer contract.
//!
//! Extracted from `inline.rs` (Plan 01-04 T2) so multiple answer paths can
//! reuse it — the accumulate/emit logic is preserved byte-for-byte:
//! - `stream_two_phase`: drives a streaming SSE response, emitting
//!   `ChatEvent::Partial` events with char-boundary-safe slicing and stopping
//!   the partial stream at the `\n---JSON---` separator. The per-provider SSE
//!   event decode (`SseDialect`) is the only provider-specific step; the
//!   two-phase separator handling is shared.
//! - `parse_two_phase_response`: splits the accumulated text at the separator
//!   and combines the answer text with the parsed JSON metadata.

use anyhow::{Context, Result};
use tokio::sync::mpsc;

use super::types::{ChatEvent, MortgageAnswer};

/// Which streaming SSE dialect the response speaks. The decode step is the
/// only provider-specific part of `stream_two_phase`; both dialects funnel
/// their text deltas through the same two-phase accumulate/emit logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SseDialect {
    /// Anthropic Messages API: `message_start` (session id) /
    /// `content_block_delta` (text) / `message_stop` (flush).
    Anthropic,
    /// OpenAI-style chat completions (Azure OpenAI): chunk `id` (session id),
    /// `choices[0].delta.content` (text), `choices[0].finish_reason` (flush),
    /// terminated by a `data: [DONE]` sentinel.
    AzureOpenAi,
}

/// A decoded SSE `data:` payload, normalized across providers. One payload
/// can carry several of these at once (e.g. an Azure chunk with both an id
/// and a content delta), so the fields are independent.
#[derive(Debug, Default, PartialEq)]
struct Decoded {
    session_id: Option<String>,
    text: Option<String>,
    /// The provider signalled end-of-message — flush any unstreamed text.
    stop: bool,
}

/// Decode one SSE `data:` payload according to the dialect. Unparseable or
/// irrelevant payloads decode to the empty `Decoded` (ignored), matching the
/// original loop's silent skip of unknown events.
fn decode_sse_data(dialect: SseDialect, data: &str) -> Decoded {
    let mut decoded = Decoded::default();
    let Ok(event) = serde_json::from_str::<serde_json::Value>(data) else {
        return decoded;
    };
    match dialect {
        SseDialect::Anthropic => match event.get("type").and_then(|t| t.as_str()) {
            Some("message_start") => {
                if let Some(id) = event["message"]["id"].as_str() {
                    decoded.session_id = Some(id.to_string());
                }
            }
            Some("content_block_delta") => {
                if let Some(text) = event["delta"]["text"].as_str() {
                    decoded.text = Some(text.to_string());
                }
            }
            Some("message_stop") => decoded.stop = true,
            _ => {}
        },
        SseDialect::AzureOpenAi => {
            if let Some(id) = event.get("id").and_then(|i| i.as_str()) {
                if !id.is_empty() {
                    decoded.session_id = Some(id.to_string());
                }
            }
            if let Some(choice) = event
                .get("choices")
                .and_then(|c| c.as_array())
                .and_then(|c| c.first())
            {
                if let Some(text) = choice["delta"]["content"].as_str() {
                    decoded.text = Some(text.to_string());
                }
                if choice.get("finish_reason").is_some_and(|f| !f.is_null()) {
                    decoded.stop = true;
                }
            }
        }
    }
    decoded
}

/// Process an SSE streaming response using the two-phase contract:
/// stream the answer text as `ChatEvent::Partial` events, stop streaming at
/// the `\n---JSON---` separator, and return the full accumulated text plus
/// the provider session id (Anthropic `message_start` message id / Azure
/// OpenAI chunk id).
/// Expected format: "<answer text>\n---JSON---\n{...}"
pub(crate) async fn stream_two_phase(
    resp: reqwest::Response,
    dialect: SseDialect,
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

                let decoded = decode_sse_data(dialect, data);
                if let Some(id) = decoded.session_id {
                    session_id = id;
                }
                if let Some(text) = decoded.text {
                    full_text.push_str(&text);

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
                if decoded.stop {
                    // Stream complete — flush any remaining answer text
                    if !separator_found && full_text.len() > last_partial_len {
                        let _ = tx.send(ChatEvent::Partial {
                            content: full_text.trim_end().to_string(),
                        }).await;
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

    // --- provider-specific SSE decode ---

    #[test]
    fn anthropic_decode_maps_message_lifecycle() {
        let start = r#"{"type":"message_start","message":{"id":"msg_abc123"}}"#;
        let decoded = decode_sse_data(SseDialect::Anthropic, start);
        assert_eq!(decoded.session_id.as_deref(), Some("msg_abc123"));
        assert!(decoded.text.is_none());
        assert!(!decoded.stop);

        let delta = r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hallo"}}"#;
        let decoded = decode_sse_data(SseDialect::Anthropic, delta);
        assert_eq!(decoded.text.as_deref(), Some("Hallo"));
        assert!(decoded.session_id.is_none());
        assert!(!decoded.stop);

        let stop = r#"{"type":"message_stop"}"#;
        let decoded = decode_sse_data(SseDialect::Anthropic, stop);
        assert!(decoded.stop);

        // Irrelevant event types are ignored.
        let ping = r#"{"type":"ping"}"#;
        assert_eq!(decode_sse_data(SseDialect::Anthropic, ping), Decoded::default());
    }

    #[test]
    fn azure_decode_maps_chat_completion_chunks() {
        // First chunk: id + role, no content yet.
        let first = r#"{"id":"chatcmpl-9x","choices":[{"delta":{"role":"assistant"},"finish_reason":null,"index":0}]}"#;
        let decoded = decode_sse_data(SseDialect::AzureOpenAi, first);
        assert_eq!(decoded.session_id.as_deref(), Some("chatcmpl-9x"));
        assert!(decoded.text.is_none());
        assert!(!decoded.stop);

        // Content delta chunk.
        let content = r#"{"id":"chatcmpl-9x","choices":[{"delta":{"content":"Hallo"},"finish_reason":null,"index":0}]}"#;
        let decoded = decode_sse_data(SseDialect::AzureOpenAi, content);
        assert_eq!(decoded.text.as_deref(), Some("Hallo"));
        assert!(!decoded.stop);

        // Final chunk: finish_reason set → flush signal.
        let last = r#"{"id":"chatcmpl-9x","choices":[{"delta":{},"finish_reason":"stop","index":0}]}"#;
        let decoded = decode_sse_data(SseDialect::AzureOpenAi, last);
        assert!(decoded.stop);
        assert!(decoded.text.is_none());

        // Azure content-filter prelude chunk without choices is ignored
        // (no text, no stop).
        let prelude = r#"{"id":"","choices":[]}"#;
        let decoded = decode_sse_data(SseDialect::AzureOpenAi, prelude);
        assert_eq!(decoded, Decoded::default());
    }

    #[test]
    fn garbage_payloads_decode_to_ignored() {
        assert_eq!(decode_sse_data(SseDialect::Anthropic, "{not json"), Decoded::default());
        assert_eq!(decode_sse_data(SseDialect::AzureOpenAi, "{not json"), Decoded::default());
    }
}

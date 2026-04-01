use std::process::Stdio;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

use super::types::{ChatEvent, MortgageAnswer, ToolEvidence};

/// Configuration for the Claude CLI subprocess.
pub struct ClaudeConfig {
    pub model: String,
    pub skill_path: String,
    pub working_dir: String,
    pub resources_dir: String,
}

impl ClaudeConfig {
    pub fn from_env() -> Self {
        // Try to find the project root by looking for resources/acceptatie in parent dirs.
        // In the Docker container CWD is /app which contains config/ and resources/acceptatie/.
        // In local dev it walks up to the workspace root.
        let project_root = find_project_root().unwrap_or_else(|| ".".to_string());

        Self {
            model: std::env::var("CLAUDE_MODEL")
                .unwrap_or_else(|_| "claude-sonnet-4-20250514".to_string()),
            skill_path: std::env::var("SKILL_PATH")
                .unwrap_or_else(|_| {
                    // Container: /app/config/acceptatie-beleid.md
                    // Local dev: {root}/apps/teun/service/config/acceptatie-beleid.md
                    let container_path = format!("{}/config/acceptatie-beleid.md", project_root);
                    let dev_path = format!("{}/apps/teun/service/config/acceptatie-beleid.md", project_root);
                    if std::path::Path::new(&container_path).exists() {
                        container_path
                    } else {
                        dev_path
                    }
                }),
            working_dir: std::env::var("WORKING_DIR")
                .unwrap_or_else(|_| project_root.clone()),
            resources_dir: std::env::var("RESOURCES_DIR")
                .unwrap_or_else(|_| format!("{}/resources/acceptatie", project_root)),
        }
    }
}

/// Walk up from CWD to find the workspace root (directory containing resources/acceptatie).
pub fn find_project_root() -> Option<String> {
    let mut dir = std::env::current_dir().ok()?;
    for _ in 0..5 {
        if dir.join("resources/acceptatie").is_dir() {
            return Some(dir.to_string_lossy().to_string());
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

/// Spawn a Claude CLI subprocess and stream events back via the channel.
/// Returns the parsed MortgageAnswer if a structured result was produced,
/// so the caller can pass it to the judge.
#[tracing::instrument(
    name = "ai.claude_cli",
    skip(config, tx),
    fields(
        gen_ai.system = "anthropic",
        gen_ai.operation.name = "claude_cli",
        gen_ai.request.model = %config.model,
        message_len = message.len(),
    )
)]
pub async fn run_claude(
    message: &str,
    session_id: Option<&str>,
    config: &ClaudeConfig,
    tx: mpsc::Sender<ChatEvent>,
) -> Result<(Option<MortgageAnswer>, ToolEvidence)> {
    let mut args = vec![
        "-p".to_string(),
        "--verbose".to_string(),
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--model".to_string(),
        config.model.clone(),
        "--add-dir".to_string(),
        config.resources_dir.clone(),
        "--append-system-prompt".to_string(),
        {
            let mut prompt = std::fs::read_to_string(&config.skill_path)
                .context("Failed to read skill file")?;
            // Replace relative resource path with the actual absolute path
            prompt = prompt.replace("resources/acceptatie/", &format!("{}/", config.resources_dir));
            prompt = prompt.replace("`resources/acceptatie`", &format!("`{}`", config.resources_dir));
            prompt
        },
    ];

    // Resume session if provided — validate format to prevent CLI flag injection
    if let Some(sid) = session_id {
        if is_valid_session_id(sid) {
            args.push("--resume".to_string());
            args.push(sid.to_string());
        } else {
            tracing::warn!("Rejected invalid session_id format");
        }
    }

    // End-of-options marker to prevent message being interpreted as CLI flags
    args.push("--".to_string());
    args.push(message.to_string());

    let message_len = message.len();
    let res_exists = std::path::Path::new(&config.resources_dir).is_dir();
    tracing::info!(
        working_dir = %config.working_dir,
        resources_dir = %config.resources_dir,
        resources_exists = res_exists,
        model = %config.model,
        message_len,
        "Spawning claude CLI"
    );

    let mut child = Command::new("claude")
        .args(&args)
        .current_dir(&config.working_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn claude CLI. Is it installed and in PATH?")?;

    tracing::info!("Claude CLI subprocess spawned");

    let stdout = child.stdout.take().context("Failed to capture stdout")?;
    let stderr = child.stderr.take().context("Failed to capture stderr")?;

    // Spawn a task to log stderr
    tokio::spawn(async move {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            tracing::warn!(stderr = %line, "Claude CLI stderr");
        }
    });

    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();
    let mut mortgage_answer: Option<MortgageAnswer> = None;
    let mut tool_evidence = ToolEvidence::default();
    let mut channel_alive = true;

    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }

        tracing::debug!(raw_line = %line, "Claude CLI stdout");

        let parsed_events = parse_stream_event(&line);
        if parsed_events.is_empty() {
            tracing::warn!(line = %line, "Failed to parse stream event");
            continue;
        }

        for event in parsed_events {
            // Collect tool evidence from file-access tools
            if let ChatEvent::ToolUse { ref tool, ref input, .. } = event {
                collect_tool_evidence(tool, input, &mut tool_evidence);
            }

            // Capture the structured answer for the judge
            if let ChatEvent::Result { ref structured_output, .. } = event {
                match serde_json::from_value::<MortgageAnswer>(structured_output.clone()) {
                    Ok(answer) => mortgage_answer = Some(answer),
                    Err(e) => tracing::debug!(error = %e, "Result not a valid MortgageAnswer"),
                }
            }

            // Send event to channel; if receiver dropped, continue processing
            // (don't kill Claude — the result is still needed for persistence/judge)
            if channel_alive {
                if tx.send(event).await.is_err() {
                    tracing::warn!("Event channel closed, continuing agent execution for persistence");
                    channel_alive = false;
                }
            }
        }
    }

    let status = child.wait().await?;
    tracing::info!(status = %status, "Claude CLI subprocess exited");
    if !status.success() {
        anyhow::bail!("Claude CLI exited with status: {status}");
    }

    tracing::info!(
        files_accessed = tool_evidence.accessed_ranges.len(),
        total_ranges = tool_evidence.accessed_ranges.values().map(|v| v.len()).sum::<usize>(),
        "Tool evidence collected"
    );

    Ok((mortgage_answer, tool_evidence))
}

/// Extract file-access evidence from tool_use inputs.
/// Handles Read, Grep, Glob, and Bash tools that access policy documents.
fn collect_tool_evidence(tool: &str, input: &serde_json::Value, evidence: &mut ToolEvidence) {
    match tool {
        "Read" => {
            // Read tool: { file_path, offset?, limit? }
            if let Some(file_path) = input.get("file_path").and_then(|v| v.as_str()) {
                let offset = input.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let limit = input.get("limit").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

                if offset > 0 && limit > 0 {
                    // Specific range: offset is 1-indexed line number
                    evidence.add_range(file_path, offset, offset + limit - 1);
                } else if offset > 0 {
                    // Offset without limit — read from offset to ~2000 lines (Read default)
                    evidence.add_range(file_path, offset, offset + 2000);
                } else {
                    // Full file read — mark as "all lines accessed"
                    // Use a large sentinel; verifier will clamp to actual doc length
                    evidence.add_range(file_path, 1, usize::MAX);
                }

                tracing::debug!(
                    file_path,
                    offset,
                    limit,
                    "Read tool evidence collected"
                );
            }
        }
        "Grep" => {
            // Grep tool: { pattern, path?, glob?, output_mode? }
            // We know which file/dir was searched, but not exact lines until result.
            // Still useful: if grep targets a specific file, record it as fully accessed.
            if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
                // If path points to a file (has extension), record it
                if path.contains('.') && !path.ends_with('/') {
                    evidence.add_range(path, 1, usize::MAX);
                    tracing::debug!(path, "Grep tool evidence collected (file target)");
                }
            }
        }
        "Glob" => {
            // Glob just lists files, no content — skip
        }
        "Bash" => {
            // Could run cat, grep, head, etc. — too complex to parse reliably.
            // Log for debugging but don't extract evidence.
            if let Some(cmd) = input.get("command").and_then(|v| v.as_str()) {
                tracing::debug!(command = cmd, "Bash tool used (evidence not extracted)");
            }
        }
        _ => {}
    }
}

/// Try to extract a structured JSON answer from text that may contain
/// markdown code blocks (```json ... ```).
fn extract_structured_json(text: &str) -> Option<serde_json::Value> {
    // First, try direct JSON parse
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(text) {
        if v.get("answer").is_some() {
            return Some(v);
        }
    }

    // Try extracting from markdown code block: ```json\n{...}\n```
    if let Some(start) = text.find("```json") {
        let json_start = start + 7; // skip "```json"
        if let Some(end) = text[json_start..].find("```") {
            let json_str = text[json_start..json_start + end].trim();
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(json_str) {
                if v.get("answer").is_some() {
                    return Some(v);
                }
            }
        }
    }

    // Try extracting from generic code block: ```\n{...}\n```
    if let Some(start) = text.find("```\n") {
        let json_start = start + 4;
        if let Some(end) = text[json_start..].find("```") {
            let json_str = text[json_start..json_start + end].trim();
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(json_str) {
                if v.get("answer").is_some() {
                    return Some(v);
                }
            }
        }
    }

    None
}

/// Validate that a session ID looks like a Claude session ID (hex string or UUID-like).
/// Rejects anything that looks like a CLI flag or contains shell metacharacters.
fn is_valid_session_id(sid: &str) -> bool {
    !sid.is_empty()
        && sid.len() <= 128
        && !sid.starts_with('-')
        && sid
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Parse a line of stream-json output from Claude CLI into a ChatEvent.
fn parse_stream_event(line: &str) -> Vec<ChatEvent> {
    let v: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return vec![],
    };

    let event_type = match v.get("type").and_then(|t| t.as_str()) {
        Some(t) => t,
        None => return vec![],
    };

    match event_type {
        "assistant" => {
            // Assistant message with content blocks — collect all events
            let message = match v.get("message") {
                Some(m) => m,
                None => return vec![],
            };
            let blocks = match message.get("content").and_then(|c| c.as_array()) {
                Some(b) => b,
                None => return vec![],
            };

            let mut events = Vec::new();
            let mut text_content: Option<String> = None;

            for block in blocks {
                let btype = match block.get("type").and_then(|t| t.as_str()) {
                    Some(t) => t,
                    None => continue,
                };
                match btype {
                    "text" => {
                        let text = match block.get("text").and_then(|t| t.as_str()) {
                            Some(t) => t,
                            None => continue,
                        };
                        if let Some(structured) = extract_structured_json(text) {
                            let session_id = v
                                .get("session_id")
                                .and_then(|s| s.as_str())
                                .unwrap_or("")
                                .to_string();
                            events.push(ChatEvent::Result {
                                structured_output: structured,
                                session_id,
                            });
                        } else {
                            // Store text to attach to a subsequent tool_use, or emit as Partial
                            text_content = Some(text.to_string());
                        }
                    }
                    "tool_use" => {
                        let tool = match block.get("name").and_then(|n| n.as_str()) {
                            Some(t) => t.to_string(),
                            None => continue,
                        };
                        let input = block.get("input").cloned().unwrap_or(serde_json::Value::Null);
                        events.push(ChatEvent::ToolUse {
                            tool,
                            input,
                            content: text_content.take(),
                        });
                    }
                    "thinking" => {
                        let text = match block.get("thinking").and_then(|t| t.as_str()) {
                            Some(t) => t,
                            None => continue,
                        };
                        events.push(ChatEvent::Thinking {
                            content: text.to_string(),
                        });
                    }
                    _ => {}
                }
            }

            // If there was text content not consumed by a tool_use, emit as Partial
            if let Some(text) = text_content {
                events.push(ChatEvent::Partial { content: text });
            }

            events
        }
        "result" => {
            // Final result
            let result_text = match v.get("result").and_then(|r| r.as_str()) {
                Some(t) => t,
                None => return vec![],
            };
            let session_id = v
                .get("session_id")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();

            if let Some(structured) = extract_structured_json(result_text) {
                return vec![ChatEvent::Result {
                    structured_output: structured,
                    session_id,
                }];
            }

            vec![ChatEvent::Result {
                structured_output: serde_json::json!({ "answer": result_text }),
                session_id,
            }]
        }
        _ => vec![],
    }
}

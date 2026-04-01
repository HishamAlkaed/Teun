use anyhow::{Context, Result};
use tokio::sync::mpsc;

use super::claude::find_project_root;
use super::types::{ChatEvent, MortgageAnswer};

/// Configuration for inline mode (Messages API, no CLI).
pub struct InlineConfig {
    pub api_key: String,
    pub model: String,
    pub skill_path: String,
    pub resources_dir: String,
}

impl InlineConfig {
    /// Build from environment. Tries JUDGE_API_KEY, ANTHROPIC_API_KEY,
    /// then falls back to reading the OAuth token from ~/.claude/.credentials.json.
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("JUDGE_API_KEY")
            .or_else(|_| std::env::var("ANTHROPIC_API_KEY"))
            .or_else(|_| read_oauth_token())
            .context("No API key available for inline mode. Set JUDGE_API_KEY, ANTHROPIC_API_KEY, or ensure ~/.claude/.credentials.json exists.")?;

        let project_root = find_project_root().unwrap_or_else(|| ".".to_string());

        Ok(Self {
            api_key,
            model: std::env::var("INLINE_MODEL")
                .or_else(|_| std::env::var("CLAUDE_MODEL"))
                .unwrap_or_else(|_| "claude-haiku-4-5-20241022".to_string()),
            skill_path: std::env::var("SKILL_PATH")
                .unwrap_or_else(|_| {
                    let container_path = format!("{}/config/acceptatie-beleid.md", project_root);
                    let dev_path = format!("{}/apps/teun/service/config/acceptatie-beleid.md", project_root);
                    if std::path::Path::new(&container_path).exists() {
                        container_path
                    } else {
                        dev_path
                    }
                }),
            resources_dir: std::env::var("RESOURCES_DIR")
                .unwrap_or_else(|_| format!("{}/resources/acceptatie", project_root)),
        })
    }
}

/// Read OAuth access token from Claude's credentials file.
fn read_oauth_token() -> std::result::Result<String, std::env::VarError> {
    let home = std::env::var("HOME").map_err(|_| std::env::VarError::NotPresent)?;
    let path = format!("{home}/.claude/.credentials.json");
    let content = std::fs::read_to_string(&path).map_err(|_| std::env::VarError::NotPresent)?;
    let v: serde_json::Value =
        serde_json::from_str(&content).map_err(|_| std::env::VarError::NotPresent)?;
    v["claudeAiOauth"]["accessToken"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or(std::env::VarError::NotPresent)
}

/// Build the system prompt for inline mode: skill instructions + all documents.
fn build_system_prompt(config: &InlineConfig) -> Result<String> {
    let mut skill = std::fs::read_to_string(&config.skill_path)
        .context("Failed to read skill file")?;

    // Replace tool-usage instructions with inline instructions
    skill = skill.replace(
        "1. Gebruik ALTIJD de Grep en Read tools om de beleidsdocumenten te doorzoeken voordat je antwoordt.\n\
         2. Doorzoek eerst met Grep op relevante termen, lees dan de gevonden secties met Read.\n\
         3. Baseer je antwoord UITSLUITEND op wat er in de documenten staat.\n\
         4. Als informatie niet in de documenten te vinden is, zeg dat eerlijk.\n\
         5. Noteer bij het lezen met Read de regelnummers van relevante passages. Gebruik deze als `line_range` in de bronverwijzingen.\n\
         6. Kopieer het relevante citaat LETTERLIJK uit het document voor het `quote` veld. Dit citaat wordt getoond aan de gebruiker als bewijs.",
        "1. De volledige beleidsdocumenten staan hieronder in de systeemprompt.\n\
         2. Baseer je antwoord UITSLUITEND op wat er in de documenten staat.\n\
         3. Als informatie niet in de documenten te vinden is, zeg dat eerlijk.\n\
         4. Kopieer het relevante citaat LETTERLIJK uit het document voor het `quote` veld.",
    );

    // Load all resource documents
    let docs = load_inline_documents(&config.resources_dir)?;
    skill.push_str("\n\n---\n\n## Beleidsdocumenten (inline)\n\n");
    skill.push_str(&docs);

    Ok(skill)
}

/// Read all markdown files from the resources directory and concatenate them.
fn load_inline_documents(resources_dir: &str) -> Result<String> {
    let mut content = String::new();
    let dir = std::fs::read_dir(resources_dir)
        .with_context(|| format!("Failed to read resources dir: {resources_dir}"))?;

    let mut entries: Vec<_> = dir
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "md")
                .unwrap_or(false)
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        let filename = path.file_name().unwrap_or_default().to_string_lossy();
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        content.push_str(&format!("### Document: {filename}\n\n"));
        content.push_str(&text);
        content.push_str("\n\n---\n\n");
    }

    Ok(content)
}

/// Run inline mode: call Anthropic Messages API with streaming.
/// Returns the parsed MortgageAnswer if the response contains valid JSON.
#[tracing::instrument(
    name = "ai.anthropic_messages",
    skip(_client, config, tx),
    fields(
        gen_ai.system = "anthropic",
        gen_ai.operation.name = "messages_api",
        gen_ai.request.model = %config.model,
        message_len = message.len(),
    )
)]
pub async fn run_inline(
    _client: &reqwest::Client,
    config: &InlineConfig,
    message: &str,
    tx: mpsc::Sender<ChatEvent>,
) -> Result<Option<MortgageAnswer>> {
    let system_prompt = build_system_prompt(config)?;

    tracing::info!(
        model = %config.model,
        system_prompt_len = system_prompt.len(),
        message_len = message.len(),
        "Starting inline Messages API call"
    );

    // Use structured system prompt with cache_control breakpoint so Anthropic
    // caches the large policy documents (~100K tokens) across requests.
    let body = serde_json::json!({
        "model": &config.model,
        "max_tokens": 4096,
        "stream": true,
        "system": [{
            "type": "text",
            "text": system_prompt,
            "cache_control": { "type": "ephemeral" }
        }],
        "messages": [{ "role": "user", "content": message }]
    });

    // Use a dedicated client with NO total timeout — the Anthropic streaming
    // response can take minutes. We only set connect and read (per-chunk) timeouts.
    let streaming_client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(30))
        .read_timeout(std::time::Duration::from_secs(120))
        .build()
        .context("Failed to build streaming HTTP client")?;

    // Determine if this is an OAuth token or API key
    let is_oauth = config.api_key.starts_with("sk-ant-oat");

    let mut req = streaming_client
        .post("https://api.anthropic.com/v1/messages")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json");

    if is_oauth {
        req = req.header("authorization", format!("Bearer {}", config.api_key));
    } else {
        req = req.header("x-api-key", &config.api_key);
    }

    let resp = req
        .json(&body)
        .send()
        .await
        .context("Anthropic API request failed")?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Anthropic API error {status}: {text}");
    }

    // Process SSE stream — extract the "answer" field from growing JSON and stream it
    let mut full_text = String::new();
    let mut session_id = String::new();
    let mut bytes_stream = resp.bytes_stream();
    let mut last_answer_len = 0usize;

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

                                // Extract the "answer" value from the partial JSON
                                // so the frontend sees readable text, not raw JSON
                                if let Some(answer_text) = extract_partial_answer(&full_text) {
                                    if answer_text.len() > last_answer_len {
                                        last_answer_len = answer_text.len();
                                        let _ = tx
                                            .send(ChatEvent::Partial {
                                                content: answer_text,
                                            })
                                            .await;
                                    }
                                }
                            }
                        }
                        Some("message_stop") => {
                            // Stream complete
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Try to extract structured JSON from the full response
    let mortgage_answer = extract_structured_json(&full_text);

    let structured_output = if let Some(ref answer) = mortgage_answer {
        serde_json::to_value(answer).unwrap_or_else(|_| serde_json::json!({ "answer": &full_text }))
    } else {
        serde_json::json!({ "answer": &full_text })
    };

    let _ = tx
        .send(ChatEvent::Result {
            structured_output,
            session_id,
        })
        .await;

    Ok(mortgage_answer)
}

/// Extract the "answer" field value from a partially-built JSON string.
/// As the LLM streams `{"answer": "text...", "rationale": ...}`, we parse
/// the answer value so the frontend can show readable text instead of raw JSON.
fn extract_partial_answer(text: &str) -> Option<String> {
    // Find the "answer" key — try both with and without space after colon
    let marker_pos = text.find("\"answer\":")?;
    let after_key = text[marker_pos + "\"answer\":".len()..].trim_start();

    if !after_key.starts_with('"') {
        return None;
    }
    let value_content = &after_key[1..]; // skip opening quote

    // Read the string value, handling JSON escape sequences
    let mut result = String::new();
    let mut chars = value_content.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('t') => result.push('\t'),
                Some('"') => result.push('"'),
                Some('\\') => result.push('\\'),
                Some(other) => {
                    result.push('\\');
                    result.push(other);
                }
                None => break, // incomplete escape at end of stream
            }
        } else if c == '"' {
            break; // end of the answer field
        } else {
            result.push(c);
        }
    }

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

/// Try to extract structured JSON from text that may contain markdown code blocks.
fn extract_structured_json(text: &str) -> Option<MortgageAnswer> {
    // Try direct parse
    if let Ok(answer) = serde_json::from_str::<MortgageAnswer>(text) {
        return Some(answer);
    }

    // Try extracting from ```json ... ```
    if let Some(start) = text.find("```json") {
        let json_start = start + 7;
        if let Some(end) = text[json_start..].find("```") {
            let json_str = text[json_start..json_start + end].trim();
            if let Ok(answer) = serde_json::from_str::<MortgageAnswer>(json_str) {
                return Some(answer);
            }
        }
    }

    // Try extracting from ``` ... ```
    if let Some(start) = text.find("```\n") {
        let json_start = start + 4;
        if let Some(end) = text[json_start..].find("```") {
            let json_str = text[json_start..json_start + end].trim();
            if let Ok(answer) = serde_json::from_str::<MortgageAnswer>(json_str) {
                return Some(answer);
            }
        }
    }

    // Try finding JSON object in text
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            let json_str = &text[start..=end];
            if let Ok(answer) = serde_json::from_str::<MortgageAnswer>(json_str) {
                return Some(answer);
            }
        }
    }

    None
}

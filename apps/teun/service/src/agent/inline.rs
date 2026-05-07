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
                .unwrap_or_else(|_| "claude-opus-4-6".to_string()),
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
         4. Als informatie niet in de documenten te vinden is, zeg dat eerlijk en vraag om verduidelijking — verzin NOOIT informatie, paginanummers, secties of citaten die niet in de documenten staan.\n\
         5. Noteer bij het lezen met Read de **regelnummers** (de nummers links van de tekst) van relevante passages. Gebruik deze als `line_range` in de bronverwijzingen. Het `line_range` veld MOET numeriek zijn, bijv. \"120-135\" of \"42\". NOOIT secienamen of tekst in dit veld.\n\
         6. Kopieer het relevante citaat LETTERLIJK uit het document voor het `quote` veld. Dit citaat wordt getoond aan de gebruiker als bewijs. De quote wordt automatisch geverifieerd tegen het document op de opgegeven regelnummers — als de quote niet overeenkomt, wordt de bron als onbetrouwbaar gemarkeerd.",
        "1. De volledige beleidsdocumenten staan hieronder in de systeemprompt. Elke regel begint met een regelnummer gevolgd door ': ', bijv. '1293: tekst'.\n\
         2. Baseer je antwoord UITSLUITEND op wat er in de documenten staat.\n\
         3. Als informatie niet in de documenten te vinden is, zeg dat eerlijk — verzin NOOIT informatie.\n\
         4. Gebruik het zichtbare regelnummer als `line_range` in bronverwijzingen (bijv. \"1293\" of \"1293-1295\"). NOOIT sectienamen of tekst in dit veld.\n\
         5. Kopieer het relevante citaat LETTERLIJK uit het document voor het `quote` veld, maar ZONDER het regelnummer-prefix (dus NIET '1293: tekst', maar gewoon 'tekst').",
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
        for (i, line) in text.lines().enumerate() {
            content.push_str(&format!("{}: {}\n", i + 1, line));
        }
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

    // Process SSE stream — stream the answer text directly, parse JSON after separator
    // Expected format: "<answer text>\n---JSON---\n{...}"
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

    // Parse structured output from the two-phase response
    let mortgage_answer = parse_two_phase_response(&full_text);

    let structured_output = if let Some(ref answer) = mortgage_answer {
        serde_json::to_value(answer).unwrap_or_else(|_| serde_json::json!({ "answer": &full_text }))
    } else {
        serde_json::json!({ "answer": full_text.trim() })
    };

    let _ = tx
        .send(ChatEvent::Result {
            structured_output,
            session_id,
        })
        .await;

    Ok(mortgage_answer)
}

/// Parse a two-phase response: plain text answer followed by `---JSON---` and structured data.
/// Returns a MortgageAnswer combining the streamed text with the parsed JSON metadata.
fn parse_two_phase_response(text: &str) -> Option<MortgageAnswer> {
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

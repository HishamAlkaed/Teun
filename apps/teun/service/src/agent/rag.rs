//! Single-call RAG answer path (Plan 01-04, RET-05).
//!
//! `run_rag`: embed the query → retrieve the top-K most similar chunks from
//! pgvector → one streaming generation call (Anthropic Messages API or Azure
//! OpenAI chat completions, switched via `LLM_PROVIDER`) → two-phase
//! `MortgageAnswer` + `ToolEvidence` assembled from the retrieved chunks'
//! canonical line ranges (the option-A verifier mitigation: the judge gets
//! real chunk-derived evidence, not `ToolEvidence::default()`).
//!
//! Logging discipline (threat T-04-02): log prompt length + chunk count only —
//! never chunk contents and never API keys.

use anyhow::{bail, Context, Result};
use tokio::sync::mpsc;

use super::stream::{self, SseDialect};
use super::types::{ChatEvent, MortgageAnswer, ToolEvidence};
use crate::rag::embed::{embed_query, EmbedConfig};
use crate::rag::store::{RagStore, RetrievedChunk};

/// Number of chunks retrieved per query (CONTEXT: top-K = 8, no similarity
/// threshold in Phase 1).
const TOP_K: i64 = 8;

/// The generation LLM provider, selected via `LLM_PROVIDER`
/// (`anthropic` default | `azure-openai`).
pub enum GenProvider {
    /// Anthropic Messages API. Model from `RAG_MODEL`
    /// (fallback `INLINE_MODEL` → `CLAUDE_MODEL`).
    Anthropic { api_key: String, model: String },
    /// Azure OpenAI chat completions. `AZURE_OPENAI_CHAT_DEPLOYMENT` is a
    /// SEPARATE deployment from the embeddings one; endpoint/key/api-version
    /// are shared with the embeddings config (Plan 01).
    AzureOpenAi {
        endpoint: String,
        api_key: String,
        deployment: String,
        api_version: String,
    },
}

/// Configuration for the RAG answer path.
pub struct RagConfig {
    pub provider: GenProvider,
    pub skill_path: String,
}

impl RagConfig {
    /// Build from environment. Unlike the old `InlineConfig`, there is NO
    /// OAuth-token-from-disk fallback (threat T-04-03: the answer path uses
    /// env API keys only).
    pub fn from_env() -> Result<Self> {
        let provider = provider_from_lookup(&|var| {
            std::env::var(var).ok().filter(|v| !v.trim().is_empty())
        })?;

        let project_root = super::find_project_root().unwrap_or_else(|| ".".to_string());
        let skill_path = std::env::var("SKILL_PATH").unwrap_or_else(|_| {
            // Container: /app/config/acceptatie-beleid.md
            // Local dev: {root}/apps/teun/service/config/acceptatie-beleid.md
            let container_path = format!("{}/config/acceptatie-beleid.md", project_root);
            let dev_path = format!("{}/apps/teun/service/config/acceptatie-beleid.md", project_root);
            if std::path::Path::new(&container_path).exists() {
                container_path
            } else {
                dev_path
            }
        });

        Ok(Self { provider, skill_path })
    }

    /// The `gen_ai.system` value recorded on the `ai.rag` span (Langfuse).
    pub fn system(&self) -> &'static str {
        match &self.provider {
            GenProvider::Anthropic { .. } => "anthropic",
            GenProvider::AzureOpenAi { .. } => "azure-openai",
        }
    }

    /// The `gen_ai.request.model` value recorded on the `ai.rag` span:
    /// the Anthropic model id or the Azure deployment name.
    pub fn model_name(&self) -> &str {
        match &self.provider {
            GenProvider::Anthropic { model, .. } => model,
            GenProvider::AzureOpenAi { deployment, .. } => deployment,
        }
    }
}

/// Resolve the generation provider from an env-var lookup (factored out of
/// `from_env` so the `LLM_PROVIDER` switch is unit-testable offline).
fn provider_from_lookup(get: &dyn Fn(&str) -> Option<String>) -> Result<GenProvider> {
    let provider = get("LLM_PROVIDER").unwrap_or_else(|| "anthropic".to_string());
    match provider.as_str() {
        "anthropic" => {
            let api_key = get("JUDGE_API_KEY")
                .or_else(|| get("ANTHROPIC_API_KEY"))
                .context("No API key for the RAG answer path. Set ANTHROPIC_API_KEY (or JUDGE_API_KEY).")?;
            let model = get("RAG_MODEL")
                .or_else(|| get("INLINE_MODEL"))
                .or_else(|| get("CLAUDE_MODEL"))
                .unwrap_or_else(|| "claude-opus-4-6".to_string());
            Ok(GenProvider::Anthropic { api_key, model })
        }
        "azure-openai" => {
            let endpoint = get("AZURE_OPENAI_ENDPOINT")
                .context("LLM_PROVIDER=azure-openai requires AZURE_OPENAI_ENDPOINT")?;
            let api_key = get("AZURE_OPENAI_API_KEY")
                .context("LLM_PROVIDER=azure-openai requires AZURE_OPENAI_API_KEY")?;
            let deployment = get("AZURE_OPENAI_CHAT_DEPLOYMENT").context(
                "LLM_PROVIDER=azure-openai requires AZURE_OPENAI_CHAT_DEPLOYMENT \
                 (a CHAT deployment, separate from the embeddings deployment)",
            )?;
            let api_version = get("AZURE_OPENAI_API_VERSION")
                .context("LLM_PROVIDER=azure-openai requires AZURE_OPENAI_API_VERSION")?;
            Ok(GenProvider::AzureOpenAi {
                endpoint: endpoint.trim_end_matches('/').to_string(),
                api_key,
                deployment,
                api_version,
            })
        }
        other => bail!("Unknown LLM_PROVIDER '{other}' (expected 'anthropic' or 'azure-openai')"),
    }
}

/// Build the RAG system prompt: the skill file with its tool-usage
/// instructions replaced by fragment-aware instructions, followed by the
/// retrieved chunks formatted with visible canonical line numbers.
///
/// Deliberately does NOT reuse inline.rs's "De volledige beleidsdocumenten
/// staan hieronder" wording — that asserted the COMPLETE corpus was present,
/// which is false here: the prompt contains only the top-K fragments.
fn build_system_prompt(config: &RagConfig, chunks: &[RetrievedChunk]) -> Result<String> {
    let mut skill = std::fs::read_to_string(&config.skill_path)
        .context("Failed to read skill file")?;

    // Replace tool-usage instructions with retrieved-fragment instructions
    skill = skill.replace(
        "1. Gebruik ALTIJD de Grep en Read tools om de beleidsdocumenten te doorzoeken voordat je antwoordt.\n\
         2. Doorzoek eerst met Grep op relevante termen, lees dan de gevonden secties met Read.\n\
         3. Baseer je antwoord UITSLUITEND op wat er in de documenten staat.\n\
         4. Als informatie niet in de documenten te vinden is, zeg dat eerlijk en vraag om verduidelijking — verzin NOOIT informatie, paginanummers, secties of citaten die niet in de documenten staan.\n\
         5. Noteer bij het lezen met Read de **regelnummers** (de nummers links van de tekst) van relevante passages. Gebruik deze als `line_range` in de bronverwijzingen. Het `line_range` veld MOET numeriek zijn, bijv. \"120-135\" of \"42\". NOOIT secienamen of tekst in dit veld.\n\
         6. Kopieer het relevante citaat LETTERLIJK uit het document voor het `quote` veld. Dit citaat wordt getoond aan de gebruiker als bewijs. De quote wordt automatisch geverifieerd tegen het document op de opgegeven regelnummers — als de quote niet overeenkomt, wordt de bron als onbetrouwbaar gemarkeerd.",
        "1. Hieronder staan de meest relevante fragmenten uit de beleidsdocumenten, opgehaald voor deze specifieke vraag. Dit is NIET het volledige corpus. Elke regel begint met een regelnummer gevolgd door ': ', bijv. '1293: tekst'.\n\
         2. Baseer je antwoord UITSLUITEND op wat er in deze fragmenten staat.\n\
         3. Als informatie niet in de fragmenten staat, betekent dat NIET automatisch dat het niet in het beleid staat — zeg dan eerlijk dat het niet in de opgehaalde passages voorkomt en verzin NOOIT informatie, paginanummers, secties of citaten.\n\
         4. Gebruik het zichtbare regelnummer als `line_range` in bronverwijzingen (bijv. \"1293\" of \"1293-1295\"). NOOIT sectienamen of tekst in dit veld.\n\
         5. Kopieer het relevante citaat LETTERLIJK uit het fragment voor het `quote` veld, maar ZONDER het regelnummer-prefix (dus NIET '1293: tekst', maar gewoon 'tekst').",
    );

    skill.push_str("\n\n---\n\n## Beleidsdocumenten (meest relevante fragmenten)\n\n");
    skill.push_str(&format_chunks(chunks));

    Ok(skill)
}

/// Format retrieved chunks for the prompt: a `### Document:` header per chunk
/// and every line prefixed with its canonical line number (counter starts at
/// `chunk.line_start`), matching the inline numbering convention so
/// `line_range` citations resolve against the canonical body.
fn format_chunks(chunks: &[RetrievedChunk]) -> String {
    let mut out = String::new();
    for chunk in chunks {
        out.push_str(&format!(
            "### Document: {} (pagina {})\n\n",
            chunk.document, chunk.page
        ));
        for (i, line) in chunk.content.lines().enumerate() {
            out.push_str(&format!("{}: {}\n", chunk.line_start as i64 + i as i64, line));
        }
        out.push_str("\n\n---\n\n");
    }
    out
}

/// Assemble `ToolEvidence` from the retrieved chunks' canonical line ranges —
/// the option-A mitigation: the judge verifies citations against the line
/// ranges that were actually put in front of the model.
fn build_evidence(chunks: &[RetrievedChunk]) -> ToolEvidence {
    let mut evidence = ToolEvidence::default();
    for chunk in chunks {
        evidence.add_range(
            &chunk.document,
            chunk.line_start.max(1) as usize,
            chunk.line_end.max(1) as usize,
        );
    }
    evidence
}

/// Anthropic Messages API request body (streaming, cache_control on the
/// system block like the old inline path).
fn anthropic_request_body(model: &str, system_prompt: &str, message: &str) -> serde_json::Value {
    serde_json::json!({
        "model": model,
        "max_tokens": 4096,
        "stream": true,
        "system": [{
            "type": "text",
            "text": system_prompt,
            "cache_control": { "type": "ephemeral" }
        }],
        "messages": [{ "role": "user", "content": message }]
    })
}

/// Azure OpenAI chat completions request body (streaming). Uses
/// `max_completion_tokens`: newer Azure chat models (e.g. gpt-5.x) REJECT
/// `max_tokens` with HTTP 400. The budget is larger than Anthropic's because
/// reasoning models spend completion tokens on hidden reasoning first.
fn azure_request_body(system_prompt: &str, message: &str) -> serde_json::Value {
    serde_json::json!({
        "stream": true,
        "max_completion_tokens": 8192,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": message }
        ]
    })
}

/// Run the RAG answer path: embed → retrieve top-K → single streaming
/// generation call → two-phase MortgageAnswer + chunk-derived ToolEvidence.
#[tracing::instrument(
    name = "ai.rag",
    skip(client, store, embed_cfg, config, tx),
    fields(
        gen_ai.system = config.system(),
        gen_ai.operation.name = "rag_answer",
        gen_ai.request.model = %config.model_name(),
        message_len = message.len(),
    )
)]
pub async fn run_rag(
    client: &reqwest::Client,
    store: &RagStore,
    embed_cfg: &EmbedConfig,
    config: &RagConfig,
    message: &str,
    tx: mpsc::Sender<ChatEvent>,
) -> Result<(Option<MortgageAnswer>, ToolEvidence)> {
    // (1) Embed the user query.
    let query_embedding = embed_query(client, embed_cfg, message)
        .await
        .context("Failed to embed query")?;

    // (2) Retrieve the top-K most similar chunks.
    let chunks = store
        .search(query_embedding, TOP_K)
        .await
        .context("Top-K retrieval failed")?;

    // (3) Chunk-derived evidence for the judge (option-A mitigation).
    let evidence = build_evidence(&chunks);

    // (4) System prompt = skill instructions + retrieved fragments.
    let system_prompt = build_system_prompt(config, &chunks)?;

    tracing::info!(
        provider = config.system(),
        model = %config.model_name(),
        chunks = chunks.len(),
        system_prompt_len = system_prompt.len(),
        message_len = message.len(),
        "Starting RAG generation call"
    );

    // (5) Streaming generation call. Dedicated client with NO total timeout —
    // a streaming response can take minutes. Only connect and read (per-chunk)
    // timeouts are set (same pattern as the old inline path).
    let streaming_client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(30))
        .read_timeout(std::time::Duration::from_secs(120))
        .build()
        .context("Failed to build streaming HTTP client")?;

    let (resp, dialect) = match &config.provider {
        GenProvider::Anthropic { api_key, model } => {
            let resp = streaming_client
                .post("https://api.anthropic.com/v1/messages")
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .header("x-api-key", api_key)
                .json(&anthropic_request_body(model, &system_prompt, message))
                .send()
                .await
                .context("Anthropic API request failed")?;
            (resp, SseDialect::Anthropic)
        }
        GenProvider::AzureOpenAi {
            endpoint,
            api_key,
            deployment,
            api_version,
        } => {
            let url = format!(
                "{endpoint}/openai/deployments/{deployment}/chat/completions?api-version={api_version}"
            );
            let resp = streaming_client
                .post(&url)
                .header("api-key", api_key)
                .header("content-type", "application/json")
                .json(&azure_request_body(&system_prompt, message))
                .send()
                .await
                .context("Azure OpenAI chat request failed")?;
            (resp, SseDialect::AzureOpenAi)
        }
    };

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Generation API error {status}: {text}");
    }

    // Shared two-phase streaming loop; only the SSE decode is provider-specific.
    let (full_text, session_id) = stream::stream_two_phase(resp, dialect, &tx).await?;

    // (6) Parse the two-phase response and emit the Result event
    // (shape identical to the old inline path).
    let mortgage_answer = stream::parse_two_phase_response(&full_text);

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

    Ok((mortgage_answer, evidence))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn lookup(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |var: &str| map.get(var).cloned()
    }

    fn chunk(document: &str, content: &str, line_start: i32, line_end: i32, page: i32) -> RetrievedChunk {
        RetrievedChunk {
            content: content.to_string(),
            document: document.to_string(),
            line_start,
            line_end,
            page,
        }
    }

    #[test]
    fn provider_defaults_to_anthropic_with_rag_model_priority() {
        let get = lookup(&[
            ("ANTHROPIC_API_KEY", "sk-test"),
            ("RAG_MODEL", "claude-x"),
            ("INLINE_MODEL", "claude-y"),
        ]);
        let provider = provider_from_lookup(&get).expect("resolves");
        match provider {
            GenProvider::Anthropic { api_key, model } => {
                assert_eq!(api_key, "sk-test");
                assert_eq!(model, "claude-x"); // RAG_MODEL wins over INLINE_MODEL
            }
            _ => panic!("expected Anthropic"),
        }
    }

    #[test]
    fn anthropic_model_falls_back_inline_then_claude_then_default() {
        let get = lookup(&[("ANTHROPIC_API_KEY", "k"), ("INLINE_MODEL", "claude-y")]);
        match provider_from_lookup(&get).expect("resolves") {
            GenProvider::Anthropic { model, .. } => assert_eq!(model, "claude-y"),
            _ => panic!("expected Anthropic"),
        }
        let get = lookup(&[("ANTHROPIC_API_KEY", "k"), ("CLAUDE_MODEL", "claude-z")]);
        match provider_from_lookup(&get).expect("resolves") {
            GenProvider::Anthropic { model, .. } => assert_eq!(model, "claude-z"),
            _ => panic!("expected Anthropic"),
        }
        let get = lookup(&[("ANTHROPIC_API_KEY", "k")]);
        match provider_from_lookup(&get).expect("resolves") {
            GenProvider::Anthropic { model, .. } => assert_eq!(model, "claude-opus-4-6"),
            _ => panic!("expected Anthropic"),
        }
    }

    #[test]
    fn anthropic_without_api_key_errors() {
        let get = lookup(&[]);
        assert!(provider_from_lookup(&get).is_err());
    }

    #[test]
    fn azure_provider_requires_chat_deployment() {
        let get = lookup(&[
            ("LLM_PROVIDER", "azure-openai"),
            ("AZURE_OPENAI_ENDPOINT", "https://example.cognitiveservices.azure.com/"),
            ("AZURE_OPENAI_API_KEY", "secret"),
            ("AZURE_OPENAI_API_VERSION", "2024-02-01"),
        ]);
        // No Debug derive on GenProvider (it carries API keys), so match
        // instead of expect_err.
        match provider_from_lookup(&get) {
            Err(err) => assert!(format!("{err:#}").contains("AZURE_OPENAI_CHAT_DEPLOYMENT")),
            Ok(_) => panic!("expected an error for the missing chat deployment"),
        }
    }

    #[test]
    fn azure_provider_resolves_and_trims_endpoint() {
        let get = lookup(&[
            ("LLM_PROVIDER", "azure-openai"),
            ("AZURE_OPENAI_ENDPOINT", "https://example.cognitiveservices.azure.com/"),
            ("AZURE_OPENAI_API_KEY", "secret"),
            ("AZURE_OPENAI_CHAT_DEPLOYMENT", "gpt-5.6-luna"),
            ("AZURE_OPENAI_API_VERSION", "2024-02-01"),
        ]);
        match provider_from_lookup(&get).expect("resolves") {
            GenProvider::AzureOpenAi { endpoint, deployment, .. } => {
                assert_eq!(endpoint, "https://example.cognitiveservices.azure.com");
                assert_eq!(deployment, "gpt-5.6-luna");
            }
            _ => panic!("expected AzureOpenAi"),
        }
    }

    #[test]
    fn unknown_provider_errors() {
        let get = lookup(&[("LLM_PROVIDER", "openrouter")]);
        assert!(provider_from_lookup(&get).is_err());
    }

    #[test]
    fn format_chunks_numbers_lines_from_line_start() {
        let chunks = vec![
            chunk("gids.pdf", "eerste regel\ntweede regel", 120, 121, 7),
            chunk("handboek.pdf", "enige regel", 1, 1, 1),
        ];
        let out = format_chunks(&chunks);
        assert!(out.contains("### Document: gids.pdf (pagina 7)"));
        assert!(out.contains("120: eerste regel\n121: tweede regel\n"));
        assert!(out.contains("### Document: handboek.pdf (pagina 1)"));
        assert!(out.contains("1: enige regel\n"));
    }

    #[test]
    fn build_evidence_collects_chunk_line_ranges_per_document() {
        let chunks = vec![
            chunk("gids.pdf", "a", 10, 20, 1),
            chunk("gids.pdf", "b", 100, 140, 3),
            chunk("handboek.pdf", "c", 5, 9, 1),
        ];
        let evidence = build_evidence(&chunks);
        assert_eq!(
            evidence.accessed_ranges.get("gids.pdf"),
            Some(&vec![(10usize, 20usize), (100, 140)])
        );
        assert_eq!(
            evidence.accessed_ranges.get("handboek.pdf"),
            Some(&vec![(5usize, 9usize)])
        );
    }

    #[test]
    fn anthropic_body_streams_with_cached_system_block() {
        let body = anthropic_request_body("claude-x", "SYSTEM", "vraag");
        assert_eq!(body["model"], "claude-x");
        assert_eq!(body["stream"], true);
        assert_eq!(body["max_tokens"], 4096);
        assert_eq!(body["system"][0]["text"], "SYSTEM");
        assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "vraag");
    }

    #[test]
    fn azure_body_uses_max_completion_tokens_not_max_tokens() {
        let body = azure_request_body("SYSTEM", "vraag");
        assert_eq!(body["stream"], true);
        // gpt-5.x deployments reject "max_tokens" with HTTP 400.
        assert!(body.get("max_tokens").is_none());
        assert_eq!(body["max_completion_tokens"], 8192);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][0]["content"], "SYSTEM");
        assert_eq!(body["messages"][1]["role"], "user");
        assert_eq!(body["messages"][1]["content"], "vraag");
    }

    /// Live end-to-end RAG answer smoke test: embeds a real question,
    /// retrieves from the seeded corpus and runs the full generation call
    /// with whatever LLM_PROVIDER is configured. Gated on TEUN_SMOKE_QUERY
    /// (+ DATABASE_URL, embeddings credentials and generation credentials).
    /// Run with --nocapture to see the answer/sources/evidence. Prints answer
    /// content and metadata only — never keys or the full prompt.
    #[tokio::test]
    #[ignore = "requires DATABASE_URL, embeddings + generation credentials and TEUN_SMOKE_QUERY"]
    async fn live_rag_answer_smoke() {
        let question = std::env::var("TEUN_SMOKE_QUERY").expect("set TEUN_SMOKE_QUERY");
        let url = std::env::var("DATABASE_URL").expect("set DATABASE_URL");
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to database");
        let store = RagStore::new(pool);
        let embed_cfg = EmbedConfig::from_env().expect("embed config");
        let config = RagConfig::from_env().expect("rag config");
        let client = reqwest::Client::new();

        let (tx, mut rx) = mpsc::channel::<ChatEvent>(64);
        let drain = tokio::spawn(async move {
            let mut events: Vec<ChatEvent> = Vec::new();
            while let Some(event) = rx.recv().await {
                events.push(event);
            }
            events
        });

        println!("provider: {} / model: {}", config.system(), config.model_name());
        let (answer, evidence) = run_rag(&client, &store, &embed_cfg, &config, &question, tx)
            .await
            .expect("run_rag");
        let events = drain.await.expect("drain events");

        let partials = events.iter().filter(|e| matches!(e, ChatEvent::Partial { .. })).count();
        let results = events.iter().filter(|e| matches!(e, ChatEvent::Result { .. })).count();
        println!("events: {partials} partial, {results} result");
        assert!(partials >= 1, "expected at least one Partial event");
        assert_eq!(results, 1, "expected exactly one Result event");

        let answer = answer.expect("two-phase MortgageAnswer parsed");
        println!("question: {question}");
        println!("answer:\n{}\n", answer.answer);
        println!("category: {:?}", answer.category);
        for source in &answer.sources {
            println!(
                "source: {} [{}] lines={:?} quote={:?}",
                source.document, source.section, source.line_range, source.quote
            );
        }
        println!("evidence ranges: {:?}", evidence.accessed_ranges);
        assert!(
            !evidence.accessed_ranges.is_empty(),
            "evidence must be assembled from retrieved chunks"
        );
    }
}

//! Embeddings client for OpenAI `text-embedding-3-large` (3072 dims).
//!
//! Azure OpenAI is the PRIMARY provider (env branch checked first);
//! plain OpenAI is the fallback. Requests reuse the caller-supplied
//! `reqwest::Client` (`state.http_client`).
//!
//! Logging discipline (threat T-01-01): log input counts / lengths only —
//! NEVER the API key and NEVER full embedding vectors.

use anyhow::{bail, Context, Result};
use serde::Deserialize;

/// OpenAI caps embedding requests at 2048 inputs per call (research pitfall 4).
const MAX_BATCH_SIZE: usize = 2048;

/// Expected output dimension of `text-embedding-3-large`.
pub const EMBED_DIM: usize = 3072;

/// Provider configuration, resolved from env once at startup.
#[derive(Clone)]
pub enum EmbedConfig {
    /// Azure OpenAI: `{endpoint}/openai/deployments/{deployment}/embeddings?api-version=...`
    /// with an `api-key` header. The deployment selects the model, so the
    /// request body carries no `model` field.
    Azure {
        endpoint: String,
        api_key: String,
        deployment: String,
        api_version: String,
    },
    /// Plain OpenAI: `https://api.openai.com/v1/embeddings` with a Bearer token.
    OpenAi { api_key: String, model: String },
}

impl EmbedConfig {
    /// Resolve provider config from env. The Azure branch is PRIMARY: it wins
    /// whenever all four `AZURE_OPENAI_*` vars are set (non-empty). Otherwise
    /// falls back to `OPENAI_API_KEY` (+ optional `EMBEDDING_MODEL`).
    pub fn from_env() -> Result<Self> {
        fn non_empty(var: &str) -> Option<String> {
            std::env::var(var).ok().filter(|v| !v.trim().is_empty())
        }

        let azure = (
            non_empty("AZURE_OPENAI_ENDPOINT"),
            non_empty("AZURE_OPENAI_API_KEY"),
            non_empty("AZURE_OPENAI_DEPLOYMENT"),
            non_empty("AZURE_OPENAI_API_VERSION"),
        );
        if let (Some(endpoint), Some(api_key), Some(deployment), Some(api_version)) = azure {
            return Ok(EmbedConfig::Azure {
                endpoint: endpoint.trim_end_matches('/').to_string(),
                api_key,
                deployment,
                api_version,
            });
        }

        if let Some(api_key) = non_empty("OPENAI_API_KEY") {
            let model = non_empty("EMBEDDING_MODEL")
                .unwrap_or_else(|| "text-embedding-3-large".to_string());
            return Ok(EmbedConfig::OpenAi { api_key, model });
        }

        bail!(
            "No embedding provider configured: set AZURE_OPENAI_ENDPOINT/AZURE_OPENAI_API_KEY/\
             AZURE_OPENAI_DEPLOYMENT/AZURE_OPENAI_API_VERSION (primary) or OPENAI_API_KEY (fallback)"
        )
    }

    fn url(&self) -> String {
        match self {
            EmbedConfig::Azure {
                endpoint,
                deployment,
                api_version,
                ..
            } => format!(
                "{endpoint}/openai/deployments/{deployment}/embeddings?api-version={api_version}"
            ),
            EmbedConfig::OpenAi { .. } => "https://api.openai.com/v1/embeddings".to_string(),
        }
    }

    fn provider_name(&self) -> &'static str {
        match self {
            EmbedConfig::Azure { .. } => "azure-openai",
            EmbedConfig::OpenAi { .. } => "openai",
        }
    }

    fn request_body(&self, batch: &[String]) -> serde_json::Value {
        match self {
            // Azure: the deployment already selects the model; omit `model`.
            // `dimensions` is omitted everywhere — text-embedding-3-large
            // returns its native 3072 dims by default.
            EmbedConfig::Azure { .. } => serde_json::json!({ "input": batch }),
            EmbedConfig::OpenAi { model, .. } => {
                serde_json::json!({ "model": model, "input": batch })
            }
        }
    }
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingItem>,
}

#[derive(Deserialize)]
struct EmbeddingItem {
    index: usize,
    embedding: Vec<f32>,
}

/// Decode an embeddings API response body, reordering by the `index` field
/// (the API does not guarantee input order). Pure — unit-testable offline.
fn parse_embedding_response(body: &str, expected: usize) -> Result<Vec<Vec<f32>>> {
    let resp: EmbeddingResponse =
        serde_json::from_str(body).context("Failed to decode embeddings response JSON")?;
    if resp.data.len() != expected {
        bail!(
            "Embeddings response has {} items, expected {}",
            resp.data.len(),
            expected
        );
    }
    let mut items = resp.data;
    items.sort_by_key(|item| item.index);
    Ok(items.into_iter().map(|item| item.embedding).collect())
}

/// Split inputs into API-sized sub-batches (≤ MAX_BATCH_SIZE each, order
/// preserved). Pure — unit-testable offline.
fn split_into_batches(inputs: &[String]) -> Vec<&[String]> {
    inputs.chunks(MAX_BATCH_SIZE).collect()
}

/// Take exactly one vector out of a batch result (the `embed_query` contract).
fn take_single(mut results: Vec<Vec<f32>>) -> Result<Vec<f32>> {
    if results.len() != 1 {
        bail!("Expected exactly 1 embedding, got {}", results.len());
    }
    Ok(results.pop().expect("length checked above"))
}

/// Embed a batch of texts. Requests are split into ≤2048-input sub-requests
/// and the results are concatenated in input order.
pub async fn embed_batch(
    client: &reqwest::Client,
    cfg: &EmbedConfig,
    inputs: &[String],
) -> Result<Vec<Vec<f32>>> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }

    let batches = split_into_batches(inputs);
    tracing::info!(
        provider = cfg.provider_name(),
        inputs = inputs.len(),
        requests = batches.len(),
        total_chars = inputs.iter().map(|s| s.len()).sum::<usize>(),
        "Embedding batch"
    );

    let mut results: Vec<Vec<f32>> = Vec::with_capacity(inputs.len());
    for batch in batches {
        let request = client.post(cfg.url()).json(&cfg.request_body(batch));
        let request = match cfg {
            EmbedConfig::Azure { api_key, .. } => request.header("api-key", api_key),
            EmbedConfig::OpenAi { api_key, .. } => request.bearer_auth(api_key),
        };

        let response = request
            .send()
            .await
            .context("Embeddings request failed to send")?;
        let status = response.status();
        let body = response
            .text()
            .await
            .context("Failed to read embeddings response body")?;
        if !status.is_success() {
            // Body may contain provider error details; it never echoes the key.
            bail!(
                "Embeddings request failed: HTTP {} — {}",
                status,
                body.chars().take(500).collect::<String>()
            );
        }

        let vectors = parse_embedding_response(&body, batch.len())?;
        results.extend(vectors);
    }

    tracing::debug!(
        vectors = results.len(),
        dims = results.first().map(|v| v.len()).unwrap_or(0),
        "Embedding batch complete"
    );
    Ok(results)
}

/// Embed a single query string — convenience wrapper returning one vector.
pub async fn embed_query(
    client: &reqwest::Client,
    cfg: &EmbedConfig,
    query: &str,
) -> Result<Vec<f32>> {
    let results = embed_batch(client, cfg, std::slice::from_ref(&query.to_string())).await?;
    take_single(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_vector(fill: f32) -> Vec<f32> {
        vec![fill; EMBED_DIM]
    }

    #[test]
    fn parse_reorders_by_index() {
        // Items deliberately out of order: index 1 first.
        let body = serde_json::json!({
            "data": [
                { "index": 1, "embedding": fake_vector(1.0) },
                { "index": 0, "embedding": fake_vector(0.0) }
            ],
            "model": "text-embedding-3-large",
            "usage": { "prompt_tokens": 42, "total_tokens": 42 }
        })
        .to_string();

        let vectors = parse_embedding_response(&body, 2).expect("decode");
        assert_eq!(vectors.len(), 2);
        assert_eq!(vectors[0].len(), EMBED_DIM);
        assert_eq!(vectors[1].len(), EMBED_DIM);
        assert_eq!(vectors[0][0], 0.0); // index 0 first after reorder
        assert_eq!(vectors[1][0], 1.0);
    }

    #[test]
    fn parse_rejects_count_mismatch() {
        let body = serde_json::json!({
            "data": [ { "index": 0, "embedding": fake_vector(0.0) } ]
        })
        .to_string();
        assert!(parse_embedding_response(&body, 2).is_err());
    }

    #[test]
    fn single_query_yields_one_3072_vector() {
        // The embed_query contract: exactly one vector of native 3072 dims.
        let single = take_single(vec![fake_vector(0.5)]).expect("single vector");
        assert_eq!(single.len(), EMBED_DIM);
        assert!(take_single(vec![]).is_err());
        assert!(take_single(vec![fake_vector(0.0), fake_vector(1.0)]).is_err());
    }

    #[test]
    fn splits_oversized_batches_at_2048() {
        let inputs: Vec<String> = (0..(MAX_BATCH_SIZE + 3)).map(|i| format!("t{i}")).collect();
        let batches = split_into_batches(&inputs);
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].len(), MAX_BATCH_SIZE);
        assert_eq!(batches[1].len(), 3);
        // Order preserved on concatenation.
        assert_eq!(batches[0][0], "t0");
        assert_eq!(batches[1][0], format!("t{}", MAX_BATCH_SIZE));
        assert_eq!(batches[1][2], format!("t{}", MAX_BATCH_SIZE + 2));
    }

    #[test]
    fn azure_config_builds_deployment_url_without_model_in_body() {
        let cfg = EmbedConfig::Azure {
            endpoint: "https://example.cognitiveservices.azure.com".to_string(),
            api_key: "secret".to_string(),
            deployment: "text-embedding-3-large".to_string(),
            api_version: "2024-02-01".to_string(),
        };
        assert_eq!(
            cfg.url(),
            "https://example.cognitiveservices.azure.com/openai/deployments/text-embedding-3-large/embeddings?api-version=2024-02-01"
        );
        let body = cfg.request_body(&["hallo".to_string()]);
        assert!(body.get("model").is_none());
        assert!(body.get("dimensions").is_none());
        assert_eq!(body["input"][0], "hallo");
    }

    #[test]
    fn openai_config_includes_model_in_body() {
        let cfg = EmbedConfig::OpenAi {
            api_key: "secret".to_string(),
            model: "text-embedding-3-large".to_string(),
        };
        assert_eq!(cfg.url(), "https://api.openai.com/v1/embeddings");
        let body = cfg.request_body(&["hallo".to_string()]);
        assert_eq!(body["model"], "text-embedding-3-large");
        assert!(body.get("dimensions").is_none());
    }
}

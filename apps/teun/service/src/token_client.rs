use anyhow::{Context, Result};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;

/// OAuth2 client-credentials token cache.
///
/// Fetches a bearer token from the token endpoint and caches it until
/// it expires (with a 30-second safety margin).
#[derive(Clone)]
pub struct TokenClient {
    http: reqwest::Client,
    token_url: String,
    client_id: String,
    client_secret: String,
    cached: Arc<RwLock<Option<CachedToken>>>,
}

struct CachedToken {
    access_token: String,
    expires_at: std::time::Instant,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default = "default_expires_in")]
    expires_in: u64,
}

fn default_expires_in() -> u64 {
    300
}

impl TokenClient {
    pub fn from_env(http: reqwest::Client) -> Option<Self> {
        let client_id = std::env::var("SCRUB_CLIENT_ID").ok()?;
        let client_secret = std::env::var("SCRUB_CLIENT_SECRET").ok()?;
        let token_url = std::env::var("SCRUB_TOKEN_URL")
            .unwrap_or_else(|_| "https://auth.nextepoch.cloud/oauth/token".to_string());

        Some(Self {
            http,
            token_url,
            client_id,
            client_secret,
            cached: Arc::new(RwLock::new(None)),
        })
    }

    pub async fn get_token(&self) -> Result<String> {
        // Check cache
        {
            let cached = self.cached.read().await;
            if let Some(ref t) = *cached {
                if t.expires_at > std::time::Instant::now() {
                    return Ok(t.access_token.clone());
                }
            }
        }

        // Fetch new token
        let resp = self
            .http
            .post(&self.token_url)
            .basic_auth(&self.client_id, Some(&self.client_secret))
            .form(&[("grant_type", "client_credentials")])
            .send()
            .await
            .context("Token request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Token endpoint returned {status}: {body}");
        }

        let token_resp: TokenResponse = resp.json().await.context("Invalid token response")?;

        let expires_at =
            std::time::Instant::now() + std::time::Duration::from_secs(token_resp.expires_in.saturating_sub(30));

        let access_token = token_resp.access_token.clone();

        // Cache
        {
            let mut cached = self.cached.write().await;
            *cached = Some(CachedToken {
                access_token: token_resp.access_token,
                expires_at,
            });
        }

        Ok(access_token)
    }
}

mod agent;
mod error;
mod eval;
mod judge;
mod rag;
mod routes;
mod session;
mod telemetry;
mod token_client;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::DefaultBodyLimit;
use axum::Router;
use sqlx::postgres::PgPoolOptions;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tower_governor::{GovernorLayer, governor::GovernorConfigBuilder};

use crate::session::SessionStore;

pub struct AppState {
    pub sessions: SessionStore,
    pub http_client: reqwest::Client,
    pub judge_config: judge::JudgeConfig,
    pub scrub_service_url: Option<String>,
    pub scrub_token_client: Option<token_client::TokenClient>,
    pub pool: sqlx::PgPool,
    pub eval_store: eval::store::EvalStore,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialise logging (and, when LANGFUSE_ENABLED=true, OTLP export to Langfuse).
    // Keep the provider so we can flush buffered spans on shutdown.
    let telemetry_provider = telemetry::init();

    // Connect to PostgreSQL
    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://localhost/teun".to_string());
    tracing::info!("Connecting to PostgreSQL");

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await?;
    tracing::info!("PostgreSQL connected");

    // Run migrations
    sqlx::migrate!("./migrations").run(&pool).await?;
    tracing::info!("Migrations applied");

    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("Failed to build HTTP client");

    let judge_config = judge::JudgeConfig::from_env();
    tracing::info!(
        model = %judge_config.judge_model,
        resources_dir = %judge_config.resources_dir,
        has_api_key = judge_config.anthropic_api_key.is_some(),
        retry_threshold = judge_config.retry_threshold,
        max_retries = judge_config.max_retries,
        "Judge config loaded"
    );

    let scrub_service_url = std::env::var("SCRUB_SERVICE_URL").ok();
    let scrub_token_client = token_client::TokenClient::from_env(http_client.clone());
    if let Some(ref url) = scrub_service_url {
        tracing::info!(
            url,
            has_auth = scrub_token_client.is_some(),
            "PII scrub service configured"
        );
    }

    let sessions = SessionStore::new(pool.clone());
    let eval_store = eval::store::EvalStore::new(pool.clone());

    // Spawn periodic session cleanup
    let cleanup_pool = pool.clone();
    let retention_days: i64 = std::env::var("CHAT_RETENTION_DAYS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(90);
    tokio::spawn(async move {
        let cleanup_store = SessionStore::new(cleanup_pool);
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;
            match cleanup_store.cleanup_expired(retention_days).await {
                Ok(count) if count > 0 => {
                    tracing::info!(count, "Cleaned up expired sessions");
                }
                Err(e) => {
                    tracing::error!(error = %e, "Session cleanup failed");
                }
                _ => {}
            }
        }
    });

    let state = Arc::new(AppState {
        sessions,
        http_client,
        judge_config,
        scrub_service_url,
        scrub_token_client,
        pool,
        eval_store,
    });

    // API routes
    let mut api = Router::new()
        .merge(routes::chat::router())
        .merge(routes::sessions::router())
        .merge(routes::scrub::router())
        .merge(routes::settings::router())
        .merge(routes::health::router())
        .merge(routes::admin::router())
        .merge(routes::admin_documents::router())
        .merge(routes::documents::router());

    // Optional rate limiting (per-IP). Set RATE_LIMIT_PER_SECOND=0 to disable.
    let rate_per_second: u64 = std::env::var("RATE_LIMIT_PER_SECOND")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    if rate_per_second > 0 {
        let rate_burst: u32 = std::env::var("RATE_LIMIT_BURST")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5);
        let governor_conf = GovernorConfigBuilder::default()
            .per_second(rate_per_second)
            .burst_size(rate_burst)
            .finish()
            .expect("Invalid rate limit config");
        tracing::info!(per_second = rate_per_second, burst = rate_burst, "Rate limiter configured");
        api = api.layer(GovernorLayer::new(governor_conf));
    } else {
        tracing::info!("Rate limiting disabled");
    }

    // Static file serving (SPA fallback)
    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| "./dist".to_string());
    let index_html = format!("{}/index.html", static_dir);
    let spa_fallback = ServeDir::new(&static_dir)
        .fallback(ServeFile::new(&index_html));

    tracing::info!(static_dir = %static_dir, "Static file serving configured");

    let app = Router::new()
        .merge(api)
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
        .fallback_service(spa_fallback);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("teun listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    // Flush any buffered spans to Langfuse before exiting.
    if let Some(provider) = telemetry_provider {
        if let Err(e) = provider.shutdown() {
            tracing::warn!(error = %e, "Failed to flush telemetry on shutdown");
        }
    }

    Ok(())
}

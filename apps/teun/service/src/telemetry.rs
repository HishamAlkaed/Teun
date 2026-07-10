//! Tracing + OpenTelemetry setup.
//!
//! Logs always go to stdout via `tracing_subscriber::fmt`. When `LANGFUSE_ENABLED`
//! is truthy, spans are also exported to Langfuse over OTLP/HTTP so we can see
//! per-request cost and latency in the Langfuse dashboard.
//!
//! Every trace is tagged with the application name `teun` (both as the OTel
//! `service.name` resource attribute and as a Langfuse trace tag) so it can be
//! told apart from the other apps that share the same Langfuse project.

use std::collections::HashMap;

use anyhow::{Context, Result};
use base64::Engine as _;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::KeyValue;
use opentelemetry_otlp::{Protocol, WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::trace::SdkTracerProvider;
use opentelemetry_sdk::Resource;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

/// The application name used to identify this app inside a shared Langfuse project.
pub const APP_NAME: &str = "teun";

/// Initialise tracing. Returns the OTel provider (when Langfuse is enabled) so the
/// caller can flush and shut it down cleanly on exit — batched spans would
/// otherwise be lost when the process ends.
pub fn init() -> Option<SdkTracerProvider> {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let fmt_layer = tracing_subscriber::fmt::layer();

    if !langfuse_enabled() {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();
        return None;
    }

    match build_provider() {
        Ok(provider) => {
            let tracer = provider.tracer(APP_NAME);
            let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt_layer)
                .with(otel_layer)
                .init();
            tracing::info!("Langfuse OTLP tracing enabled (app={APP_NAME})");
            Some(provider)
        }
        Err(e) => {
            // Never let a telemetry misconfiguration take down the service.
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt_layer)
                .init();
            tracing::error!(error = %format!("{e:#}"), "Failed to init Langfuse tracing; continuing without it");
            None
        }
    }
}

fn langfuse_enabled() -> bool {
    std::env::var("LANGFUSE_ENABLED")
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

fn build_provider() -> Result<SdkTracerProvider> {
    let host = std::env::var("LANGFUSE_HOST")
        .unwrap_or_else(|_| "https://cloud.langfuse.com".to_string());
    let public_key =
        std::env::var("LANGFUSE_PUBLIC_KEY").context("LANGFUSE_PUBLIC_KEY is not set")?;
    let secret_key =
        std::env::var("LANGFUSE_SECRET_KEY").context("LANGFUSE_SECRET_KEY is not set")?;

    let endpoint = format!(
        "{}/api/public/otel/v1/traces",
        host.trim_end_matches('/')
    );

    // Langfuse authenticates OTLP with HTTP Basic auth: base64("public:secret").
    let credentials =
        base64::engine::general_purpose::STANDARD.encode(format!("{public_key}:{secret_key}"));
    let mut headers = HashMap::new();
    headers.insert("Authorization".to_string(), format!("Basic {credentials}"));
    headers.insert("x-langfuse-ingestion-version".to_string(), "4".to_string());

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_protocol(Protocol::HttpBinary)
        .with_endpoint(endpoint)
        .with_headers(headers)
        .build()
        .context("Failed to build OTLP span exporter")?;

    // `service.name` is how Langfuse separates apps sharing one project.
    let resource = Resource::builder()
        .with_service_name(APP_NAME)
        .with_attribute(KeyValue::new("service.version", env!("CARGO_PKG_VERSION")))
        .build();

    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(resource)
        .build();

    Ok(provider)
}

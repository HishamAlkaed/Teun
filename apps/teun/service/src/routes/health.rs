use std::sync::Arc;

use axum::{Router, routing::get};

use crate::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/teun/health", get(health))
}

async fn health() -> &'static str {
    "ok"
}

use std::sync::Arc;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::error::AppError;
use crate::AppState;

#[derive(Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default)]
    pub scrub_enabled: bool,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/teun/settings", get(get_settings).put(put_settings))
}

async fn get_settings(
    State(state): State<Arc<AppState>>,
) -> Result<Json<AppSettings>, AppError> {
    let row = sqlx::query("SELECT value FROM settings WHERE key = 'app_settings'")
        .fetch_optional(&state.pool)
        .await
        .map_err(anyhow::Error::from)?;

    let settings = match row {
        Some(r) => {
            let value: serde_json::Value = r.get("value");
            serde_json::from_value(value).unwrap_or(AppSettings {
                scrub_enabled: false,
            })
        }
        None => AppSettings {
            scrub_enabled: false,
        },
    };

    Ok(Json(settings))
}

async fn put_settings(
    State(state): State<Arc<AppState>>,
    Json(settings): Json<AppSettings>,
) -> Result<Json<AppSettings>, AppError> {
    let value = serde_json::to_value(&settings).map_err(|e| anyhow::anyhow!("{e}"))?;

    sqlx::query(
        "INSERT INTO settings (key, value) VALUES ('app_settings', $1) \
         ON CONFLICT (key) DO UPDATE SET value = $1",
    )
    .bind(value)
    .execute(&state.pool)
    .await
    .map_err(anyhow::Error::from)?;

    Ok(Json(settings))
}

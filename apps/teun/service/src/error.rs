use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(Debug)]
pub enum AppError {
    Internal(anyhow::Error),
    BadRequest(String),
    NotFound,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::Internal(e) => {
                tracing::error!("Internal error: {e:#}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
            }
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::NotFound => (StatusCode::NOT_FOUND, "Not found".to_string()),
        };

        let body = serde_json::json!({ "error": message });
        (status, axum::Json(body)).into_response()
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::Internal(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    fn status_of(err: AppError) -> StatusCode {
        let response = err.into_response();
        response.status()
    }

    #[test]
    fn bad_request_returns_400() {
        assert_eq!(
            status_of(AppError::BadRequest("oops".into())),
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn not_found_returns_404() {
        assert_eq!(status_of(AppError::NotFound), StatusCode::NOT_FOUND);
    }

    #[test]
    fn internal_returns_500() {
        assert_eq!(
            status_of(AppError::Internal(anyhow::anyhow!("boom"))),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn from_anyhow_converts_to_internal() {
        let err: AppError = anyhow::anyhow!("test").into();
        assert_eq!(status_of(err), StatusCode::INTERNAL_SERVER_ERROR);
    }
}

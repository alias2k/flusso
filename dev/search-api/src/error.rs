//! Mapping a handler error to an HTTP response.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// A handler error.
pub(crate) enum ApiError {
    /// A `flusso-search` client error. An upstream OpenSearch status is passed
    /// through; anything else (transport, decode) is a bad gateway.
    Upstream(flusso_search::Error),
    /// A `get`-by-id lookup found nothing → `404`.
    NotFound { resource: &'static str, id: String },
}

impl From<flusso_search::Error> for ApiError {
    fn from(error: flusso_search::Error) -> Self {
        ApiError::Upstream(error)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::Upstream(error) => {
                let status = match &error {
                    flusso_search::Error::Status { status, .. } => {
                        StatusCode::from_u16(*status).unwrap_or(StatusCode::BAD_GATEWAY)
                    }
                    _ => StatusCode::BAD_GATEWAY,
                };
                (status, error.to_string())
            }
            ApiError::NotFound { resource, id } => {
                (StatusCode::NOT_FOUND, format!("{resource}/{id} not found"))
            }
        };
        if status.is_server_error() {
            tracing::warn!(%status, %message, "request failed");
        } else {
            tracing::debug!(%status, %message, "request failed");
        }
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

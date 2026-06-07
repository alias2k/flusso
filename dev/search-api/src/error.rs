//! Mapping a client error to an HTTP response.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// Wraps a client error. An upstream OpenSearch status is passed through;
/// anything else (transport, decode) is a bad gateway.
pub(crate) struct ApiError(flusso_search::Error);

impl From<flusso_search::Error> for ApiError {
    fn from(error: flusso_search::Error) -> Self {
        ApiError(error)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self.0 {
            flusso_search::Error::Status { status, .. } => {
                StatusCode::from_u16(*status).unwrap_or(StatusCode::BAD_GATEWAY)
            }
            _ => StatusCode::BAD_GATEWAY,
        };
        tracing::warn!(error = %self.0, "request failed");
        (
            status,
            Json(serde_json::json!({ "error": self.0.to_string() })),
        )
            .into_response()
    }
}

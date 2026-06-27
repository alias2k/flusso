//! The axum server: routes the JSON API and serves the embedded SPA.
//!
//! [`serve`] binds a local address and runs until the process is signalled. The
//! API is rooted at `/api/*`; everything else falls through to the embedded
//! frontend (the `assets` module). State is just the path to the `flusso.toml`
//! being edited — the file is the source of truth, re-read per request, so the
//! server holds no model of its own.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use tokio::net::TcpListener;

use crate::api;
use crate::assets;

/// How to run the designer: which config to edit and where to listen.
#[derive(Debug, Clone)]
pub struct DesignOptions {
    /// Path to the `flusso.toml` the designer reads and writes.
    pub config_path: PathBuf,
    /// Local address to bind the UI + API to.
    pub address: SocketAddr,
    /// Open the designer URL in the default browser once the listener is bound.
    pub open_browser: bool,
}

#[derive(Clone)]
struct AppState {
    config_path: Arc<PathBuf>,
}

/// Bind `options.address` and serve the designer until the listener closes.
pub async fn serve(options: DesignOptions) -> Result<()> {
    let state = AppState {
        config_path: Arc::new(options.config_path),
    };
    let app = router(state);

    let listener = TcpListener::bind(options.address).await?;
    let local = listener.local_addr()?;
    let url = format!("http://{local}");
    tracing::info!(%url, "flusso designer ready — open {url} in your browser");

    // Best-effort: the socket is already bound (connections queue until `serve`
    // accepts), so the browser can open immediately. A failure to launch one
    // (headless box, no handler) is logged, never fatal.
    if options.open_browser
        && let Err(e) = open::that_detached(&url)
    {
        tracing::warn!(error = %e, %url, "could not open a browser; open the URL manually");
    }

    axum::serve(listener, app).await?;
    Ok(())
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/project", get(project))
        .route("/api/catalog", get(catalog))
        .route("/api/preview", post(preview))
        .route("/api/validate", post(validate))
        .route("/api/save", post(save))
        .with_state(state)
        .fallback(assets::serve)
}

async fn project(State(state): State<AppState>) -> Result<Response, ApiError> {
    let project = api::load_project(&state.config_path)?;
    Ok(Json(project).into_response())
}

async fn catalog(State(state): State<AppState>) -> Response {
    Json(api::introspect(&state.config_path).await).into_response()
}

async fn preview(Json(request): Json<api::PreviewRequest>) -> Result<Response, ApiError> {
    let response = api::build_preview(request)?;
    Ok(Json(response).into_response())
}

async fn validate(Json(request): Json<api::ValidateRequest>) -> Response {
    Json(api::validate(request).await).into_response()
}

async fn save(
    State(state): State<AppState>,
    Json(request): Json<api::SaveRequest>,
) -> Result<Response, ApiError> {
    let response = api::save_project(&state.config_path, request)?;
    Ok(Json(response).into_response())
}

/// An unexpected handler failure — reported as a 500 with a JSON `{ "error" }`
/// body. Recoverable, surfaced conditions (DB unreachable, a schema that won't
/// parse) are *not* errors: they ride back in the normal response body.
struct ApiError(anyhow::Error);

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        Self(error)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        tracing::error!(error = %format!("{:#}", self.0), "designer request failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("{:#}", self.0) })),
        )
            .into_response()
    }
}

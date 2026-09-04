//! The axum server: routes the JSON API and serves the embedded SPA.
//!
//! [`serve`] binds a local address and runs until the process is signalled. The
//! API is rooted at `/api/*`; everything else falls through to the embedded
//! frontend (the `assets` module). State is just the path to the `flusso.toml`
//! being edited — the file is the source of truth, re-read per request, so the
//! server holds no model of its own.

use std::fmt;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use config::Config;
use kernel::AdapterDescription;
use tokio::net::TcpListener;

use crate::api;
use crate::assets;

/// How to run the designer: which config to edit, where to listen, and what
/// the composition root knows about the adapters.
#[derive(Debug, Clone)]
pub struct DesignOptions {
    /// Path to the `flusso.toml` the designer reads and writes.
    pub config_path: PathBuf,
    /// Local address to bind the UI + API to.
    pub address: SocketAddr,
    /// Open the designer URL in the default browser once the listener is bound.
    pub open_browser: bool,
    /// What each registered adapter declares about its options. The designer
    /// renders its source/stream/sink forms from these and never names an
    /// adapter itself; the composition root supplies them.
    pub adapters: Vec<AdapterDescription>,
    /// The composition root's config validation: every port entry against its
    /// adapter. Run before any connection attempt.
    pub validate: ConfigValidator,
}

/// The composition root's validation function, boxed for the state.
type ValidateFn = dyn Fn(&Config) -> Result<()> + Send + Sync;

/// A config validator handed in by the composition root.
#[derive(Clone)]
pub struct ConfigValidator(Arc<ValidateFn>);

impl ConfigValidator {
    pub fn new(validate: impl Fn(&Config) -> Result<()> + Send + Sync + 'static) -> Self {
        Self(Arc::new(validate))
    }

    /// Validate `config` against the registered adapters.
    pub fn check(&self, config: &Config) -> Result<()> {
        (self.0)(config)
    }
}

impl fmt::Debug for ConfigValidator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ConfigValidator")
    }
}

#[derive(Clone)]
struct AppState {
    config_path: Arc<PathBuf>,
    adapters: Arc<Vec<AdapterDescription>>,
    validate: ConfigValidator,
}

/// Bind `options.address` and serve the designer until the listener closes.
pub async fn serve(options: DesignOptions) -> Result<()> {
    let state = AppState {
        config_path: Arc::new(options.config_path),
        adapters: Arc::new(options.adapters),
        validate: options.validate,
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

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

/// Resolve when the process is asked to stop (Ctrl-C, or SIGTERM on Unix), so
/// the server shuts down cleanly instead of being hard-killed.
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
    tracing::info!("flusso designer shutting down");
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/project", get(project))
        .route("/api/adapters", get(adapters))
        .route("/api/catalog", get(catalog))
        .route("/api/test-connection", post(test_connection))
        .route("/api/parse", post(parse))
        .route("/api/preview", post(preview))
        .route("/api/validate", post(validate))
        .route("/api/sample", post(sample))
        .route("/api/diff", post(diff))
        .route("/api/save", post(save))
        .route("/api/dirs", get(dirs))
        .with_state(state)
        .fallback(assets::serve)
}

async fn project(State(state): State<AppState>) -> Result<Response, ApiError> {
    let project = api::load_project(&state.config_path)?;
    Ok(Json(project).into_response())
}

async fn adapters(State(state): State<AppState>) -> Response {
    Json(state.adapters.as_ref()).into_response()
}

async fn catalog(State(state): State<AppState>) -> Response {
    Json(api::introspect(&state.config_path).await).into_response()
}

async fn dirs(State(state): State<AppState>) -> Response {
    Json(api::list_dirs(&state.config_path)).into_response()
}

async fn test_connection(
    State(state): State<AppState>,
    Json(config): Json<config::toml::ConfigToml>,
) -> Response {
    Json(api::test_connection(config, &state.validate).await).into_response()
}

async fn parse(Json(request): Json<api::ParseRequest>) -> Response {
    Json(api::parse_index(&request)).into_response()
}

async fn preview(Json(request): Json<api::PreviewRequest>) -> Result<Response, ApiError> {
    let response = api::build_preview(request)?;
    Ok(Json(response).into_response())
}

async fn validate(
    State(state): State<AppState>,
    Json(request): Json<api::ValidateRequest>,
) -> Response {
    Json(api::validate(request, &state.validate).await).into_response()
}

async fn sample(Json(request): Json<api::SampleRequest>) -> Response {
    Json(api::sample(request).await).into_response()
}

async fn diff(
    State(state): State<AppState>,
    Json(request): Json<api::SaveRequest>,
) -> Result<Response, ApiError> {
    let diffs = api::diff_project(&state.config_path, request)?;
    Ok(Json(diffs).into_response())
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

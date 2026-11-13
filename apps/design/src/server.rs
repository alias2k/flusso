//! The axum server: routes the JSON API and serves the embedded SPA.
//!
//! [`serve`] binds a local address and runs until the process is signalled. The
//! API is rooted at `/api/*`; everything else falls through to the embedded
//! frontend (the `assets` module). State is just the path to the `flusso.toml`
//! being edited — the file is the source of truth, re-read per request, so the
//! server holds no model of its own. The one thing it remembers is what it
//! recovered at start ([`api::recover_project`]), until a page load shows it.

use std::fmt;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use config::toml::ConfigToml;
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
type ValidateFn = dyn Fn(&ConfigToml) -> Result<()> + Send + Sync;

/// A config validator handed in by the composition root.
#[derive(Clone)]
pub struct ConfigValidator(Arc<ValidateFn>);

impl ConfigValidator {
    pub fn new(validate: impl Fn(&ConfigToml) -> Result<()> + Send + Sync + 'static) -> Self {
        Self(Arc::new(validate))
    }

    /// Validate an unsaved `flusso.toml` against the registered adapters: every
    /// port entry and every `[index.sinks.<name>]` table.
    pub fn check(&self, config: &ConfigToml) -> Result<()> {
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
    /// Interrupted saves settled since the last page load, to tell it about.
    recovered: Arc<Mutex<Vec<api::RecoveredSave>>>,
}

/// Bind `options.address` and serve the designer until the listener closes.
///
/// A save a killed designer left half done is settled first, before the
/// project is read; what it did is logged and shown on the first page load.
pub async fn serve(options: DesignOptions) -> Result<()> {
    let config_path = options.config_path.clone();
    let recovered = match blocking(move || {
        api::prune_save_records();
        api::recover_project(&config_path)
    })
    .await
    {
        Ok(recovery) => recovery.recovered,
        Err(e) => {
            tracing::warn!(error = %format!("{e:#}"), "could not check for an interrupted save");
            Vec::new()
        }
    };
    let state = AppState {
        config_path: Arc::new(options.config_path),
        adapters: Arc::new(options.adapters),
        validate: options.validate,
        recovered: Arc::new(Mutex::new(recovered)),
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
        .route("/api/expression", post(expression))
        .route("/api/value", post(value))
        .route("/api/preview", post(preview))
        .route("/api/validate", post(validate))
        .route("/api/sample", post(sample))
        .route("/api/diff", post(diff))
        .route("/api/save", post(save))
        .route("/api/dirs", get(dirs))
        .with_state(state)
        .fallback(assets::serve)
}

/// Run `work`, which may wait for the project's lock, off the async workers.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> anyhow::Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| anyhow::anyhow!("a designer request stopped: {e}"))?
}

/// Settle an interrupted save before the project is read; a blocked one
/// keeps the project from loading (a 409 naming the files).
async fn project(State(state): State<AppState>) -> Result<Response, ApiError> {
    let config_path = Arc::clone(&state.config_path);
    let (mut recovery, mut project) = blocking(move || {
        let recovery = api::recover_project(&config_path)?;
        if recovery.blocked.is_some() {
            return Err(anyhow::Error::new(api::Interrupted(recovery)));
        }
        Ok((recovery, api::load_project(&config_path)?))
    })
    .await?;
    if let Ok(mut earlier) = state.recovered.lock() {
        recovery.recovered.splice(0..0, earlier.drain(..));
    }
    project.recovery = recovery;
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

async fn expression(Json(request): Json<api::ExpressionRequest>) -> Response {
    Json(api::check_expression(&request)).into_response()
}

async fn value(Json(request): Json<api::ValueRequest>) -> Response {
    Json(api::read_value(&request)).into_response()
}

async fn preview(
    State(state): State<AppState>,
    Json(request): Json<api::PreviewRequest>,
) -> Result<Response, ApiError> {
    let response = api::build_preview(&state.config_path, request)?;
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
    let config_path = Arc::clone(&state.config_path);
    let review = blocking(move || api::review_project(&config_path, request)).await?;
    Ok(Json(review).into_response())
}

async fn save(
    State(state): State<AppState>,
    Json(request): Json<api::SaveRequest>,
) -> Result<Response, ApiError> {
    let config_path = Arc::clone(&state.config_path);
    let response = blocking(move || api::save_project(&config_path, request)).await?;
    Ok(Json(response).into_response())
}

/// An unexpected handler failure — reported as a 500 with a JSON `{ "error" }`
/// body. Recoverable, surfaced conditions (DB unreachable, a schema that won't
/// parse) are *not* errors: they ride back in the normal response body. An
/// interrupted save ([`api::Interrupted`]) is a 409 whose body adds the
/// `recovery`, and a save over files changed on disk ([`api::Stale`]) one
/// whose body adds the `stale` files, so the editor can say what happened in
/// the user's language.
struct ApiError(anyhow::Error);

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        Self(error)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if let Some(api::Interrupted(recovery)) = self.0.downcast_ref::<api::Interrupted>() {
            return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({ "error": self.0.to_string(), "recovery": recovery })),
            )
                .into_response();
        }
        if let Some(api::Stale(files)) = self.0.downcast_ref::<api::Stale>() {
            return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({ "error": self.0.to_string(), "stale": files })),
            )
                .into_response();
        }
        tracing::error!(error = %format!("{:#}", self.0), "designer request failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("{:#}", self.0) })),
        )
            .into_response()
    }
}

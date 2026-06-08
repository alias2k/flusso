//! The operational HTTP surface the `flusso` binary serves: liveness, readiness,
//! the live status document, and the Prometheus metrics scrape — one small axum
//! server reading the [`Status`] the daemon exposes plus the metrics registry the
//! binary installed. (The daemon owns the *domain*; this transport lives here.)
//!
//! | Route      | Purpose                                                  |
//! | ---------- | -------------------------------------------------------- |
//! | `/healthz` | Liveness — `200` whenever the process is up.             |
//! | `/readyz`  | Readiness — `200` once past startup, else `503`.         |
//! | `/status`  | The live [`StatusSnapshot`](daemon::StatusSnapshot) as JSON. |
//! | `/metrics` | Prometheus exposition text.                              |

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use daemon::{Phase, Status};
use prometheus::{Registry, TextEncoder};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

/// Shared state for the HTTP handlers. Cheap to clone (an `Arc` and a registry
/// handle, both internally reference-counted).
#[derive(Clone, Debug)]
pub(crate) struct AppState {
    pub status: Arc<Status>,
    /// `None` when the Prometheus reader wasn't installed — `/metrics` then
    /// reports that.
    pub registry: Option<Registry>,
}

/// Serve the surface over an already-bound `listener`, draining in-flight
/// requests once `shutdown` resolves (sender signalled or dropped). The listener
/// is bound by the caller so a bad address fails fast; a serve-loop error here
/// is logged, never fatal to the pipeline.
pub(crate) async fn serve(listener: TcpListener, state: AppState, shutdown: oneshot::Receiver<()>) {
    if let Ok(addr) = listener.local_addr() {
        tracing::info!(%addr, "HTTP surface listening on /healthz /readyz /status /metrics");
    }
    let graceful = async move {
        let _ = shutdown.await;
    };
    if let Err(error) = axum::serve(listener, router(state))
        .with_graceful_shutdown(graceful)
        .await
    {
        tracing::error!(%error, "HTTP server stopped on error");
    }
}

/// Build the router with its state wired in.
fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/status", get(status))
        .route("/metrics", get(metrics))
        .with_state(state)
}

/// Liveness: the process is running.
async fn healthz() -> impl IntoResponse {
    StatusCode::OK
}

/// Readiness: serving once the pipeline is past startup (backfilling or live).
/// A stopped pipeline is deliberately *not* ready.
async fn readyz(State(state): State<AppState>) -> impl IntoResponse {
    match state.status.snapshot().phase {
        Phase::Backfilling | Phase::Live => StatusCode::OK,
        Phase::Starting | Phase::Stopped => StatusCode::SERVICE_UNAVAILABLE,
    }
}

/// The full live status document.
async fn status(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.status.snapshot())
}

/// Prometheus exposition text, or a note when metrics are disabled.
async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    let Some(registry) = state.registry else {
        return (
            StatusCode::NOT_FOUND,
            "metrics are not enabled\n".to_owned(),
        );
    };
    match TextEncoder::new().encode_to_string(&registry.gather()) {
        Ok(text) => (StatusCode::OK, text),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to encode metrics: {error}\n"),
        ),
    }
}

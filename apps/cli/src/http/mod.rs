//! The operational HTTP surfaces the `flusso` binary serves — two listeners on
//! two ports, gating by *port* (a physical trust boundary) rather than path:
//!
//! | Surface     | Routes                                      | Auth                 |
//! | ----------- | ------------------------------------------- | -------------------- |
//! | **public**  | `/healthz` `/readyz` `/status` `/metrics`   | none (network-gated) |
//! | **private** | `/indexes` (and, later, `/reindex`)         | HTTP Basic           |
//!
//! The daemon owns the *domain* (the [`Status`] these read); the transport lives
//! here in the binary. A serve-loop error is logged, never fatal to the pipeline.

mod auth;

pub(crate) use auth::{BasicAuth, DEFAULT_ADMIN_PASSWORD, DEFAULT_ADMIN_USER};

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router, middleware};
use daemon::{Phase, Status};
use prometheus::{Registry, TextEncoder};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

/// Shared state for the public handlers. Cheap to clone (an `Arc` and a registry
/// handle, both internally reference-counted).
#[derive(Clone, Debug)]
pub(crate) struct PublicState {
    pub status: Arc<Status>,
    /// `None` when the Prometheus reader wasn't installed — `/metrics` then
    /// reports that.
    pub registry: Option<Registry>,
}

/// Serve `router` over an already-bound `listener`, draining in-flight requests
/// once `shutdown` resolves (sender signalled or dropped). The listener is bound
/// by the caller so a bad address fails fast; a serve-loop error here is logged,
/// never fatal to the pipeline. `surface` names it in logs (`public`/`private`).
pub(crate) async fn serve(
    surface: &'static str,
    listener: TcpListener,
    router: Router,
    shutdown: oneshot::Receiver<()>,
) {
    if let Ok(addr) = listener.local_addr() {
        tracing::info!(%addr, surface, "HTTP surface listening");
    }
    let graceful = async move {
        let _ = shutdown.await;
    };
    if let Err(error) = axum::serve(listener, router)
        .with_graceful_shutdown(graceful)
        .await
    {
        tracing::error!(%error, surface, "HTTP server stopped on error");
    }
}

/// The public, unauthenticated surface: liveness, readiness, the live status
/// document, and the Prometheus scrape.
pub(crate) fn public_router(state: PublicState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/status", get(status))
        .route("/metrics", get(metrics))
        .with_state(state)
}

/// The private control surface, behind HTTP Basic auth. `/reindex` is added with
/// the sink reindex machinery; for now it lists the indexes and their state.
pub(crate) fn private_router(status: Arc<Status>, basic_auth: Arc<BasicAuth>) -> Router {
    Router::new()
        .route("/indexes", get(indexes))
        .layer(middleware::from_fn_with_state(
            basic_auth,
            auth::require_basic_auth,
        ))
        .with_state(status)
}

/// Liveness: the process is running.
async fn healthz() -> impl IntoResponse {
    StatusCode::OK
}

/// Readiness: serving once the pipeline is past startup (backfilling or live).
/// A stopped pipeline is deliberately *not* ready.
async fn readyz(State(state): State<PublicState>) -> impl IntoResponse {
    match state.status.snapshot().phase {
        Phase::Backfilling | Phase::Live => StatusCode::OK,
        Phase::Starting | Phase::Stopped => StatusCode::SERVICE_UNAVAILABLE,
    }
}

/// The full live status document.
async fn status(State(state): State<PublicState>) -> impl IntoResponse {
    Json(state.status.snapshot())
}

/// Prometheus exposition text, or a note when metrics are disabled.
async fn metrics(State(state): State<PublicState>) -> impl IntoResponse {
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

/// The logical indexes and their current lifecycle state, as JSON
/// (`{"users": "seeded", …}`), read from the live [`Status`]. The sink's
/// physical generation is folded in when reindex lands.
async fn indexes(State(status): State<Arc<Status>>) -> impl IntoResponse {
    Json(status.snapshot().indexes)
}

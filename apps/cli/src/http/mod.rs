//! The operational HTTP surfaces the `flusso` binary serves — two listeners on
//! two ports, gating by *port* (a physical trust boundary) rather than path:
//!
//! | Surface     | Routes                                      | Auth                 |
//! | ----------- | ------------------------------------------- | -------------------- |
//! | **public**  | `/healthz` `/readyz` `/status` `/metrics`   | none (network-gated) |
//! | **private** | `/indexes` `/reindex`                       | HTTP Basic           |
//!
//! The daemon owns the *domain* (the [`Status`] these read); the transport lives
//! here in the binary. A serve-loop error is logged, never fatal to the pipeline.

mod auth;

pub(crate) use auth::{BasicAuth, DEFAULT_ADMIN_PASSWORD, DEFAULT_ADMIN_USER};

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router, middleware};
use daemon::{IndexName, Phase, Status};
use prometheus::{Registry, TextEncoder};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};

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

/// Shared state for the private control handlers.
#[derive(Clone, Debug)]
pub(crate) struct PrivateState {
    pub status: Arc<Status>,
    /// Reindex requests go here; the run loop drains them and restarts the
    /// pipeline to rebuild the named index into a fresh generation.
    pub reindex: mpsc::Sender<IndexName>,
}

/// The private control surface, behind HTTP Basic auth: list indexes and trigger
/// an on-demand reindex.
pub(crate) fn private_router(state: PrivateState, basic_auth: Arc<BasicAuth>) -> Router {
    Router::new()
        .route("/indexes", get(indexes))
        .route("/reindex", post(reindex))
        .layer(middleware::from_fn_with_state(
            basic_auth,
            auth::require_basic_auth,
        ))
        .with_state(state)
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
/// (`{"users": "seeded", …}`), read from the live [`Status`].
async fn indexes(State(state): State<PrivateState>) -> impl IntoResponse {
    Json(state.status.snapshot().indexes)
}

/// Stage an on-demand rebuild of one index (`POST /reindex?index=<name>`).
/// Validates the name and that the index exists, then queues a reindex request
/// for the run loop, which restarts the pipeline to rebuild it into a fresh
/// generation. Returns `202 Accepted` — the rebuild runs asynchronously; watch
/// `/status` for the index returning to `seeded`.
async fn reindex(
    State(state): State<PrivateState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let Some(raw) = params.get("index") else {
        return (
            StatusCode::BAD_REQUEST,
            "missing query parameter ?index=<name>\n",
        )
            .into_response();
    };
    let Ok(index) = IndexName::try_new(raw.clone()) else {
        return (
            StatusCode::BAD_REQUEST,
            format!("invalid index name {raw:?}\n"),
        )
            .into_response();
    };
    if !state.status.snapshot().indexes.contains_key(index.as_ref()) {
        return (
            StatusCode::NOT_FOUND,
            format!("unknown index {}\n", index.as_ref()),
        )
            .into_response();
    }
    match state.reindex.try_send(index.clone()) {
        Ok(()) => (
            StatusCode::ACCEPTED,
            format!("reindex of {} queued\n", index.as_ref()),
        )
            .into_response(),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "reindex queue is full or closed\n".to_owned(),
        )
            .into_response(),
    }
}

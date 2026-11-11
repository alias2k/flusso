//! The two operational HTTP surfaces `flusso run` serves.
//!
//! | Surface | Routes | Auth |
//! | --- | --- | --- |
//! | **public** | `/healthz` `/readyz` `/status` `/metrics` `/wait` | none |
//! | **private** | `/indexes` `/reindex` | HTTP Basic |
//!
//! Both read the daemon's [`Status`]; the public one also holds the daemon's
//! [`Fence`] (`/wait` is how a client waits to read its own write), the
//! private one the [`DaemonControl`] a reindex goes through. Transport is the
//! binary's concern: the daemon exposes data and operations, this module puts
//! them on the wire.

mod auth;

pub(crate) use auth::{BasicAuth, DEFAULT_ADMIN_PASSWORD, DEFAULT_ADMIN_USER};

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router, middleware};
use daemon::{
    ControlError, DaemonControl, Fence, FenceError, FenceLevel, FenceOutcome, FenceRequest,
    IndexName, SinkName, Status,
};
use prometheus::{Registry, TextEncoder};
use serde_json::json;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

/// How long `/wait` waits when the request names no `timeout_ms`.
const DEFAULT_WAIT: Duration = Duration::from_secs(5);

/// The longest `/wait` holds a request open; a longer `timeout_ms` is cut to
/// this, so a client cannot park a connection indefinitely.
const MAX_WAIT: Duration = Duration::from_secs(60);

/// What the public surface reads.
#[derive(Clone, Debug)]
pub(crate) struct PublicState {
    pub status: Arc<Status>,
    pub registry: Option<Registry>,
    /// The daemon's fence: what `/wait` waits on.
    pub fence: Fence,
}

/// Serve `router` on `listener` until `shutdown` fires.
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

pub(crate) fn public_router(state: PublicState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/status", get(status))
        .route("/metrics", get(metrics))
        .route("/wait", get(wait))
        .with_state(state)
}

/// What the private surface reads and drives.
#[derive(Clone, Debug)]
pub(crate) struct PrivateState {
    pub status: Arc<Status>,
    /// The daemon's operations handle: a reindex reaches the targeted sink
    /// engines through it.
    pub control: DaemonControl,
}

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

async fn healthz() -> impl IntoResponse {
    StatusCode::OK
}

/// Ready when every engine is: the ingest engine follows the source and every
/// sink engine follows its lane (live or backfilling).
async fn readyz(State(state): State<PublicState>) -> impl IntoResponse {
    if state.status.is_ready() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

async fn status(State(state): State<PublicState>) -> impl IntoResponse {
    Json(state.status.snapshot())
}

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

/// Wait until a commit is readable
/// (`GET /wait?position=<text>[&until=applied|searchable][&sink=<name>][&timeout_ms=<n>]`).
///
/// `position` is what the client read from the source after its commit
/// (Postgres: `SELECT pg_current_wal_lsn()`). The request is held until the
/// named sink, or every sink, has every commit at or before it `applied` (a
/// fetch by id sees it) or `searchable` (a search sees it; the default), then
/// answers `200`. `408` when `timeout_ms` (default 5000, at most 60000) passes
/// first; both carry the position reached. Nothing is refreshed or hurried on
/// behalf of the caller.
async fn wait(
    State(state): State<PublicState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let refuse = |status: StatusCode, error: String| {
        (status, Json(json!({ "error": error }))).into_response()
    };
    let Some(position) = params.get("position") else {
        return refuse(
            StatusCode::BAD_REQUEST,
            "missing query parameter ?position=<source position>".to_owned(),
        );
    };
    let level = match params.get("until").map(|raw| raw.parse::<FenceLevel>()) {
        None => FenceLevel::default(),
        Some(Ok(level)) => level,
        Some(Err(error)) => return refuse(StatusCode::BAD_REQUEST, format!("until: {error}")),
    };
    let sink = match params.get("sink") {
        None => None,
        Some(raw) => match SinkName::try_new(raw.clone()) {
            Ok(sink) => Some(sink),
            Err(_) => {
                return refuse(
                    StatusCode::BAD_REQUEST,
                    format!("invalid sink name {raw:?}"),
                );
            }
        },
    };
    let timeout = match params.get("timeout_ms").map(|raw| raw.parse::<u64>()) {
        None => DEFAULT_WAIT,
        Some(Ok(millis)) => Duration::from_millis(millis).min(MAX_WAIT),
        Some(Err(_)) => {
            return refuse(
                StatusCode::BAD_REQUEST,
                "timeout_ms must be a whole number of milliseconds".to_owned(),
            );
        }
    };
    let request = FenceRequest {
        position: position.clone(),
        level,
        sink,
        timeout,
    };
    let (status, reached, position) = match state.fence.wait(&request).await {
        Ok(FenceOutcome::Reached { position }) => (StatusCode::OK, true, position),
        Ok(FenceOutcome::TimedOut { position }) => (StatusCode::REQUEST_TIMEOUT, false, position),
        Err(error @ FenceError::InvalidPosition(_)) => {
            return refuse(StatusCode::BAD_REQUEST, error.to_string());
        }
        Err(error @ FenceError::UnknownSink(_)) => {
            return refuse(StatusCode::NOT_FOUND, error.to_string());
        }
        Err(error @ FenceError::Unavailable(_)) => {
            return refuse(StatusCode::SERVICE_UNAVAILABLE, error.to_string());
        }
    };
    (
        status,
        Json(json!({
            "reached": reached,
            "position": position,
            "until": level.as_str(),
            "sink": request.sink.as_ref().map(AsRef::<str>::as_ref),
        })),
    )
        .into_response()
}

/// Every sink's indexes and their states: `{ "<sink>": { "<index>": state } }`.
async fn indexes(State(state): State<PrivateState>) -> impl IntoResponse {
    let snapshot = state.status.snapshot();
    let per_sink: BTreeMap<String, _> = snapshot
        .sinks
        .into_iter()
        .map(|(name, sink)| (name, sink.indexes))
        .collect();
    Json(per_sink)
}

/// Rebuild one index (`POST /reindex?index=<name>[&sink=<name>]`) on one sink
/// or, without `sink`, on every sink. Validates the names, then hands the
/// operation to the daemon; the targeted sink engines stage a fresh generation
/// and request their snapshot, no restart involved.
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
    let sink = match params.get("sink") {
        None => None,
        Some(raw) => match SinkName::try_new(raw.clone()) {
            Ok(sink) => Some(sink),
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    format!("invalid sink name {raw:?}\n"),
                )
                    .into_response();
            }
        },
    };
    match state.control.reindex(index.clone(), sink.as_ref()) {
        Ok(()) => (
            StatusCode::ACCEPTED,
            match &sink {
                Some(sink) => format!("reindex of {} on {sink} queued\n", index.as_ref()),
                None => format!("reindex of {} on every sink queued\n", index.as_ref()),
            },
        )
            .into_response(),
        Err(ControlError::UnknownSink(name)) => {
            (StatusCode::NOT_FOUND, format!("unknown sink {name}\n")).into_response()
        }
        Err(error @ (ControlError::Busy(_) | ControlError::Closed(_))) => {
            (StatusCode::SERVICE_UNAVAILABLE, format!("{error}\n")).into_response()
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;

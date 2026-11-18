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
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
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
use tokio::sync::{oneshot, watch};

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
    /// The open `/wait`s, and whether the stop's drain is over.
    pub waits: Waits,
}

/// How long a refused `/wait` should wait before asking again, in seconds.
const RETRY_AFTER_SECS: &str = "1";

/// The `/wait` requests in flight, and the moment a stop's drain is over.
/// During the drain a wait goes on as usual (in-flight commits land then);
/// once it is over every open or new wait is answered `503` "stopping".
#[derive(Debug, Clone)]
pub(crate) struct Waits {
    over: Arc<watch::Sender<bool>>,
    open: Arc<AtomicUsize>,
}

impl Default for Waits {
    fn default() -> Self {
        Self {
            over: Arc::new(watch::Sender::new(false)),
            open: Arc::default(),
        }
    }
}

impl Waits {
    /// The drain is over: answer every open wait and refuse new ones.
    pub(crate) fn end(&self) {
        self.over.send_replace(true);
    }

    /// [`end`](Self::end), then give the open waits up to a second to be
    /// answered before the process exits under them.
    pub(crate) async fn end_before_exit(&self) {
        self.end();
        let _ = tokio::time::timeout(Duration::from_secs(1), async {
            while self.open.load(Ordering::SeqCst) > 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
    }

    fn is_over(&self) -> bool {
        *self.over.borrow()
    }

    async fn over(&self) {
        let mut seen = self.over.subscribe();
        let _ = seen.wait_for(|over| *over).await;
    }

    fn enter(&self) -> OpenWait {
        self.open.fetch_add(1, Ordering::SeqCst);
        OpenWait(Arc::clone(&self.open))
    }
}

/// One open wait, counted until dropped.
struct OpenWait(Arc<AtomicUsize>);

impl Drop for OpenWait {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// `503` with `Retry-After` and why: `stopping` (this process is going away;
/// the next one answers the same position) or `starting` (the source isn't
/// live yet).
fn unavailable(reason: &str, error: String) -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        [(header::RETRY_AFTER, RETRY_AFTER_SECS)],
        Json(json!({ "error": error, "reason": reason })),
    )
        .into_response()
}

/// Bind `surface`'s listener on `address`. An address already taken gets an
/// error naming the flusso there when `probe` (that flusso's public address,
/// usually `address` itself) answers `/status` with its pid.
pub(crate) async fn bind(
    surface: &'static str,
    address: SocketAddr,
    probe: Option<SocketAddr>,
) -> anyhow::Result<TcpListener> {
    let error = match TcpListener::bind(address).await {
        Ok(listener) => return Ok(listener),
        Err(error) => error,
    };
    let context = format!("binding the {surface} HTTP surface to {address}");
    if error.kind() != std::io::ErrorKind::AddrInUse {
        return Err(anyhow::Error::new(error).context(context));
    }
    let holder = match probe {
        Some(probe) => running_pid(probe).await,
        None => None,
    };
    let flag = format!("--{surface}-address");
    let explained = match holder {
        Some(pid) => format!(
            "{context}: another flusso is already serving there; it reports pid {pid}{}. Stop it \
             before starting this one, or give this run its own {flag} and --slot",
            if is_local_flusso(pid) {
                ", a flusso process on this host"
            } else {
                " (from a container that is the pid inside it, not one on this host)"
            }
        ),
        None => format!(
            "{context}: possibly a flusso still running. Stop whatever holds the address, or \
             pass {flag}"
        ),
    };
    Err(anyhow::Error::new(error).context(explained))
}

/// Whether a process `pid` runs here and is a flusso: its name, read from
/// `/proc` or, without one (macOS), from `ps`. Anything unreadable is "no".
fn is_local_flusso(pid: u64) -> bool {
    let name = std::fs::read_to_string(format!("/proc/{pid}/comm")).or_else(|_| {
        std::process::Command::new("ps")
            .args(["-o", "comm=", "-p", &pid.to_string()])
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
    });
    name.is_ok_and(|name| {
        name.trim()
            .rsplit('/')
            .next()
            .is_some_and(|name| name.starts_with("flusso"))
    })
}

/// The pid a flusso serving its public surface on `address` reports in
/// `/status`, if one answers within a second.
async fn running_pid(address: SocketAddr) -> Option<u64> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let exchange = async {
        let mut stream = tokio::net::TcpStream::connect(address).await.ok()?;
        stream
            .write_all(b"GET /status HTTP/1.0\r\nHost: flusso\r\n\r\n")
            .await
            .ok()?;
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.ok()?;
        let text = String::from_utf8(response).ok()?;
        let (_, body) = text.split_once("\r\n\r\n")?;
        serde_json::from_str::<serde_json::Value>(body)
            .ok()?
            .get("pid")?
            .as_u64()
    };
    tokio::time::timeout(Duration::from_secs(1), exchange)
        .await
        .ok()
        .flatten()
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

async fn status(State(state): State<PublicState>) -> Response {
    let mut body = match serde_json::to_value(state.status.snapshot()) {
        Ok(body) => body,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to encode the status: {error}\n"),
            )
                .into_response();
        }
    };
    if let Some(fields) = body.as_object_mut() {
        fields.insert("pid".to_owned(), json!(std::process::id()));
    }
    Json(body).into_response()
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
/// (`GET /wait?position=<text>[&until=applied|searchable][&sink=<name>][&index=<name>]…[&timeout_ms=<n>]`).
///
/// `position` is what the client read from the source after its commit
/// (Postgres: `SELECT pg_current_wal_lsn()`). The request is held until the
/// named sink, or every sink, has, for the named indexes (`index`, repeatable)
/// or every index, every commit at or before it `applied` (a fetch by id sees
/// it) or `searchable` (a search sees it; the default), then answers `200`. `408` when `timeout_ms` (default 5000, at most 60000) passes
/// first; both carry the position reached. Nothing is refreshed or hurried on
/// behalf of the caller.
async fn wait(
    State(state): State<PublicState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Response {
    let refuse = |status: StatusCode, error: String| {
        (status, Json(json!({ "error": error }))).into_response()
    };
    let mut indexes = Vec::new();
    for (key, raw) in &pairs {
        if key == "index" {
            match IndexName::try_new(raw.clone()) {
                Ok(index) => indexes.push(index),
                Err(_) => {
                    return refuse(
                        StatusCode::BAD_REQUEST,
                        format!("invalid index name {raw:?}"),
                    );
                }
            }
        }
    }
    let params: HashMap<String, String> = pairs.into_iter().collect();
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
        indexes,
        timeout,
    };
    let stopping = || {
        unavailable(
            "stopping",
            "flusso is stopping; ask again: the next process answers the same position".to_owned(),
        )
    };
    if state.waits.is_over() {
        return stopping();
    }
    let _open = state.waits.enter();
    let waited = tokio::select! {
        waited = state.fence.wait(&request) => waited,
        () = state.waits.over() => return stopping(),
    };
    let (status, reached, position) = match waited {
        Ok(FenceOutcome::Reached { position }) => (StatusCode::OK, true, position),
        Ok(FenceOutcome::TimedOut { position }) => (StatusCode::REQUEST_TIMEOUT, false, position),
        Err(error @ FenceError::InvalidPosition(_)) => {
            return refuse(StatusCode::BAD_REQUEST, error.to_string());
        }
        Err(error @ (FenceError::UnknownSink(_) | FenceError::UnknownIndex(_))) => {
            return refuse(StatusCode::NOT_FOUND, error.to_string());
        }
        Err(error @ FenceError::Unavailable(_)) => {
            return unavailable("starting", error.to_string());
        }
    };
    (
        status,
        Json(json!({
            "reached": reached,
            "position": position,
            "until": level.as_str(),
            "sink": request.sink.as_ref().map(AsRef::<str>::as_ref),
            "indexes": request.indexes.iter().map(AsRef::<str>::as_ref).collect::<Vec<_>>(),
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
        Err(error) => (StatusCode::SERVICE_UNAVAILABLE, format!("{error}\n")).into_response(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;

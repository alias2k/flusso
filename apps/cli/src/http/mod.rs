//! The two operational HTTP surfaces `flusso run` serves.
//!
//! | Surface | Routes | Auth |
//! | --- | --- | --- |
//! | **public** | `/healthz` `/readyz` `/status` `/metrics` `/wait` `/indexes/{name}/status` | none |
//! | **private** | `/indexes` `/indexes/{name}/reindex` | HTTP Basic |
//!
//! `/indexes/…` is served by both: `…/status` only by the public one,
//! `GET /indexes` and `…/reindex` only by the private one; each answers `404`
//! for the other's routes.
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

use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router, middleware};
use daemon::{
    ControlError, DaemonControl, Fence, FenceError, FenceLevel, FenceOutcome, FenceRequest,
    IndexName, SinkName, Status, Unserved,
};
use prometheus::{Registry, TextEncoder};
use serde_json::json;
use stream::LaneKey;
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

/// `503` with `Retry-After` for a wait some lane can't vouch for (#379): its
/// index is being seeded (`backfilling`) or rebuilt beside the generation
/// reads see (`rebuilding`), named in `lanes`. The most telling reason wins.
fn unserved(position: &str, lanes: &[(LaneKey, Unserved)]) -> Response {
    let reason = lanes
        .iter()
        .map(|(_, why)| *why)
        .max()
        .unwrap_or(Unserved::Starting);
    let names: Vec<String> = lanes
        .iter()
        .map(|(lane, why)| format!("{} on {} ({})", lane.index, lane.sink, why.as_str()))
        .collect();
    (
        StatusCode::SERVICE_UNAVAILABLE,
        [(header::RETRY_AFTER, RETRY_AFTER_SECS)],
        Json(json!({
            "error": format!(
                "reads don't see this position yet: {}",
                names.join(", ")
            ),
            "reason": reason.as_str(),
            "position": position,
            "lanes": lanes
                .iter()
                .map(|(lane, why)| json!({
                    "index": lane.index.as_ref(),
                    "sink": lane.sink.as_ref(),
                    "state": why.as_str(),
                }))
                .collect::<Vec<_>>(),
        })),
    )
        .into_response()
}

/// `409` for a wait inside a gap a reattach accepted (ADR 0012 stage 10,
/// S3): the lanes' indexes never got those changes, so no wait helps. Each
/// lane names its gap, in the source's own text.
fn in_gap(position: &str, lanes: &[(LaneKey, daemon::FenceGap)]) -> Response {
    let names: Vec<String> = lanes
        .iter()
        .map(|(lane, gap)| {
            format!(
                "{} on {} (from {} to {})",
                lane.index,
                lane.sink,
                gap.from.as_deref().unwrap_or("an unknown position"),
                gap.to.as_deref().unwrap_or("an unknown position"),
            )
        })
        .collect();
    (
        StatusCode::CONFLICT,
        Json(json!({
            "error": format!(
                "the position falls inside changes a reattach accepted as missing: {}; \
                 reindex the index on a sink that can rebuild it",
                names.join(", ")
            ),
            "reason": "gap",
            "position": position,
            "lanes": lanes
                .iter()
                .map(|(lane, gap)| json!({
                    "index": lane.index.as_ref(),
                    "sink": lane.sink.as_ref(),
                    "state": "gap",
                    "gap": gap,
                }))
                .collect::<Vec<_>>(),
        })),
    )
        .into_response()
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
        .route("/indexes/{name}/status", get(index_status))
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
        .route("/indexes/{name}/reindex", post(reindex))
        .route("/indexes/{name}/reattach", post(reattach))
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
        Ok(FenceOutcome::Unserved { position, lanes }) => {
            return unserved(&position, &lanes);
        }
        Ok(FenceOutcome::InGap { position, lanes }) => return in_gap(&position, &lanes),
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

/// `name` as an index the deployment tracks: `400` when it is no index
/// name, `404` (naming the known ones) when it is not one of them.
fn known_index(status: &Status, name: &str) -> Result<IndexName, Box<Response>> {
    let Ok(index) = IndexName::try_new(name.to_owned()) else {
        return Err(Box::new(
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("invalid index name {name:?}") })),
            )
                .into_response(),
        ));
    };
    if !status.tracks(&index) {
        return Err(Box::new(
            (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": format!("unknown index {}", index.as_ref()),
                    "indexes": status.index_names(),
                })),
            )
                .into_response(),
        ));
    }
    Ok(index)
}

/// One index's ingest state (`GET /indexes/<name>/status`): the same JSON as
/// `/status`'s `ingest.indexes.<name>` (ADR 0012 stage 8a, slice 7).
async fn index_status(State(state): State<PublicState>, Path(name): Path<String>) -> Response {
    let index = match known_index(&state.status, &name) {
        Ok(index) => index,
        Err(refused) => return *refused,
    };
    match state.status.index_ingest(&index) {
        Some(entry) => Json(entry).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("unknown index {}", index.as_ref()) })),
        )
            .into_response(),
    }
}

/// Rebuild one index (`POST /indexes/<name>/reindex[?sink=<name>]`) on one
/// sink or, without `sink`, on every sink. Validates the names, then hands
/// the operation to the daemon; the targeted sink engines stage a fresh
/// generation and request their snapshot, no restart involved. `202`
/// `{"index", "sink" (null: every sink), "queued": true}`, with `skipped`
/// naming each sink whose rebuild of the index is starting or running (ADR
/// 0012 stage 10, O20) when no sink is named; `409` when the named sink's (or
/// every sink's) is; `400` an invalid index or sink name; `404` an unknown
/// index or sink; `503` while stopping or with the engine not running.
async fn reindex(
    State(state): State<PrivateState>,
    Path(name): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let index = match known_index(&state.status, &name) {
        Ok(index) => index,
        Err(refused) => return *refused,
    };
    let sink = match params.get("sink") {
        None => None,
        Some(raw) => match SinkName::try_new(raw.clone()) {
            Ok(sink) => Some(sink),
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error": format!("invalid sink name {raw:?}") })),
                )
                    .into_response();
            }
        },
    };
    // O20: a rebuild starting, or running with its lane open, takes no
    // reindex. Named, the sink refuses with `409`; unnamed, the reindex goes
    // on where it applies and lists the lanes it skipped.
    let targets: Vec<SinkName> = match &sink {
        Some(sink) => vec![sink.clone()],
        None => state.control.sinks().cloned().collect(),
    };
    if state.control.is_stopping() {
        return stopping_refusal();
    }
    let detached = state.status.detached_lanes();
    let skipped: Vec<(SinkName, String)> = targets
        .iter()
        .filter_map(|target| {
            let lane = LaneKey::new(index.clone(), target.clone());
            // A detached index on a sink that can't rebuild is reattached
            // (O16): a reindex there is ignored, so name what works.
            if state.control.is_live_only(target) && detached.contains(&lane) {
                return Some((
                    target.clone(),
                    format!(
                        "this sink can't rebuild the index (backfill = false): \
                         flusso reattach {index} --sink {target} --accept-gap"
                    ),
                ));
            }
            state
                .status
                .reindex_refusal(&lane)
                .map(|reason| (target.clone(), reason))
        })
        .collect();
    let refuse = |reason: &str| {
        (
            StatusCode::CONFLICT,
            Json(json!({
                "error": reason,
                "reason": reason,
                "index": index.as_ref(),
                "skipped": skipped
                    .iter()
                    .map(|(sink, reason)| json!({ "sink": sink.as_ref(), "reason": reason }))
                    .collect::<Vec<_>>(),
            })),
        )
            .into_response()
    };
    if let Some((_, reason)) = skipped.first()
        && (sink.is_some() || skipped.len() == targets.len())
    {
        return refuse(reason);
    }
    let queued = if skipped.is_empty() {
        state.control.reindex(index.clone(), sink.as_ref())
    } else {
        targets
            .iter()
            .filter(|target| !skipped.iter().any(|(sink, _)| sink == *target))
            .try_for_each(|target| state.control.reindex(index.clone(), Some(target)))
    };
    match queued {
        Ok(()) => {
            let mut body = json!({
                "index": index.as_ref(),
                "sink": sink.as_ref().map(|sink| sink.as_ref()),
                "queued": true,
            });
            if !skipped.is_empty()
                && let Some(fields) = body.as_object_mut()
            {
                fields.insert(
                    "skipped".to_owned(),
                    skipped
                        .iter()
                        .map(|(sink, reason)| json!({ "sink": sink.as_ref(), "reason": reason }))
                        .collect(),
                );
            }
            (StatusCode::ACCEPTED, Json(body)).into_response()
        }
        Err(ControlError::UnknownSink(name)) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("unknown sink {name}") })),
        )
            .into_response(),
        Err(error) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": error.to_string() })),
        )
            .into_response(),
    }
}

/// `POST /indexes/{name}/reattach?sink=<name>&accept_gap=true` (ADR 0012
/// stage 10, O16): reattach a detached lane on a sink that can't rebuild,
/// accepting the changes it missed. `400` without `sink` or without
/// `accept_gap=true`; `404` for an unknown index or sink; `409` when the
/// lane is not detached, is being attached, or its sink can rebuild (a
/// reindex does); `503` while stopping or busy; else `202`.
async fn reattach(
    State(state): State<PrivateState>,
    Path(name): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let index = match known_index(&state.status, &name) {
        Ok(index) => index,
        Err(refused) => return *refused,
    };
    let refuse = |status: StatusCode, error: String| {
        (
            status,
            Json(json!({ "error": error, "index": index.as_ref() })),
        )
            .into_response()
    };
    let Some(raw) = params.get("sink") else {
        return refuse(
            StatusCode::BAD_REQUEST,
            "name the sink: a reattach is for one lane (?sink=<name>)".to_owned(),
        );
    };
    let Ok(sink) = SinkName::try_new(raw.clone()) else {
        return refuse(
            StatusCode::BAD_REQUEST,
            format!("invalid sink name {raw:?}"),
        );
    };
    if params.get("accept_gap").map(String::as_str) != Some("true") {
        return refuse(
            StatusCode::BAD_REQUEST,
            "a reattach drops the changes made while the lane was detached: \
             say so with accept_gap=true"
                .to_owned(),
        );
    }
    if !state.control.sinks().any(|known| *known == sink) {
        return refuse(StatusCode::NOT_FOUND, format!("unknown sink {sink}"));
    }
    if state.control.is_stopping() {
        return stopping_refusal();
    }
    let conflict = |reason: &str| {
        (
            StatusCode::CONFLICT,
            Json(json!({
                "error": reason,
                "reason": reason,
                "index": index.as_ref(),
                "sink": sink.as_ref(),
            })),
        )
            .into_response()
    };
    if !state.control.is_live_only(&sink) {
        return conflict(&format!(
            "this sink can rebuild the index (backfill = true), with nothing missing: \
             flusso reindex {index} --sink {sink}"
        ));
    }
    if let Some(reason) = state
        .status
        .reattach_refusal(&LaneKey::new(index.clone(), sink.clone()))
    {
        return conflict(reason);
    }
    let verdict = match state.control.reattach(index.clone(), &sink) {
        Ok(verdict) => verdict,
        Err(ControlError::UnknownSink(name)) => {
            return refuse(StatusCode::NOT_FOUND, format!("unknown sink {name}"));
        }
        Err(ControlError::Stopping) => return stopping_refusal(),
        Err(error @ ControlError::CanRebuild(_)) => return conflict(&error.to_string()),
        Err(error) => return refuse(StatusCode::SERVICE_UNAVAILABLE, error.to_string()),
    };
    // The engine answers between two batches: the operator hears its
    // verdict, not a blind "queued" (review of step 4, S4).
    match tokio::time::timeout(REATTACH_VERDICT, verdict).await {
        Ok(Ok(daemon::ReattachVerdict::Started)) => (
            StatusCode::ACCEPTED,
            Json(json!({
                "index": index.as_ref(),
                "sink": sink.as_ref(),
                "started": true,
            })),
        )
            .into_response(),
        Ok(Ok(daemon::ReattachVerdict::Refused(reason))) => conflict(&reason),
        Ok(Ok(daemon::ReattachVerdict::Failed(error))) => refuse(
            StatusCode::SERVICE_UNAVAILABLE,
            format!(
                "the reattach failed: {error}. Nothing accepted is lost. If it failed before \
                 the record's delete, nothing changed; /status tells: the lane still detached, \
                 ask again; attaching or open, the reattach took effect"
            ),
        ),
        Ok(Err(_)) => refuse(
            StatusCode::SERVICE_UNAVAILABLE,
            "the sink engine's run ended before the reattach started (a stop or a failure); \
             nothing changed: ask again"
                .to_owned(),
        ),
        Err(_) => refuse(
            StatusCode::SERVICE_UNAVAILABLE,
            format!(
                "no verdict within {}s: the sink engine is busy; the reattach may still \
                 start, and /status shows the lane when it does",
                REATTACH_VERDICT.as_secs()
            ),
        ),
    }
}

/// How long a reattach waits for its sink engine's verdict.
const REATTACH_VERDICT: std::time::Duration = std::time::Duration::from_secs(30);

/// `503` `stopping`: the deployment takes no operation while it stops.
fn stopping_refusal() -> Response {
    unavailable(
        "stopping",
        "flusso is stopping and takes no operation; ask the next process".to_owned(),
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;

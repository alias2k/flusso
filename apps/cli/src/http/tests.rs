//! The `/wait` endpoint over a real listener: every status it answers with.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use async_trait::async_trait;
use futures::stream::BoxStream;
use source::SourceError;
use source::cdc::{ChangeCapture, Continuity, LiveChange, SourcePosition};

use super::*;

/// A source that has read through `at` with nothing outstanding, so every sink
/// stands there for both levels.
#[derive(Default)]
struct SteadySource {
    at: AtomicU64,
    closed: AtomicBool,
    listeners: std::sync::Mutex<Vec<source::cdc::AdvanceListener>>,
}

impl std::fmt::Debug for SteadySource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SteadySource").finish_non_exhaustive()
    }
}

impl SteadySource {
    /// Read on through `at`, waking whoever waits on the fence.
    fn advance(&self, at: u64) {
        self.at.store(at, Ordering::SeqCst);
        for listener in self.listeners.lock().unwrap().iter() {
            listener();
        }
    }
}

#[async_trait]
impl ChangeCapture for SteadySource {
    async fn continuity(&self) -> source::Result<Continuity> {
        Ok(Continuity::Resumed)
    }

    async fn prepare(&self) -> source::Result<()> {
        Ok(())
    }

    async fn live(&self) -> source::Result<BoxStream<'static, source::Result<LiveChange>>> {
        Ok(Box::pin(futures::stream::empty()))
    }

    fn confirm(&self, _: source::cdc::Landed) {}

    fn parse_position(&self, text: &str) -> source::Result<SourcePosition> {
        text.parse()
            .map(SourcePosition)
            .map_err(|_| SourceError::Decode(format!("`{text}` is not a number")))
    }

    fn reached(&self, _: source::cdc::Landed) -> Option<SourcePosition> {
        (!self.closed.load(Ordering::SeqCst))
            .then(|| SourcePosition(self.at.load(Ordering::SeqCst)))
    }

    fn on_advance(&self, listener: source::cdc::AdvanceListener) {
        self.listeners.lock().unwrap().push(listener);
    }
}

/// Serve the public router over a source standing at `at`; returns its base
/// URL and the source.
async fn serve_public(at: u64) -> (String, Arc<SteadySource>) {
    let (base, source, _) = serve_stoppable(at).await;
    (base, source)
}

/// [`serve_public`], plus the handle that ends a stop's drain.
async fn serve_stoppable(at: u64) -> (String, Arc<SteadySource>, Waits) {
    serve_staged(at, |_| {}).await
}

/// [`serve_stoppable`] with the `primary` sink staged, after `events` told
/// the fence what else its sink engine did.
async fn serve_staged(
    at: u64,
    events: impl FnOnce(&daemon::FenceObserver),
) -> (String, Arc<SteadySource>, Waits) {
    let waits = Waits::default();
    let source = Arc::new(SteadySource::default());
    source.at.store(at, Ordering::SeqCst);
    let primary = SinkName::try_new("primary").unwrap();
    let lane = stream::LaneKey::new(
        kernel::IndexName::try_new("users").unwrap(),
        primary.clone(),
    );
    let fence = Fence::new(
        Arc::clone(&source) as Arc<dyn ChangeCapture>,
        Arc::new(daemon::Progress::new([lane.clone()])),
        [lane],
    );
    let observer = daemon::FenceObserver::new(fence.clone());
    events(&observer);
    daemon::Observer::on_sink_started(&observer, &primary);
    let state = PublicState {
        status: Arc::new(Status::new([], [primary.clone()], Instant::now())),
        registry: None,
        fence,
        waits: waits.clone(),
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, public_router(state)).await });
    (base, source, waits)
}

async fn get_wait(base: &str, query: &str) -> (u16, serde_json::Value) {
    let response = reqwest::get(format!("{base}/wait?{query}")).await.unwrap();
    let status = response.status().as_u16();
    (status, response.json().await.unwrap())
}

#[tokio::test]
async fn a_reached_position_answers_200_with_where_the_sink_is() {
    let (base, _source) = serve_public(40).await;
    let (status, body) = get_wait(&base, "position=35").await;
    assert_eq!(status, 200);
    assert_eq!(
        body,
        json!({ "reached": true, "position": "40", "until": "searchable", "sink": null, "indexes": [] })
    );

    let (status, body) = get_wait(&base, "position=40&until=applied&sink=primary").await;
    assert_eq!(status, 200);
    assert_eq!(
        body,
        json!({ "reached": true, "position": "40", "until": "applied", "sink": "primary", "indexes": [] })
    );
}

#[tokio::test]
async fn an_unreached_position_answers_408_after_the_timeout() {
    let (base, _source) = serve_public(40).await;
    let started = Instant::now();
    let (status, body) = get_wait(&base, "position=41&timeout_ms=150").await;
    assert_eq!(status, 408);
    assert!(started.elapsed() >= Duration::from_millis(150));
    assert_eq!(
        body,
        json!({ "reached": false, "position": "40", "until": "searchable", "sink": null, "indexes": [] })
    );
}

#[tokio::test]
async fn a_bad_request_answers_400() {
    let (base, _source) = serve_public(40).await;
    for query in [
        "until=applied",
        "position=soon",
        "position=1&until=visible",
        "position=1&timeout_ms=later",
        "position=1&sink=",
    ] {
        let (status, body) = get_wait(&base, query).await;
        assert_eq!(status, 400, "{query}: {body}");
        assert!(body["error"].is_string(), "{query}: {body}");
    }
}

#[tokio::test]
async fn an_unknown_sink_answers_404() {
    let (base, _source) = serve_public(40).await;
    let (status, body) = get_wait(&base, "position=1&sink=audit").await;
    assert_eq!(status, 404);
    assert_eq!(body["error"], "unknown sink `audit`");
}

#[tokio::test]
async fn a_wait_on_named_indexes_echoes_them() {
    let (base, _source) = serve_public(40).await;
    let (status, body) = get_wait(&base, "position=35&index=users&until=applied").await;
    assert_eq!(status, 200);
    assert_eq!(
        body,
        json!({ "reached": true, "position": "40", "until": "applied", "sink": null, "indexes": ["users"] })
    );
}

#[tokio::test]
async fn an_unknown_or_invalid_index_is_refused() {
    let (base, _source) = serve_public(40).await;
    let (status, body) = get_wait(&base, "position=1&index=users&index=orders").await;
    assert_eq!(status, 404);
    assert_eq!(body["error"], "unknown index `orders`");
    let (status, body) = get_wait(&base, "position=1&index=").await;
    assert_eq!(status, 400, "{body}");
}

#[tokio::test]
async fn a_source_that_is_not_live_yet_answers_503() {
    let (base, source) = serve_public(40).await;
    source.closed.store(true, Ordering::SeqCst);
    let (status, body) = get_wait(&base, "position=1").await;
    assert_eq!(status, 503);
    assert!(body["error"].is_string());
}

/// A `/wait` answered with its status, `Retry-After` and body.
async fn get_wait_full(base: &str, query: &str) -> (u16, Option<String>, serde_json::Value) {
    let response = reqwest::get(format!("{base}/wait?{query}")).await.unwrap();
    let status = response.status().as_u16();
    let retry = response
        .headers()
        .get("retry-after")
        .map(|value| value.to_str().unwrap().to_owned());
    (status, retry, response.json().await.unwrap())
}

#[tokio::test]
async fn a_wait_open_when_the_stop_begins_is_answered_200_if_reached_during_the_drain() {
    let (base, source, _waits) = serve_stoppable(10).await;
    let waiting =
        tokio::spawn(async move { get_wait(&base, "position=20&timeout_ms=60000").await });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    source.advance(20);
    let (status, body) = tokio::time::timeout(std::time::Duration::from_secs(2), waiting)
        .await
        .expect("the wait is answered once reached")
        .unwrap();

    assert_eq!(status, 200, "{body}");
}

#[tokio::test]
async fn an_open_wait_is_answered_503_stopping_with_retry_after_once_the_drain_ends() {
    let (base, _source, waits) = serve_stoppable(10).await;
    let waiting =
        tokio::spawn(async move { get_wait_full(&base, "position=999&timeout_ms=60000").await });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    waits.end();
    let (status, retry, body) = tokio::time::timeout(std::time::Duration::from_secs(2), waiting)
        .await
        .expect("the wait is answered at once")
        .unwrap();

    assert_eq!(status, 503);
    assert_eq!(retry.as_deref(), Some("1"));
    assert_eq!(body["reason"], "stopping", "{body}");
}

#[tokio::test]
async fn a_wait_after_the_drain_is_refused_and_one_before_the_source_is_live_says_starting() {
    let (base, source, waits) = serve_stoppable(10).await;
    source.closed.store(true, Ordering::SeqCst);
    let (status, retry, body) = get_wait_full(&base, "position=1").await;
    assert_eq!((status, retry.as_deref()), (503, Some("1")));
    assert_eq!(body["reason"], "starting", "{body}");

    source.closed.store(false, Ordering::SeqCst);
    waits.end();
    let (status, retry, body) = get_wait_full(&base, "position=1").await;
    assert_eq!((status, retry.as_deref()), (503, Some("1")));
    assert_eq!(
        body["reason"], "stopping",
        "even a reached position: this process no longer answers: {body}"
    );
}

/// A wait on an index its sink is seeding or rebuilding is `503` with the
/// reason and the lanes, never `200` (#379).
#[tokio::test]
async fn a_wait_on_an_index_being_seeded_or_rebuilt_is_answered_503_with_the_reason() {
    let primary = SinkName::try_new("primary").unwrap();
    let users = kernel::IndexName::try_new("users").unwrap();
    for (events, reason) in [
        (
            Box::new(|observer: &daemon::FenceObserver| {
                daemon::Observer::on_backfill_requested(
                    observer,
                    &SinkName::try_new("primary").unwrap(),
                    &[kernel::IndexName::try_new("users").unwrap()],
                );
            }) as Box<dyn FnOnce(&daemon::FenceObserver)>,
            "backfilling",
        ),
        (
            Box::new(|observer: &daemon::FenceObserver| {
                daemon::Observer::on_index_rebuilding(
                    observer,
                    &SinkName::try_new("primary").unwrap(),
                    &kernel::IndexName::try_new("users").unwrap(),
                );
            }),
            "rebuilding",
        ),
    ] {
        let (base, _source, _waits) = serve_staged(40, events).await;
        let (status, retry, body) = get_wait_full(&base, "position=30&timeout_ms=50").await;
        assert_eq!((status, retry.as_deref()), (503, Some("1")), "{body}");
        assert_eq!(body["reason"], reason, "{body}");
        assert_eq!(body["position"], "40", "{body}");
        assert_eq!(
            body["lanes"],
            serde_json::json!([{
                "index": users.as_ref(),
                "sink": primary.as_ref(),
                "state": reason,
            }]),
            "{body}"
        );
    }
}

#[tokio::test]
async fn status_carries_the_pid_of_the_process() {
    let (base, _source) = serve_public(10).await;

    let body: serde_json::Value = reqwest::get(format!("{base}/status"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(body["pid"], std::process::id());
    assert!(
        body.get("phase").is_some(),
        "the snapshot is still there: {body}"
    );
}

#[tokio::test]
async fn a_taken_address_names_the_flusso_holding_it() {
    let (base, _source) = serve_public(10).await;
    let address: SocketAddr = base.trim_start_matches("http://").parse().unwrap();

    let error = bind("public", address, Some(address)).await.unwrap_err();

    let text = format!("{error:#}");
    assert!(
        text.contains(&format!(
            "another flusso is already serving there; it reports pid {}, a flusso process on this host",
            std::process::id()
        )),
        "{text}"
    );
    assert!(
        !text.contains("kill"),
        "no command to kill anything: {text}"
    );
    assert!(
        error.root_cause().to_string().contains("in use"),
        "the OS error stays in the chain: {text}"
    );
}

#[test]
fn a_pid_that_is_not_a_flusso_here_is_not_called_one() {
    assert!(
        !is_local_flusso(1),
        "pid 1 here is init, not the flusso a container reports"
    );
    assert!(!is_local_flusso(u64::from(u32::MAX)), "no such process");
}

fn index(name: &str) -> IndexName {
    IndexName::try_new(name).unwrap()
}

/// A status over `users` and `orders` into `primary`, with the engine's
/// per-index states attached.
/// `archive` is configured and disabled.
fn indexed_status() -> (Arc<Status>, Arc<daemon::IndexStates>) {
    let states = Arc::new(daemon::IndexStates::new([index("users"), index("orders")]));
    let status = Arc::new(
        Status::new(
            [index("users"), index("orders"), index("archive")],
            [SinkName::try_new("primary").unwrap()],
            Instant::now(),
        )
        .with_ingest_indexes([index("users"), index("orders")]),
    );
    status.attach_index_states(Arc::clone(&states));
    (status, states)
}

/// Serve the public router over `status`; returns its base URL.
async fn serve_status(status: Arc<Status>) -> String {
    let source = Arc::new(SteadySource::default());
    let lane = stream::LaneKey::new(index("users"), SinkName::try_new("primary").unwrap());
    let fence = Fence::new(
        source as Arc<dyn ChangeCapture>,
        Arc::new(daemon::Progress::new([lane.clone()])),
        [lane],
    );
    let state = PublicState {
        status,
        registry: None,
        fence,
        waits: Waits::default(),
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, public_router(state)).await });
    base
}

/// Serve the private router over `status` and a detached control of
/// `primary`; returns its base URL, the control's receivers and its stop.
async fn serve_private(
    status: Arc<Status>,
) -> (
    String,
    std::collections::BTreeMap<SinkName, tokio::sync::mpsc::Receiver<daemon::SinkControl>>,
    daemon::Stop,
) {
    let stop = daemon::Stop::new();
    let (control, receivers) =
        DaemonControl::detached(&[SinkName::try_new("primary").unwrap()], stop.clone());
    let state = PrivateState {
        status,
        control,
        reattach_wait: std::time::Duration::from_millis(200),
    };
    let auth = Arc::new(BasicAuth::new("admin".to_owned(), "secret".to_owned()));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, private_router(state, auth)).await });
    (base, receivers, stop)
}

async fn get_json(url: &str) -> (u16, serde_json::Value) {
    let response = reqwest::get(url).await.unwrap();
    let status = response.status().as_u16();
    (
        status,
        response.json().await.unwrap_or(serde_json::Value::Null),
    )
}

/// Slice 7: `GET /indexes/<name>/status` answers that index's
/// `ingest.indexes.<name>` entry, the same JSON as `/status`'s.
#[tokio::test]
async fn an_index_status_is_its_ingest_entry() {
    let (status, states) = indexed_status();
    states.set_exemption(&index("orders"), daemon::ExemptionState::Frozen);
    let base = serve_status(Arc::clone(&status)).await;
    let (code, body) = get_json(&format!("{base}/indexes/orders/status")).await;
    assert_eq!(code, 200);
    assert_eq!(body["exemption"], "frozen");
    assert_eq!(body["working"], true);
    assert_eq!(body["documents_waiting"], 0);
    let (_, whole) = get_json(&format!("{base}/status")).await;
    assert_eq!(whole["ingest"]["indexes"]["orders"], body);
}

/// Slice 8-2 (S9): the route serves a down index's state, since, last
/// error and restarts, and a walk waiting to retry.
#[tokio::test]
async fn a_down_index_and_a_retrying_walk_are_served_on_the_index_route() {
    use daemon::Observer;
    let (status, _) = indexed_status();
    let observer = daemon::StatusObserver::new(Arc::clone(&status));
    observer.on_index_failed(
        &index("orders"),
        daemon::Work::Live,
        "the build failed",
        std::time::Duration::from_secs(4),
    );
    observer.on_index_failed(
        &index("users"),
        daemon::Work::Backfill,
        "the key read failed",
        std::time::Duration::from_secs(4),
    );
    let base = serve_status(Arc::clone(&status)).await;
    let (code, orders) = get_json(&format!("{base}/indexes/orders/status")).await;
    assert_eq!(code, 200);
    assert_eq!(orders["state"], "down");
    assert!(orders["down_since"].is_string());
    assert_eq!(orders["working"], false);
    assert_eq!(orders["held_by"], serde_json::Value::Null);
    assert_eq!(orders["last_error"]["live"]["message"], "the build failed");
    assert!(orders["last_error"]["live"]["at"].is_string());
    assert_eq!(orders["last_error"]["backfill"], serde_json::Value::Null);
    assert_eq!(
        orders["restarts"],
        serde_json::json!({"live": 0, "backfill": 0})
    );
    let (_, users) = get_json(&format!("{base}/indexes/users/status")).await;
    assert_eq!(users["state"], "running");
    assert_eq!(users["working"], true);
    assert_eq!(users["backfill"]["state"], "retrying");
    assert!(users["backfill"]["retry_at"].is_string());
    assert_eq!(
        users["last_error"]["backfill"]["message"],
        "the key read failed"
    );
    assert_eq!(users["last_error"]["live"], serde_json::Value::Null);
}

/// An unknown (or disabled) index is `404`, naming the known ones; a name
/// that is no index name is `400`.
#[tokio::test]
async fn an_unknown_or_invalid_index_status_is_refused() {
    let (status, _) = indexed_status();
    let base = serve_status(status).await;
    let (code, body) = get_json(&format!("{base}/indexes/items/status")).await;
    assert_eq!(code, 404);
    assert_eq!(body["indexes"], serde_json::json!(["orders", "users"]));
    let (code, _) = get_json(&format!("{base}/indexes/no-such!/status")).await;
    assert_eq!(code, 400);
}

/// Each surface answers `404` for the other's `/indexes/…` routes.
#[tokio::test]
async fn each_surface_serves_only_its_own_index_routes() {
    let (status, _) = indexed_status();
    let public = serve_status(Arc::clone(&status)).await;
    let (private, _receivers, _stop) = serve_private(status).await;
    let client = reqwest::Client::new();
    let reindex_on_public = client
        .post(format!("{public}/indexes/users/reindex"))
        .send()
        .await
        .unwrap();
    assert_eq!(reindex_on_public.status().as_u16(), 404);
    let status_on_private = client
        .get(format!("{private}/indexes/users/status"))
        .basic_auth("admin", Some("secret"))
        .send()
        .await
        .unwrap();
    assert_eq!(status_on_private.status().as_u16(), 404);
}

async fn post_reindex(base: &str, path: &str) -> (u16, String) {
    let response = reqwest::Client::new()
        .post(format!("{base}{path}"))
        .basic_auth("admin", Some("secret"))
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    (status, response.text().await.unwrap())
}

/// `POST /indexes/<name>/reindex` queues the rebuild on the named sink, or
/// on every sink, and answers `202`; the old `POST /reindex` is gone.
#[tokio::test]
async fn a_reindex_is_queued_on_the_index_path() {
    let (status, _) = indexed_status();
    let (base, mut receivers, _stop) = serve_private(status).await;
    let (code, body) = post_reindex(&base, "/indexes/users/reindex?sink=primary").await;
    assert_eq!(code, 202, "{body}");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&body).unwrap(),
        serde_json::json!({"index": "users", "sink": "primary", "queued": true})
    );
    let (code, body) = post_reindex(&base, "/indexes/orders/reindex").await;
    assert_eq!(code, 202);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&body).unwrap(),
        serde_json::json!({"index": "orders", "sink": null, "queued": true})
    );
    let primary = receivers
        .get_mut(&SinkName::try_new("primary").unwrap())
        .unwrap();
    for expected in ["users", "orders"] {
        let daemon::SinkControl::Reindex { indexes } = primary.try_recv().unwrap() else {
            panic!("a reindex was sent");
        };
        assert_eq!(indexes, vec![index(expected)]);
    }
    let (code, _) = post_reindex(&base, "/reindex?index=users").await;
    assert_eq!(code, 404, "the old route is gone");
}

/// The reindex route's refusals: `400` for an invalid index or sink name,
/// `404` for an unknown index or sink, `503` while stopping, `401` without
/// credentials.
#[tokio::test]
async fn a_reindex_is_refused_with_why() {
    let (status, _) = indexed_status();
    let (base, _receivers, stop) = serve_private(status).await;
    for (path, code) in [
        ("/indexes/no-such!/reindex", 400),
        ("/indexes/users/reindex?sink=Not%20A%20Sink!", 400),
        ("/indexes/items/reindex", 404),
        ("/indexes/users/reindex?sink=audit", 404),
    ] {
        assert_eq!(post_reindex(&base, path).await.0, code, "{path}");
    }
    let anonymous = reqwest::Client::new()
        .post(format!("{base}/indexes/users/reindex"))
        .send()
        .await
        .unwrap();
    assert_eq!(anonymous.status().as_u16(), 401);
    stop.request();
    let (code, body) = post_reindex(&base, "/indexes/users/reindex").await;
    assert_eq!(code, 503, "{body}");
    assert!(body.contains("stopping"), "{body}");
}

/// Review round 2 of #454, item 2: a disabled index has no status route and
/// no reindex (both `404`), and isn't named among the indexes a `404` lists;
/// `/status`'s older `indexes.<name>` keeps it, as before slice 7.
#[tokio::test]
async fn a_disabled_index_has_no_index_routes() {
    let (status, _) = indexed_status();
    let public = serve_status(Arc::clone(&status)).await;
    let (private, _receivers, _stop) = serve_private(status).await;
    let (code, body) = get_json(&format!("{public}/indexes/archive/status")).await;
    assert_eq!(code, 404);
    assert_eq!(body["indexes"], serde_json::json!(["orders", "users"]));
    assert_eq!(
        post_reindex(&private, "/indexes/archive/reindex").await.0,
        404
    );
    let (_, whole) = get_json(&format!("{public}/status")).await;
    assert!(whole["ingest"]["indexes"].get("archive").is_none());
    assert!(
        whole["indexes"].get("archive").is_some(),
        "unchanged: {whole}"
    );
}

/// ADR 0012 stage 10, O20: a reindex of an index whose rebuild is starting
/// (waiting for its attach) or running with its lane open is refused with
/// `409` and why, for a named sink and for every sink; once the rebuilding
/// lane is parked, the reindex is taken: it starts the rebuild again.
#[tokio::test]
async fn a_reindex_of_a_rebuild_starting_or_running_is_refused_with_409() {
    use daemon::Observer;
    let (status, _) = indexed_status();
    let observer = daemon::StatusObserver::new(Arc::clone(&status));
    let users = LaneKey::new(index("users"), SinkName::try_new("primary").unwrap());
    let (base, _receivers, _stop) = serve_private(Arc::clone(&status)).await;

    observer.on_lane_rebuild(&users, Some(1), Some(4));
    for path in [
        "/indexes/users/reindex?sink=primary",
        "/indexes/users/reindex",
    ] {
        let (code, body) = post_reindex(&base, path).await;
        assert_eq!(code, 409, "{path}: {body}");
        assert!(
            body.contains("the index is being rebuilt (starting)"),
            "{body}"
        );
    }

    observer.on_lane_rebuild(&users, Some(1), None);
    let (code, body) = post_reindex(&base, "/indexes/users/reindex?sink=primary").await;
    assert_eq!(code, 409, "{body}");
    assert!(body.contains("\"the index is being rebuilt\""), "{body}");

    observer.on_lane_health(
        &users,
        &daemon::LaneHealth::Parked {
            reason: daemon::ParkReason::Blocked {
                reason: "index write (api)".to_owned(),
            },
            since: tokio::time::Instant::now(),
            next_probe: tokio::time::Instant::now(),
        },
    );
    let (code, body) = post_reindex(&base, "/indexes/users/reindex?sink=primary").await;
    assert_eq!(code, 202, "a parked rebuild starts again: {body}");
}

/// ADR 0012 stage 10, §9 and O21: `/status` names a detached lane at the top
/// level and beside its sink's committed count, and shows a lane's rebuild.
#[tokio::test]
async fn status_lists_detached_lanes_and_rebuilds() {
    use daemon::Observer;
    let (status, _) = indexed_status();
    let observer = daemon::StatusObserver::new(Arc::clone(&status));
    let users = LaneKey::new(index("users"), SinkName::try_new("primary").unwrap());
    observer.on_lane_health(
        &users,
        &daemon::LaneHealth::Detached {
            since: tokio::time::Instant::now(),
            reason: "blocked: index write (api)".to_owned(),
            through: None,
        },
    );
    let snapshot = serde_json::to_value(status.snapshot()).unwrap();
    assert_eq!(
        snapshot["detached"],
        serde_json::json!([{ "index": "users", "sink": "primary" }])
    );
    assert_eq!(
        snapshot["sinks"]["primary"]["detached_lanes"],
        serde_json::json!(["users"])
    );
    observer.on_lane_rebuild(&users, Some(2), Some(5));
    assert!(status.reindex_refusal(&users).is_some());
}

/// ADR 0012 stage 10, S3: a wait for a position inside a gap a reattach
/// accepted is refused at once with `409`, `reason: gap`, naming the gap;
/// one after the gap passes.
#[tokio::test]
async fn a_wait_inside_a_reattachs_gap_is_refused_with_409() {
    let (base, _source, _waits) = serve_staged(40, |observer| {
        daemon::Observer::on_lane_gaps(
            observer,
            &LaneKey::new(index("users"), SinkName::try_new("primary").unwrap()),
            &[
                sink::Gap {
                    from: Some("10".to_owned()),
                    to: Some("20".to_owned()),
                },
                sink::Gap {
                    from: Some("25".to_owned()),
                    to: Some("28".to_owned()),
                },
            ],
        );
    })
    .await;
    let (status, body) = get_wait(&base, "position=15&until=applied").await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["reason"], "gap");
    assert_eq!(
        body["lanes"],
        json!([{
            "index": "users",
            "sink": "primary",
            "state": "gap",
            "gap": { "from": "10", "to": "20" },
        }])
    );
    assert!(
        body["error"].as_str().unwrap().contains("from 10 to 20"),
        "{body}"
    );
    let (status, _) = get_wait(&base, "position=22&until=applied").await;
    assert_eq!(status, 200, "between the two gaps");
    let (status, body) = get_wait(&base, "position=27&until=applied").await;
    assert_eq!(status, 409, "inside the second gap: {body}");
    assert_eq!(body["lanes"][0]["gap"], json!({ "from": "25", "to": "28" }));
    let (status, _) = get_wait(&base, "position=30&until=applied").await;
    assert_eq!(status, 200);
}

/// ADR 0012 stage 10, O16: `POST /indexes/<name>/reattach` names its sink
/// and says `accept_gap=true` (`400` otherwise), refuses an unknown sink
/// (`404`), a lane that is not detached or is being attached, and a sink that
/// can rebuild (`409`), and queues the reattach on its sink's engine (`202`).
#[tokio::test]
async fn a_reattach_is_refused_unless_it_applies_and_queued_when_it_does() {
    use daemon::Observer;
    let (status, _) = indexed_status();
    let observer = daemon::StatusObserver::new(Arc::clone(&status));
    let primary = SinkName::try_new("primary").unwrap();
    let users = LaneKey::new(index("users"), primary.clone());
    let stop = daemon::Stop::new();
    let (control, mut receivers) = DaemonControl::detached(&[primary.clone()], stop.clone());
    let serve = |control: DaemonControl| {
        let state = PrivateState {
            status: Arc::clone(&status),
            control,
            reattach_wait: std::time::Duration::from_millis(200),
        };
        async move {
            let auth = Arc::new(BasicAuth::new("admin".to_owned(), "secret".to_owned()));
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            tokio::spawn(async move { axum::serve(listener, private_router(state, auth)).await });
            base
        }
    };
    let can_rebuild = serve(control.clone()).await;
    let base = serve(control.with_live_only([primary.clone()])).await;

    for (path, code) in [
        ("/indexes/users/reattach?accept_gap=true", 400),
        ("/indexes/users/reattach?sink=primary", 400),
        ("/indexes/users/reattach?sink=primary&accept_gap=yes", 400),
        ("/indexes/users/reattach?sink=audit&accept_gap=true", 404),
        ("/indexes/nope/reattach?sink=primary&accept_gap=true", 404),
        ("/indexes/users/reattach?sink=primary&accept_gap=true", 409),
    ] {
        let (got, body) = post_reindex(&base, path).await;
        assert_eq!(got, code, "{path}: {body}");
    }
    let (_, body) = post_reindex(
        &base,
        "/indexes/users/reattach?sink=primary&accept_gap=true",
    )
    .await;
    assert!(body.contains("the lane is not detached"), "{body}");

    observer.on_lane_health(
        &users,
        &daemon::LaneHealth::Detached {
            since: tokio::time::Instant::now(),
            reason: "blocked".to_owned(),
            through: Some("10".to_owned()),
        },
    );
    let (got, body) = post_reindex(
        &can_rebuild,
        "/indexes/users/reattach?sink=primary&accept_gap=true",
    )
    .await;
    assert_eq!(got, 409, "{body}");
    assert!(
        body.contains("flusso reindex users --sink primary"),
        "{body}"
    );
    assert!(body.contains("\"reason\""), "{body}");

    observer.on_lane_rebuild(&users, None, Some(3));
    let (got, body) = post_reindex(
        &base,
        "/indexes/users/reattach?sink=primary&accept_gap=true",
    )
    .await;
    assert_eq!(got, 409, "{body}");
    assert!(body.contains("being attached already"), "{body}");

    observer.on_lane_rebuild(&users, None, None);
    // The engine's verdict decides the answer (review of step 4, S4).
    let mut engine = receivers.remove(&primary).unwrap();
    let answers = tokio::spawn(async move {
        // What the engine does with each reattach in turn: answer, drop the
        // reply (its run ended), or hold it unanswered (busy past the wait).
        enum Does {
            Answers(daemon::ReattachVerdict),
            Drops,
            Holds,
        }
        let mut held = Vec::new();
        for does in [
            Does::Answers(daemon::ReattachVerdict::Started),
            Does::Answers(daemon::ReattachVerdict::Refused(
                "the seed marker is not on the seeded generation reads see; restart flusso"
                    .to_owned(),
            )),
            Does::Drops,
            Does::Answers(daemon::ReattachVerdict::Failed(
                "the meta index refused".to_owned(),
            )),
            Does::Holds,
        ] {
            let Some(daemon::SinkControl::Reattach { index, reply }) = engine.recv().await else {
                panic!("a reattach");
            };
            assert_eq!(index, kernel::IndexName::try_new("users").unwrap());
            match does {
                Does::Answers(verdict) => reply.send(verdict),
                Does::Drops => drop(reply),
                Does::Holds => held.push(reply),
            }
        }
        (engine, held)
    });
    let path = "/indexes/users/reattach?sink=primary&accept_gap=true";
    let (got, body) = post_reindex(&base, path).await;
    assert_eq!(got, 202, "{body}");
    assert!(body.contains("\"started\":true"), "{body}");
    let (got, body) = post_reindex(&base, path).await;
    assert_eq!(got, 409, "the engine refused: {body}");
    assert!(body.contains("restart flusso"), "{body}");
    let (got, body) = post_reindex(&base, path).await;
    assert_eq!(got, 503, "no verdict, the run ended: {body}");
    let (got, body) = post_reindex(&base, path).await;
    assert_eq!(got, 503, "the engine failed a step: {body}");
    assert!(body.contains("Nothing accepted is lost"), "{body}");
    assert!(body.contains("/status tells"), "{body}");
    let (got, body) = post_reindex(&base, path).await;
    assert_eq!(got, 503, "no verdict within the wait: {body}");
    assert!(body.contains("no verdict within"), "{body}");
    let (_engine, _held) = answers.await.unwrap();

    // A detached index on a sink that can't rebuild: a reindex names reattach.
    let (got, body) = post_reindex(&base, "/indexes/users/reindex?sink=primary").await;
    assert_eq!(got, 409, "{body}");
    assert!(
        body.contains("flusso reattach users --sink primary --accept-gap"),
        "{body}"
    );

    // Stopping: every operation is 503, before any other refusal.
    stop.request();
    for path in [path, "/indexes/users/reindex?sink=primary"] {
        let (got, body) = post_reindex(&base, path).await;
        assert_eq!(got, 503, "{path}: {body}");
        assert!(body.contains("stopping"), "{body}");
    }
}

/// ADR 0012 stage 10, O16: `/status` shows a lane's accepted gap until a
/// seed rebuilds the index.
#[tokio::test]
async fn status_shows_a_lanes_gaps_until_a_seed() {
    use daemon::Observer;
    let primary = SinkName::try_new("primary").unwrap();
    let users = LaneKey::new(index("users"), primary.clone());
    let fence = Fence::new(
        Arc::new(SteadySource::default()) as Arc<dyn ChangeCapture>,
        Arc::new(daemon::Progress::new([users.clone()])),
        [users.clone()],
    );
    let status = Arc::new(
        Status::new([index("users")], [primary.clone()], Instant::now()).with_fence(fence),
    );
    let observer = daemon::StatusObserver::new(Arc::clone(&status));
    observer.on_lane_gaps(
        &users,
        &[
            sink::Gap {
                from: Some("2".to_owned()),
                to: Some("5".to_owned()),
            },
            sink::Gap {
                from: Some("10".to_owned()),
                to: None,
            },
        ],
    );
    let snapshot = serde_json::to_value(status.snapshot()).unwrap();
    assert_eq!(
        snapshot["sinks"]["primary"]["lanes"]["users"]["gaps"],
        json!([{ "from": "2", "to": "5" }, { "from": "10", "to": null }])
    );
    observer.on_index_seeded(&primary, &index("users"));
    let snapshot = serde_json::to_value(status.snapshot()).unwrap();
    assert!(
        snapshot["sinks"]["primary"]["lanes"]["users"]
            .get("gaps")
            .is_none()
    );
}

/// Review of step 4, S2: a reindex of a rebuild parked on its seed's retire
/// block is refused with `409`, naming the block and what lifts it: a
/// restart would delete the complete generation and meet the same block.
#[tokio::test]
async fn a_reindex_of_a_rebuild_waiting_on_its_seeds_block_is_refused_with_409() {
    use daemon::Observer;
    let (status, _) = indexed_status();
    let observer = daemon::StatusObserver::new(Arc::clone(&status));
    let users = LaneKey::new(index("users"), SinkName::try_new("primary").unwrap());
    let (base, _receivers, _stop) = serve_private(Arc::clone(&status)).await;
    observer.on_lane_rebuild(&users, Some(1), None);
    observer.on_lane_health(
        &users,
        &daemon::LaneHealth::Parked {
            reason: daemon::ParkReason::SeedBlocked {
                reason: "the old generation users_1 can't be retired: index write (api); lift \
                         the block on users_1"
                    .to_owned(),
            },
            since: tokio::time::Instant::now(),
            next_probe: tokio::time::Instant::now(),
        },
    );
    let (code, body) = post_reindex(&base, "/indexes/users/reindex?sink=primary").await;
    assert_eq!(code, 409, "{body}");
    assert!(
        body.contains("waits for the block on the serving generation"),
        "{body}"
    );
    assert!(body.contains("lift the block on users_1"), "{body}");
}

//! The `/wait` endpoint over a real listener: every status it answers with.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use async_trait::async_trait;
use futures::stream::BoxStream;
use kernel::Position;
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

    fn confirm(&self, _: Position) {}

    fn parse_position(&self, text: &str) -> source::Result<SourcePosition> {
        text.parse()
            .map(SourcePosition)
            .map_err(|_| SourceError::Decode(format!("`{text}` is not a number")))
    }

    fn reached(&self, _: Option<Position>) -> Option<SourcePosition> {
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
    let waits = Waits::default();
    let source = Arc::new(SteadySource::default());
    source.at.store(at, Ordering::SeqCst);
    let primary = SinkName::try_new("primary").unwrap();
    let state = PublicState {
        status: Arc::new(Status::new([], [primary.clone()], Instant::now())),
        registry: None,
        fence: Fence::new(Arc::clone(&source) as Arc<dyn ChangeCapture>, [primary]),
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
        json!({ "reached": true, "position": "40", "until": "searchable", "sink": null })
    );

    let (status, body) = get_wait(&base, "position=40&until=applied&sink=primary").await;
    assert_eq!(status, 200);
    assert_eq!(
        body,
        json!({ "reached": true, "position": "40", "until": "applied", "sink": "primary" })
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
        json!({ "reached": false, "position": "40", "until": "searchable", "sink": null })
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

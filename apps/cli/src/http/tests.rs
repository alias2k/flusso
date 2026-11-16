//! The `/wait` endpoint over a real listener: every status it answers with.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use async_trait::async_trait;
use futures::stream::BoxStream;
use kernel::Position;
use source::cdc::{ChangeCapture, ChangeEvent, Continuity, LiveChange, SourcePosition};
use source::{SnapshotTable, SourceError};

use super::*;

/// A source that has read through `at` with nothing outstanding, so every sink
/// stands there for both levels.
#[derive(Debug, Default)]
struct SteadySource {
    at: AtomicU64,
    closed: AtomicBool,
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

    async fn snapshot(
        &self,
        _: &[SnapshotTable],
    ) -> source::Result<BoxStream<'static, source::Result<ChangeEvent>>> {
        Ok(Box::pin(futures::stream::empty()))
    }

    fn parse_position(&self, text: &str) -> source::Result<SourcePosition> {
        text.parse()
            .map(SourcePosition)
            .map_err(|_| SourceError::Decode(format!("`{text}` is not a number")))
    }

    fn reached(&self, _: Option<Position>) -> Option<SourcePosition> {
        (!self.closed.load(Ordering::SeqCst))
            .then(|| SourcePosition(self.at.load(Ordering::SeqCst)))
    }
}

/// Serve the public router over a source standing at `at`; returns its base
/// URL and the source.
async fn serve_public(at: u64) -> (String, Arc<SteadySource>) {
    let (base, source, _) = serve_stoppable(at).await;
    (base, source)
}

/// [`serve_public`], plus the sender that tells the surface flusso is stopping.
async fn serve_stoppable(at: u64) -> (String, Arc<SteadySource>, watch::Sender<bool>) {
    let (stopping, stopping_rx) = watch::channel(false);
    let source = Arc::new(SteadySource::default());
    source.at.store(at, Ordering::SeqCst);
    let primary = SinkName::try_new("primary").unwrap();
    let state = PublicState {
        status: Arc::new(Status::new([], [primary.clone()], Instant::now())),
        registry: None,
        fence: Fence::new(Arc::clone(&source) as Arc<dyn ChangeCapture>, [primary]),
        stopping: stopping_rx,
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, public_router(state)).await });
    (base, source, stopping)
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

#[tokio::test]
async fn an_open_wait_answers_503_once_flusso_starts_to_stop() {
    let (base, _source, stopping) = serve_stoppable(10).await;
    let waiting =
        tokio::spawn(async move { get_wait(&base, "position=999&timeout_ms=60000").await });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    stopping.send(true).unwrap();
    let (status, body) = tokio::time::timeout(std::time::Duration::from_secs(2), waiting)
        .await
        .expect("the wait is answered at once")
        .unwrap();

    assert_eq!(status, 503);
    assert_eq!(body["error"], "flusso is shutting down");
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

    assert!(
        error.to_string().contains(&format!(
            "another flusso (pid {}) is already running there",
            std::process::id()
        )),
        "{error}"
    );
}

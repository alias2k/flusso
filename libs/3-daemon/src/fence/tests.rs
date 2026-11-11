use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use futures::stream::BoxStream;
use source::cdc::{AdvanceListener, ChangeEvent, Continuity, LiveChange};
use source::{SnapshotTable, SourceError};

use super::*;

/// A source whose positions are plain numbers. It mirrors the Postgres
/// bookkeeping in miniature: `emit` registers a change at a source position,
/// `confirm` forgets the changes at or before a stream position, and `reached`
/// answers the horizon once nothing is outstanding past the asked position.
#[derive(Default)]
struct NumberedSource {
    state: Mutex<Numbered>,
    listeners: Mutex<Vec<AdvanceListener>>,
    closed: AtomicBool,
}

impl std::fmt::Debug for NumberedSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NumberedSource")
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Default)]
struct Numbered {
    entries: BTreeMap<u64, u64>,
    horizon: u64,
    floor: u64,
}

impl NumberedSource {
    /// A change at stream position `seq`, committed at source position `at`.
    fn emit(&self, seq: u64, at: u64) {
        let mut state = self.state.lock().unwrap();
        state.entries.insert(seq, at);
        state.horizon = state.horizon.max(at);
    }

    /// The source read on to `at` with nothing new to deliver.
    fn idle_to(&self, at: u64) {
        self.state.lock().unwrap().horizon = at;
        for listener in self.listeners.lock().unwrap().iter() {
            listener();
        }
    }

    fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }
}

#[async_trait]
impl ChangeCapture for NumberedSource {
    async fn continuity(&self) -> source::Result<Continuity> {
        Ok(Continuity::Resumed)
    }

    async fn prepare(&self) -> source::Result<()> {
        Ok(())
    }

    async fn live(&self) -> source::Result<BoxStream<'static, source::Result<LiveChange>>> {
        Ok(Box::pin(futures::stream::empty()))
    }

    fn confirm(&self, position: Position) {
        let mut state = self.state.lock().unwrap();
        let reached = reach(&state, Some(position.0));
        state.floor = state.floor.max(reached);
        state.entries = state.entries.split_off(&(position.0 + 1));
    }

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

    fn reached(&self, position: Option<Position>) -> Option<SourcePosition> {
        if self.closed.load(Ordering::SeqCst) {
            return None;
        }
        let state = self.state.lock().unwrap();
        Some(SourcePosition(reach(&state, position.map(|p| p.0))))
    }

    fn on_advance(&self, listener: AdvanceListener) {
        self.listeners.lock().unwrap().push(listener);
    }
}

fn reach(state: &Numbered, seq: Option<u64>) -> u64 {
    let next = match seq {
        Some(seq) => state.entries.range(seq + 1..).next(),
        None => state.entries.iter().next(),
    };
    match next {
        None => state.horizon.max(state.floor),
        Some((_, &next)) => seq
            .and_then(|seq| state.entries.range(..=seq).next_back().map(|(_, at)| *at))
            .unwrap_or(0)
            .max(state.floor)
            .min(next - 1),
    }
}

fn sink(name: &str) -> SinkName {
    SinkName::try_new(name).unwrap()
}

fn fence(sinks: &[&str]) -> (Fence, Arc<NumberedSource>, FenceObserver) {
    let source = Arc::new(NumberedSource::default());
    let fence = Fence::new(
        Arc::clone(&source) as Arc<dyn ChangeCapture>,
        sinks.iter().map(|name| sink(name)),
    );
    let observer = FenceObserver::new(fence.clone());
    (fence, source, observer)
}

fn request(position: &str, level: FenceLevel, on: Option<&str>) -> FenceRequest {
    FenceRequest {
        position: position.to_owned(),
        level,
        sink: on.map(sink),
        timeout: Duration::from_secs(5),
    }
}

fn reached(position: &str) -> Result<FenceOutcome, FenceError> {
    Ok(FenceOutcome::Reached {
        position: position.to_owned(),
    })
}

fn timed_out(position: &str) -> Result<FenceOutcome, FenceError> {
    Ok(FenceOutcome::TimedOut {
        position: position.to_owned(),
    })
}

#[tokio::test(start_paused = true)]
async fn a_fence_passes_once_the_sink_applied_the_position() {
    let (fence, source, observer) = fence(&["primary"]);
    source.emit(0, 20);

    let waiting = tokio::spawn({
        let fence = fence.clone();
        async move { fence.wait(&request("20", FenceLevel::Applied, None)).await }
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!waiting.is_finished(), "the change is not applied yet");

    observer.on_position_applied(&sink("primary"), Position(0));
    assert_eq!(waiting.await.unwrap(), reached("20"));
}

#[tokio::test(start_paused = true)]
async fn a_position_already_reached_returns_at_once() {
    let (fence, source, _) = fence(&["primary"]);
    source.idle_to(40);
    assert_eq!(
        fence
            .wait(&request("35", FenceLevel::Searchable, None))
            .await,
        reached("40"),
        "nothing outstanding: both levels follow the source"
    );
}

#[tokio::test(start_paused = true)]
async fn a_fence_times_out_reporting_how_far_it_got() {
    let (fence, source, observer) = fence(&["primary"]);
    source.emit(0, 10);
    source.emit(1, 20);
    observer.on_position_applied(&sink("primary"), Position(0));

    let started = Instant::now();
    assert_eq!(
        fence.wait(&request("20", FenceLevel::Applied, None)).await,
        timed_out("10")
    );
    assert_eq!(started.elapsed(), Duration::from_secs(5));
}

#[tokio::test(start_paused = true)]
async fn searchable_trails_applied_until_the_engine_reports_it() {
    let (fence, source, observer) = fence(&["primary"]);
    source.emit(0, 20);
    observer.on_position_applied(&sink("primary"), Position(0));
    // Every sink acknowledged, so the source forgets the change: the fence
    // must not take "nothing outstanding" for "searchable".
    source.confirm(Position(0));

    assert_eq!(
        fence.wait(&request("20", FenceLevel::Applied, None)).await,
        reached("20")
    );
    assert_eq!(
        fence
            .wait(&FenceRequest {
                timeout: Duration::from_millis(100),
                ..request("20", FenceLevel::Searchable, None)
            })
            .await,
        timed_out("0"),
    );

    observer.on_position_searchable(&sink("primary"), Position(0));
    assert_eq!(
        fence
            .wait(&request("20", FenceLevel::Searchable, None))
            .await,
        reached("20")
    );
}

#[tokio::test(start_paused = true)]
async fn a_stalled_sibling_blocks_an_all_sinks_fence_but_not_one_on_the_healthy_sink() {
    let (fence, source, observer) = fence(&["audit", "primary"]);
    source.emit(0, 20);
    observer.on_position_applied(&sink("primary"), Position(0));
    observer.on_position_searchable(&sink("primary"), Position(0));

    assert_eq!(
        fence
            .wait(&request("20", FenceLevel::Searchable, Some("primary")))
            .await,
        reached("20")
    );
    assert_eq!(
        fence.wait(&request("20", FenceLevel::Applied, None)).await,
        timed_out("0"),
        "an all-sinks fence waits for the slowest sink"
    );
}

#[tokio::test(start_paused = true)]
async fn idle_source_progress_wakes_a_waiter() {
    let (fence, source, observer) = fence(&["primary"]);
    source.emit(0, 20);
    observer.on_position_applied(&sink("primary"), Position(0));
    observer.on_position_searchable(&sink("primary"), Position(0));

    // The client's position lies past its commit: unrelated WAL followed it.
    let waiting = tokio::spawn({
        let fence = fence.clone();
        async move {
            fence
                .wait(&request("25", FenceLevel::Searchable, None))
                .await
        }
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!waiting.is_finished());

    source.idle_to(30);
    assert_eq!(waiting.await.unwrap(), reached("30"));
}

#[tokio::test(start_paused = true)]
async fn a_reopened_source_stream_never_walks_a_sink_back() {
    let (fence, source, observer) = fence(&["primary"]);
    source.emit(0, 20);
    observer.on_position_applied(&sink("primary"), Position(0));
    assert_eq!(
        fence.wait(&request("20", FenceLevel::Applied, None)).await,
        reached("20")
    );

    // The stream reopens and redelivers the change under a new position.
    source.emit(1, 20);
    assert_eq!(
        fence.wait(&request("20", FenceLevel::Applied, None)).await,
        reached("20"),
        "the sink applied it once already"
    );
}

#[tokio::test]
async fn a_wait_that_cannot_start_says_why() {
    let (fence, source, _) = fence(&["primary"]);
    assert!(matches!(
        fence
            .wait(&request("soon", FenceLevel::Applied, None))
            .await,
        Err(FenceError::InvalidPosition(_))
    ));
    assert_eq!(
        fence
            .wait(&request("1", FenceLevel::Applied, Some("audit")))
            .await,
        Err(FenceError::UnknownSink("audit".to_owned()))
    );
    source.close();
    assert!(matches!(
        fence.wait(&request("1", FenceLevel::Applied, None)).await,
        Err(FenceError::Unavailable(_))
    ));
}

#[test]
fn positions_report_each_sink_and_the_slowest() {
    let (fence, source, observer) = fence(&["audit", "primary"]);
    source.emit(0, 20);
    source.emit(1, 30);
    observer.on_position_applied(&sink("primary"), Position(1));
    observer.on_position_applied(&sink("audit"), Position(0));
    observer.on_position_searchable(&sink("audit"), Position(0));

    let positions = fence.positions();
    assert_eq!(positions.applied.as_deref(), Some("20"));
    assert_eq!(positions.searchable.as_deref(), Some("0"));
    let primary = &positions.sinks[&sink("primary")];
    assert_eq!(primary.applied.as_deref(), Some("30"));
    assert_eq!(primary.searchable.as_deref(), Some("0"));
    let audit = &positions.sinks[&sink("audit")];
    assert_eq!(audit.applied.as_deref(), Some("20"));
    assert_eq!(audit.searchable.as_deref(), Some("20"));
}

#[test]
fn the_status_document_carries_the_positions() {
    let (fence, source, observer) = fence(&["primary"]);
    let status = crate::Status::new(
        [kernel::IndexName::try_new("users").unwrap()],
        [sink("primary")],
        std::time::Instant::now(),
    )
    .with_fence(fence);

    source.emit(0, 20);
    observer.on_position_applied(&sink("primary"), Position(0));

    let json = serde_json::to_value(status.snapshot()).unwrap();
    assert_eq!(json["applied_position"], "20");
    assert_eq!(json["searchable_position"], "0");
    assert_eq!(json["sinks"]["primary"]["applied_position"], "20");
    assert_eq!(json["sinks"]["primary"]["searchable_position"], "0");
}

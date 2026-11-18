use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use futures::stream::BoxStream;
use source::SourceError;
use source::cdc::{AdvanceListener, Continuity, Landed, LiveChange};

use stream::LaneKey;

use super::*;

/// A source whose positions are plain numbers. It mirrors the Postgres
/// bookkeeping in miniature: `emit` stamps a single-change commit at a source
/// position, and `reached` reads the answer off the stamps and the horizon.
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
    next_seq: u64,
    horizon: u64,
}

impl NumberedSource {
    /// A single-change commit at source position `at`.
    fn emit(&self, at: u64) -> Position {
        let mut state = self.state.lock().unwrap();
        let seq = state.next_seq;
        state.next_seq += 1;
        let position = Position::stamped(seq, at - 1, state.horizon, at);
        state.horizon = state.horizon.max(at);
        position
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

    fn confirm(&self, _: Landed) {}

    fn parse_position(&self, text: &str) -> source::Result<SourcePosition> {
        text.parse()
            .map(SourcePosition)
            .map_err(|_| SourceError::Decode(format!("`{text}` is not a number")))
    }

    fn reached(&self, landed: Landed) -> Option<SourcePosition> {
        if self.closed.load(Ordering::SeqCst) {
            return None;
        }
        let state = self.state.lock().unwrap();
        Some(SourcePosition(match landed {
            Landed::Before(oldest) => oldest.fence_before,
            Landed::Through(last) if last.seq + 1 == state.next_seq => state.horizon,
            Landed::Through(last) => last.confirm_through,
            Landed::Idle if state.next_seq == 0 => state.horizon,
            Landed::Idle => 0,
        }))
    }

    fn on_advance(&self, listener: AdvanceListener) {
        self.listeners.lock().unwrap().push(listener);
    }
}

fn sink(name: &str) -> SinkName {
    SinkName::try_new(name).unwrap()
}

/// A fence over a numbered source, with the progress the engines would feed.
struct Rig {
    fence: Fence,
    source: Arc<NumberedSource>,
    progress: Arc<Progress>,
    observer: FenceObserver,
    lanes: Vec<LaneKey>,
}

fn index(name: &str) -> IndexName {
    IndexName::try_new(name).unwrap()
}

/// The `users` lane of sink `on`.
fn lane(on: &str) -> LaneKey {
    LaneKey::new(index("users"), sink(on))
}

impl Rig {
    /// The ingest engine published a batch of `position` to every lane.
    fn route(&self, position: Position) {
        for lane in &self.lanes {
            self.progress.published(lane, position, position);
        }
    }

    /// The `users` lane of `on` landed the item of `position`.
    fn applied(&self, on: &str, position: Position) {
        self.applied_on(&lane(on), position);
    }

    /// `lane` landed the item of `position`.
    fn applied_on(&self, lane: &LaneKey, position: Position) {
        self.progress.applied(lane, position);
        self.observer.on_position_applied(lane, position);
    }

    /// The `users` lane of `on` made everything through `position`
    /// searchable.
    fn searchable(&self, on: &str, position: Position) {
        self.searchable_on(&lane(on), position);
    }

    /// `lane` made everything through `position` searchable.
    fn searchable_on(&self, lane: &LaneKey, position: Position) {
        self.progress.searchable(lane, position);
        self.observer.on_position_searchable(lane, position);
    }
}

/// A fence over the `users` index of `sinks`.
fn fence(sinks: &[&str]) -> Rig {
    fence_over(&["users"], sinks)
}

/// A fence over every lane of `indexes` × `sinks`.
fn fence_over(indexes: &[&str], sinks: &[&str]) -> Rig {
    let source = Arc::new(NumberedSource::default());
    let lanes: Vec<LaneKey> = indexes
        .iter()
        .flat_map(|i| sinks.iter().map(move |s| LaneKey::new(index(i), sink(s))))
        .collect();
    let progress = Arc::new(Progress::new(lanes.iter().cloned()));
    let fence = Fence::new(
        Arc::clone(&source) as Arc<dyn ChangeCapture>,
        Arc::clone(&progress),
        lanes.iter().cloned(),
    );
    let observer = FenceObserver::new(fence.clone());
    Rig {
        fence,
        source,
        progress,
        observer,
        lanes,
    }
}

fn request(position: &str, level: FenceLevel, on: Option<&str>) -> FenceRequest {
    FenceRequest {
        position: position.to_owned(),
        level,
        sink: on.map(sink),
        indexes: Vec::new(),
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
    let rig = fence(&["primary"]);
    let change = rig.source.emit(20);
    rig.route(change);

    let waiting = tokio::spawn({
        let fence = rig.fence.clone();
        async move { fence.wait(&request("20", FenceLevel::Applied, None)).await }
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!waiting.is_finished(), "the change is not applied yet");

    rig.applied("primary", change);
    assert_eq!(waiting.await.unwrap(), reached("20"));
}

#[tokio::test(start_paused = true)]
async fn a_position_already_reached_returns_at_once() {
    let rig = fence(&["primary"]);
    rig.source.idle_to(40);
    assert_eq!(
        rig.fence
            .wait(&request("35", FenceLevel::Searchable, None))
            .await,
        reached("40"),
        "nothing outstanding: both levels follow the source"
    );
}

#[tokio::test(start_paused = true)]
async fn a_fence_times_out_reporting_how_far_it_got() {
    let rig = fence(&["primary"]);
    let first = rig.source.emit(10);
    let second = rig.source.emit(20);
    rig.route(first);
    rig.route(second);
    rig.applied("primary", first);

    let started = Instant::now();
    assert_eq!(
        rig.fence
            .wait(&request("20", FenceLevel::Applied, None))
            .await,
        timed_out("19"),
        "everything short of the second commit"
    );
    assert_eq!(started.elapsed(), Duration::from_secs(5));
}

#[tokio::test(start_paused = true)]
async fn searchable_trails_applied_until_the_engine_reports_it() {
    let rig = fence(&["primary"]);
    let change = rig.source.emit(20);
    rig.route(change);
    rig.applied("primary", change);

    assert_eq!(
        rig.fence
            .wait(&request("20", FenceLevel::Applied, None))
            .await,
        reached("20")
    );
    assert_eq!(
        rig.fence
            .wait(&FenceRequest {
                timeout: Duration::from_millis(100),
                ..request("20", FenceLevel::Searchable, None)
            })
            .await,
        timed_out("19"),
    );

    rig.searchable("primary", change);
    assert_eq!(
        rig.fence
            .wait(&request("20", FenceLevel::Searchable, None))
            .await,
        reached("20")
    );
}

#[tokio::test(start_paused = true)]
async fn a_stalled_sibling_blocks_an_all_sinks_fence_but_not_one_on_the_healthy_sink() {
    let rig = fence(&["audit", "primary"]);
    let change = rig.source.emit(20);
    rig.route(change);
    rig.applied("primary", change);
    rig.searchable("primary", change);

    assert_eq!(
        rig.fence
            .wait(&request("20", FenceLevel::Searchable, Some("primary")))
            .await,
        reached("20")
    );
    assert_eq!(
        rig.fence
            .wait(&request("20", FenceLevel::Applied, None))
            .await,
        timed_out("19"),
        "an all-sinks fence waits for the slowest sink"
    );
}

#[tokio::test(start_paused = true)]
async fn idle_source_progress_wakes_a_waiter() {
    let rig = fence(&["primary"]);
    let change = rig.source.emit(20);
    rig.route(change);
    rig.applied("primary", change);
    rig.searchable("primary", change);

    // The client's position lies past its commit: unrelated WAL followed it.
    let waiting = tokio::spawn({
        let fence = rig.fence.clone();
        async move {
            fence
                .wait(&request("25", FenceLevel::Searchable, None))
                .await
        }
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!waiting.is_finished());

    rig.source.idle_to(30);
    assert_eq!(waiting.await.unwrap(), reached("30"));
}

#[tokio::test(start_paused = true)]
async fn a_reopened_source_stream_never_walks_a_sink_back() {
    let rig = fence(&["primary"]);
    let change = rig.source.emit(20);
    rig.route(change);
    rig.applied("primary", change);
    assert_eq!(
        rig.fence
            .wait(&request("20", FenceLevel::Applied, None))
            .await,
        reached("20")
    );

    // The stream reopens and redelivers the change under a new position.
    let replayed = rig.source.emit(20);
    rig.route(replayed);
    assert_eq!(
        rig.fence
            .wait(&request("20", FenceLevel::Applied, None))
            .await,
        reached("20"),
        "the sink applied it once already"
    );
}

#[tokio::test]
async fn a_wait_that_cannot_start_says_why() {
    let rig = fence(&["primary"]);
    assert!(matches!(
        rig.fence
            .wait(&request("soon", FenceLevel::Applied, None))
            .await,
        Err(FenceError::InvalidPosition(_))
    ));
    assert_eq!(
        rig.fence
            .wait(&request("1", FenceLevel::Applied, Some("audit")))
            .await,
        Err(FenceError::UnknownSink("audit".to_owned()))
    );
    rig.source.close();
    assert!(matches!(
        rig.fence
            .wait(&request("1", FenceLevel::Applied, None))
            .await,
        Err(FenceError::Unavailable(_))
    ));
}

#[test]
fn positions_report_each_sink_and_the_slowest() {
    let rig = fence(&["audit", "primary"]);
    let first = rig.source.emit(20);
    let second = rig.source.emit(30);
    rig.route(first);
    rig.route(second);
    rig.applied("primary", first);
    rig.applied("primary", second);
    rig.applied("audit", first);
    rig.searchable("audit", first);

    let positions = rig.fence.positions();
    assert_eq!(positions.applied.as_deref(), Some("29"));
    assert_eq!(positions.searchable.as_deref(), Some("19"));
    let primary = &positions.sinks[&sink("primary")];
    assert_eq!(primary.applied.as_deref(), Some("30"));
    assert_eq!(primary.searchable.as_deref(), Some("19"));
    let audit = &positions.sinks[&sink("audit")];
    assert_eq!(audit.applied.as_deref(), Some("29"));
    assert_eq!(audit.searchable.as_deref(), Some("29"));
}

#[test]
fn the_status_document_carries_the_positions() {
    let rig = fence(&["primary"]);
    let status = crate::Status::new(
        [kernel::IndexName::try_new("users").unwrap()],
        [sink("primary")],
        std::time::Instant::now(),
    )
    .with_fence(rig.fence.clone());

    let change = rig.source.emit(20);
    rig.route(change);
    rig.applied("primary", change);

    let json = serde_json::to_value(status.snapshot()).unwrap();
    assert_eq!(json["applied_position"], "20");
    assert_eq!(json["searchable_position"], "19");
    assert_eq!(json["sinks"]["primary"]["applied_position"], "20");
    assert_eq!(json["sinks"]["primary"]["searchable_position"], "19");
}

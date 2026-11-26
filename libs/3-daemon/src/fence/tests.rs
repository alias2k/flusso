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
            Landed::Free => state.horizon,
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
            self.progress
                .published(lane, stream::ItemKey::new(position.seq), position, position);
        }
    }

    /// The `users` lane of `on` landed the item of `position`.
    fn applied(&self, on: &str, position: Position) {
        self.applied_on(&lane(on), position);
    }

    /// `lane` landed the item of `position`.
    fn applied_on(&self, lane: &LaneKey, position: Position) {
        self.progress
            .applied(lane, stream::ItemKey::new(position.seq));
        self.observer.on_position_applied(lane, position);
    }

    /// The `users` lane of `on` made everything through `position`
    /// searchable.
    fn searchable(&self, on: &str, position: Position) {
        self.searchable_on(&lane(on), position);
    }

    /// `lane` made everything through `position` searchable.
    fn searchable_on(&self, lane: &LaneKey, position: Position) {
        self.progress
            .searchable(lane, stream::ItemKey::new(position.seq));
        self.observer.on_position_searchable(lane, position);
    }
}

/// A fence over the `users` index of `sinks`, every sink staged.
fn fence(sinks: &[&str]) -> Rig {
    fence_over(&["users"], sinks)
}

/// A fence over every lane of `indexes` × `sinks`, every sink staged.
fn fence_over(indexes: &[&str], sinks: &[&str]) -> Rig {
    let rig = fence_unstaged(indexes, sinks);
    for on in sinks {
        rig.observer.on_sink_started(&sink(on));
    }
    rig
}

/// A fence over every lane of `indexes` × `sinks`, no sink staged yet.
fn fence_unstaged(indexes: &[&str], sinks: &[&str]) -> Rig {
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
    assert_eq!(
        json["sinks"]["primary"]["lanes"]["users"],
        serde_json::json!({
            "applied_position": "20",
            "searchable_position": "19",
            "outstanding": 0,
            "health": "open",
        })
    );
}

/// ADR 0012 stage 8c: a lane whose oldest item stays outstanding ages in
/// `/status` and pins the slot; its sibling that landed stays at 0.
#[test]
fn the_status_document_carries_each_lanes_stall_and_the_pinning_lane() {
    let rig = fence_over(&["users", "orders"], &["primary"]);
    let stalls = Arc::new(crate::Stalls::new(rig.lanes.iter().cloned()));
    let status = crate::Status::new(
        [index("users"), index("orders")],
        [sink("primary")],
        std::time::Instant::now(),
    )
    .with_fence(rig.fence.clone())
    .with_stalls(Arc::clone(&stalls));
    let users = LaneKey::new(index("users"), sink("primary"));
    let orders = LaneKey::new(index("orders"), sink("primary"));
    let change = rig.source.emit(20);
    rig.route(change);
    rig.applied_on(&users, change);

    let start = tokio::time::Instant::now();
    stalls.sample(start, &rig.progress, &*rig.source, Some(4_096));
    stalls.sample(
        start + std::time::Duration::from_secs(90),
        &rig.progress,
        &*rig.source,
        Some(4_096),
    );

    let json = serde_json::to_value(status.snapshot()).unwrap();
    let lanes = &json["sinks"]["primary"]["lanes"];
    assert_eq!(lanes["orders"]["stall_seconds"], 90.0, "{json}");
    assert_eq!(lanes["orders"]["wal_behind_bytes"], 4_096, "{json}");
    assert_eq!(lanes["users"]["stall_seconds"], 0.0, "{json}");
    assert!(lanes["users"].get("wal_behind_bytes").is_none(), "{json}");
    assert_eq!(
        json["pinned_by"],
        serde_json::json!({ "index": "orders", "sink": "primary" })
    );

    rig.applied_on(&orders, change);
    stalls.sample(
        start + std::time::Duration::from_secs(91),
        &rig.progress,
        &*rig.source,
        Some(0),
    );
    let json = serde_json::to_value(status.snapshot()).unwrap();
    assert_eq!(
        json["sinks"]["primary"]["lanes"]["orders"]["stall_seconds"],
        0.0
    );
    assert!(json["pinned_by"].is_null(), "{json}");
}

fn on_indexes(position: &str, level: FenceLevel, indexes: &[&str]) -> FenceRequest {
    FenceRequest {
        indexes: indexes.iter().map(|name| index(name)).collect(),
        ..request(position, level, Some("primary"))
    }
}

/// Two indexes on one sink: the `orders` lane never lands, the `users` lane
/// does. A fence on `users` passes; one on `orders`, on both, or on every
/// index waits for `orders`.
#[tokio::test(start_paused = true)]
async fn a_stalled_sibling_index_blocks_no_fence_on_the_healthy_index() {
    let rig = fence_over(&["orders", "users"], &["primary"]);
    let change = rig.source.emit(20);
    rig.route(change);
    let users = LaneKey::new(index("users"), sink("primary"));
    rig.applied_on(&users, change);
    rig.searchable_on(&users, change);

    for level in [FenceLevel::Applied, FenceLevel::Searchable] {
        assert_eq!(
            rig.fence.wait(&on_indexes("20", level, &["users"])).await,
            reached("20"),
            "{level:?}"
        );
    }
    for indexes in [&["orders"][..], &["orders", "users"], &[]] {
        assert_eq!(
            rig.fence
                .wait(&FenceRequest {
                    timeout: Duration::from_millis(100),
                    ..on_indexes("20", FenceLevel::Applied, indexes)
                })
                .await,
            timed_out("19"),
            "{indexes:?} waits for the stalled orders lane"
        );
    }
}

/// A fence on one index wakes when that index's lane lands, whatever its
/// sibling does.
#[tokio::test(start_paused = true)]
async fn an_index_fence_wakes_when_its_lane_lands() {
    let rig = fence_over(&["orders", "users"], &["primary"]);
    let change = rig.source.emit(20);
    rig.route(change);
    let waiting = tokio::spawn({
        let fence = rig.fence.clone();
        async move {
            fence
                .wait(&on_indexes("20", FenceLevel::Applied, &["orders"]))
                .await
        }
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    rig.applied_on(&LaneKey::new(index("users"), sink("primary")), change);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!waiting.is_finished(), "users landing doesn't pass orders");

    rig.applied_on(&LaneKey::new(index("orders"), sink("primary")), change);
    assert_eq!(waiting.await.unwrap(), reached("20"));
}

#[tokio::test]
async fn a_fence_on_an_index_with_no_lane_is_refused() {
    let rig = fence_over(&["users"], &["primary"]);
    assert_eq!(
        rig.fence
            .wait(&on_indexes("1", FenceLevel::Applied, &["users", "orders"]))
            .await,
        Err(FenceError::UnknownIndex("orders".to_owned()))
    );
}

#[test]
fn positions_report_each_lane() {
    let rig = fence_over(&["orders", "users"], &["primary"]);
    let first = rig.source.emit(20);
    let second = rig.source.emit(30);
    rig.route(first);
    rig.route(second);
    let users = LaneKey::new(index("users"), sink("primary"));
    rig.applied_on(&users, first);
    rig.applied_on(&users, second);

    let positions = rig.fence.positions();
    let primary = &positions.sinks[&sink("primary")];
    assert_eq!(primary.applied.as_deref(), Some("19"), "its slowest lane");
    assert_eq!(
        primary.indexes[&index("users")],
        LanePositions {
            applied: Some("30".to_owned()),
            searchable: Some("19".to_owned()),
            outstanding: 0,
        }
    );
    assert_eq!(
        primary.indexes[&index("orders")],
        LanePositions {
            applied: Some("19".to_owned()),
            searchable: Some("19".to_owned()),
            outstanding: 2,
        }
    );
    assert_eq!(
        rig.fence.outstanding(),
        [
            (LaneKey::new(index("orders"), sink("primary")), 2),
            (users, 0),
        ]
    );
}

/// The answer a wait gets when `lanes` can't vouch for it.
fn unserved(position: &str, lanes: &[(&str, &str, Unserved)]) -> Result<FenceOutcome, FenceError> {
    Ok(FenceOutcome::Unserved {
        position: position.to_owned(),
        lanes: lanes
            .iter()
            .map(|(i, s, why)| (LaneKey::new(index(i), sink(s)), *why))
            .collect(),
    })
}

/// A first seed: the lane applied the change, but reads see a filling index.
/// The wait holds; the seed passes it at once.
#[tokio::test(start_paused = true)]
async fn a_backfilling_index_is_vouched_for_once_seeded() {
    let rig = fence(&["primary"]);
    rig.observer
        .on_backfill_requested(&sink("primary"), &[index("users")]);
    let change = rig.source.emit(20);
    rig.route(change);
    rig.applied("primary", change);
    rig.searchable("primary", change);

    let waiting = tokio::spawn({
        let fence = rig.fence.clone();
        async move {
            fence
                .wait(&request("20", FenceLevel::Searchable, None))
                .await
        }
    });
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(
        !waiting.is_finished(),
        "applied, but the index is still filling"
    );
    rig.observer
        .on_index_seeded(&sink("primary"), &index("users"));
    assert_eq!(waiting.await.unwrap(), reached("20"));
}

#[tokio::test(start_paused = true)]
async fn a_wait_on_a_backfilling_index_answers_unserved_at_its_timeout() {
    let rig = fence(&["primary"]);
    rig.observer
        .on_backfill_requested(&sink("primary"), &[index("users")]);
    let change = rig.source.emit(20);
    rig.route(change);
    rig.applied("primary", change);

    let started = Instant::now();
    assert_eq!(
        rig.fence
            .wait(&request("20", FenceLevel::Applied, None))
            .await,
        unserved("20", &[("users", "primary", Unserved::Backfilling)]),
    );
    assert_eq!(started.elapsed(), Duration::from_secs(5));
}

/// A reindex of a serving index: the old generation keeps what the lane had
/// applied when the rebuild began, and gets nothing after.
#[tokio::test(start_paused = true)]
async fn a_reindex_vouches_for_what_the_old_generation_holds_and_no_more() {
    let rig = fence(&["primary"]);
    let before = rig.source.emit(10);
    rig.route(before);
    rig.applied("primary", before);
    rig.searchable("primary", before);
    rig.observer
        .on_index_rebuilding(&sink("primary"), &index("users"));
    rig.observer
        .on_backfill_requested(&sink("primary"), &[index("users")]);
    let after = rig.source.emit(20);
    rig.route(after);
    rig.applied("primary", after);
    rig.searchable("primary", after);

    for level in [FenceLevel::Applied, FenceLevel::Searchable] {
        assert_eq!(
            rig.fence.wait(&request("10", level, None)).await,
            reached("10"),
            "{level}: the serving generation holds the change"
        );
        assert_eq!(
            rig.fence.wait(&request("20", level, None)).await,
            unserved("20", &[("users", "primary", Unserved::Rebuilding)]),
            "{level}: the change went to the generation being built"
        );
    }
    rig.observer
        .on_index_seeded(&sink("primary"), &index("users"));
    assert_eq!(
        rig.fence
            .wait(&request("20", FenceLevel::Searchable, None))
            .await,
        reached("20")
    );
}

/// The old generation's searchable mark is its own: a reindex never vouches
/// for a search past what a refresh had covered.
#[tokio::test(start_paused = true)]
async fn a_reindex_keeps_the_searchable_mark_it_had() {
    let rig = fence(&["primary"]);
    let first = rig.source.emit(10);
    let second = rig.source.emit(20);
    rig.route(first);
    rig.route(second);
    rig.applied("primary", second);
    rig.searchable("primary", first);
    rig.observer
        .on_index_rebuilding(&sink("primary"), &index("users"));

    assert_eq!(
        rig.fence
            .wait(&request("20", FenceLevel::Applied, None))
            .await,
        reached("20")
    );
    let answer = rig
        .fence
        .wait(&request("20", FenceLevel::Searchable, None))
        .await;
    assert!(
        matches!(answer, Ok(FenceOutcome::Unserved { .. })),
        "{answer:?}"
    );
}

/// After a `Fresh` source the old generation misses changes no one saw:
/// nothing is vouched for until the rebuild is seeded.
#[tokio::test(start_paused = true)]
async fn a_rebuild_after_a_fresh_source_vouches_for_nothing() {
    let source = Arc::new(NumberedSource::default());
    let lane = lane("primary");
    let progress = Arc::new(Progress::new([lane.clone()]));
    let fence = Fence::new(
        Arc::clone(&source) as Arc<dyn ChangeCapture>,
        Arc::clone(&progress),
        [lane.clone()],
    );
    let observer = FenceObserver::new(fence.clone());
    observer.on_index_rebuilding(&sink("primary"), &index("users"));
    observer.on_backfill_requested(&sink("primary"), &[index("users")]);
    observer.on_sink_started(&sink("primary"));
    source.idle_to(40);

    assert_eq!(
        fence.wait(&request("30", FenceLevel::Applied, None)).await,
        unserved("40", &[("users", "primary", Unserved::Rebuilding)]),
        "a commit made while flusso was away is not in the old generation"
    );
    observer.on_index_seeded(&sink("primary"), &index("users"));
    assert_eq!(
        fence.wait(&request("30", FenceLevel::Applied, None)).await,
        reached("40")
    );
}

/// A sink that finds its generation gone recreates it empty and asks for a
/// snapshot, with no rebuild: nothing of the old one is left to vouch for.
#[tokio::test(start_paused = true)]
async fn a_recreated_index_vouches_for_nothing_it_had() {
    let rig = fence(&["primary"]);
    let before = rig.source.emit(10);
    rig.route(before);
    rig.applied("primary", before);
    rig.observer
        .on_backfill_requested(&sink("primary"), &[index("users")]);
    assert_eq!(
        rig.fence
            .wait(&request("10", FenceLevel::Applied, None))
            .await,
        unserved("10", &[("users", "primary", Unserved::Backfilling)]),
    );
}

/// Per-index fences: a wait naming only the healthy index passes while its
/// sibling rebuilds; the wait over every index doesn't.
#[tokio::test(start_paused = true)]
async fn a_wait_naming_only_healthy_indexes_passes_beside_a_rebuild() {
    let rig = fence_over(&["orders", "users"], &["primary"]);
    rig.observer
        .on_index_rebuilding(&sink("primary"), &index("orders"));
    rig.source.idle_to(40);

    assert_eq!(
        rig.fence
            .wait(&on_indexes("30", FenceLevel::Applied, &["users"]))
            .await,
        reached("40")
    );
    assert_eq!(
        rig.fence
            .wait(&request("30", FenceLevel::Applied, None))
            .await,
        unserved("40", &[("orders", "primary", Unserved::Rebuilding)]),
    );
}

/// A wait on a healthy sink never looks at a sibling sink's backfill; a wait
/// on every sink does.
#[tokio::test(start_paused = true)]
async fn a_backfilling_sibling_sink_blocks_only_a_wait_on_every_sink() {
    let rig = fence(&["audit", "primary"]);
    rig.observer
        .on_backfill_requested(&sink("audit"), &[index("users")]);
    rig.source.idle_to(40);

    assert_eq!(
        rig.fence
            .wait(&request("30", FenceLevel::Searchable, Some("primary")))
            .await,
        reached("40")
    );
    assert_eq!(
        rig.fence
            .wait(&request("30", FenceLevel::Searchable, None))
            .await,
        unserved("40", &[("users", "audit", Unserved::Backfilling)]),
    );
}

/// Before its sink engine staged, a lane vouches for nothing; staging serves
/// the lanes it didn't send to a snapshot.
#[tokio::test(start_paused = true)]
async fn a_lane_is_vouched_for_once_its_sink_staged_it() {
    let source = Arc::new(NumberedSource::default());
    let lanes = [
        lane("primary"),
        LaneKey::new(index("orders"), sink("primary")),
    ];
    let fence = Fence::new(
        Arc::clone(&source) as Arc<dyn ChangeCapture>,
        Arc::new(Progress::new(lanes.iter().cloned())),
        lanes.iter().cloned(),
    );
    let observer = FenceObserver::new(fence.clone());
    source.idle_to(40);
    assert_eq!(
        fence.wait(&request("30", FenceLevel::Applied, None)).await,
        unserved(
            "40",
            &[
                ("orders", "primary", Unserved::Starting),
                ("users", "primary", Unserved::Starting)
            ]
        ),
    );

    observer.on_backfill_requested(&sink("primary"), &[index("orders")]);
    observer.on_sink_started(&sink("primary"));
    assert_eq!(
        fence
            .wait(&on_indexes("30", FenceLevel::Applied, &["users"]))
            .await,
        reached("40")
    );
    assert_eq!(
        fence.wait(&request("30", FenceLevel::Applied, None)).await,
        unserved("40", &[("orders", "primary", Unserved::Backfilling)]),
    );
}

/// What `Fence::settled` may skip when a build was left behind (an ingest
/// engine that failed while stopping): the index's pending documents hold
/// both levels of the lane at their change, so the searchable and applied
/// answers agree although an item past them is not searchable yet. That
/// wait could never be answered anyway: making the item searchable leaves
/// the lane's searchable answer where it was, held by the build. So
/// `settled` cutting it short loses no answer a waiter could get.
#[tokio::test(start_paused = true)]
async fn a_build_left_behind_holds_both_levels_so_settled_cuts_off_no_answerable_wait() {
    let rig = fence(&["primary"]);
    let users = lane("primary");
    let held = rig.source.emit(10);
    let later = rig.source.emit(20);
    rig.progress.mark([(
        source::document::DocumentId {
            index: index("users"),
            key: source::RowKey(Vec::new()),
        },
        held,
    )]);
    rig.progress
        .take(&index("users"), usize::MAX, &|_: &source::RowKey| false)
        .unwrap();
    rig.route(later);
    rig.applied("primary", later);

    let waiting = tokio::spawn({
        let fence = rig.fence.clone();
        async move {
            fence
                .wait(&request("20", FenceLevel::Searchable, None))
                .await
        }
    });
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert_eq!(
        rig.progress.lane(&users, Level::Searchable),
        rig.progress.lane(&users, Level::Applied),
        "both held at the build's change"
    );
    tokio::time::timeout(Duration::from_millis(100), rig.fence.settled())
        .await
        .expect("settled returns");

    rig.searchable("primary", later);
    assert_eq!(
        rig.progress.lane(&users, Level::Searchable),
        Some(Landed::Before(held)),
        "the build still holds the searchable answer once the item is searchable"
    );
    assert_eq!(waiting.await.unwrap(), timed_out("9"));
}

/// The race behind one registration call: the sink lands the batch's item
/// before the ingest task registers it, and a sibling sink's lane is full,
/// so its item never registers. The registration that moves the healthy
/// lane's answer wakes its waiter, which passes without the full lane.
#[tokio::test(start_paused = true)]
async fn a_fence_on_a_healthy_sink_passes_while_its_index_build_waits_on_a_full_sibling() {
    let rig = fence(&["a", "b"]);
    let change = rig.source.emit(20);
    rig.progress.mark([(
        source::document::DocumentId {
            index: index("users"),
            key: source::RowKey(Vec::new()),
        },
        change,
    )]);
    let taken = rig
        .progress
        .take(&index("users"), usize::MAX, &|_: &source::RowKey| false)
        .unwrap();
    rig.progress.build_landed(&index("users"), taken.token);
    rig.progress.build_sending(&index("users"), taken.token);

    let waiting = tokio::spawn({
        let fence = rig.fence.clone();
        async move {
            fence
                .wait(&request("20", FenceLevel::Applied, Some("a")))
                .await
        }
    });
    tokio::time::sleep(Duration::from_millis(10)).await;
    let key = rig.progress.next_key(&index("users")).unwrap();
    rig.progress.applied(&lane("a"), key);
    rig.observer.on_position_applied(&lane("a"), change);
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert!(!waiting.is_finished(), "the item isn't registered yet");

    let step = rig.progress.close_step();
    rig.progress
        .published_build(&lane("a"), key, taken.oldest, change, taken.token, step);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), waiting)
            .await
            .expect("woken by the registration")
            .unwrap(),
        reached("20")
    );
    assert_eq!(
        rig.progress.lane(&lane("b"), Level::Applied),
        Some(Landed::Before(change)),
        "the full sibling still holds itself back"
    );
}

/// The health a detach gives a lane, as the sink engine reports it.
fn detached() -> engine::LaneHealth {
    engine::LaneHealth::Detached {
        since: Instant::now(),
        reason: "blocked: index write (api)".to_owned(),
        through: None,
    }
}

/// A detached lane (ADR 0012 stage 10, R5, O2): heard before it leaves the
/// slot's minimum, its fence freezes at what it had reached. A wait at or
/// before that passes; one past it is answered `detached` at once, not at
/// its timeout, whatever the progress answers for the lane since.
#[tokio::test(start_paused = true)]
async fn a_detached_lane_answers_at_once() {
    let rig = fence(&["primary"]);
    let before = rig.source.emit(10);
    rig.route(before);
    rig.applied("primary", before);
    rig.searchable("primary", before);
    rig.observer.on_lane_health(&lane("primary"), &detached());
    rig.progress.detach(&lane("primary"));
    rig.source.idle_to(30);

    for level in [FenceLevel::Applied, FenceLevel::Searchable] {
        assert_eq!(
            rig.fence.wait(&request("10", level, None)).await,
            reached("10"),
            "{level}: frozen at what it had reached"
        );
        let started = Instant::now();
        assert_eq!(
            rig.fence.wait(&request("20", level, None)).await,
            unserved("10", &[("users", "primary", Unserved::Detached)]),
            "{level}: past the detach"
        );
        assert_eq!(started.elapsed(), Duration::ZERO, "{level}: at once");
    }
}

/// A lane detached before this process started has nothing frozen: every
/// wait on it is answered `detached` at once, never "not following live
/// changes", and a wait naming no index fails with it (O14). A wait naming
/// another index passes.
#[tokio::test(start_paused = true)]
async fn a_lane_detached_before_the_start_answers_detached_at_once() {
    // Staging finds the record before the sink is started.
    let rig = fence_unstaged(&["users", "orders"], &["primary"]);
    rig.observer.on_lane_health(&lane("primary"), &detached());
    rig.progress.detach(&lane("primary"));
    rig.observer.on_sink_started(&sink("primary"));
    rig.source.close();
    assert_eq!(
        rig.fence
            .wait(&on_indexes("1", FenceLevel::Applied, &["users"]))
            .await,
        unserved("", &[("users", "primary", Unserved::Detached)]),
    );

    let rig = fence_unstaged(&["users", "orders"], &["primary"]);
    rig.observer.on_lane_health(&lane("primary"), &detached());
    rig.progress.detach(&lane("primary"));
    rig.observer.on_sink_started(&sink("primary"));
    let orders = LaneKey::new(index("orders"), sink("primary"));
    let change = rig.source.emit(20);
    rig.progress
        .published(&orders, stream::ItemKey::new(change.seq), change, change);
    rig.applied_on(&orders, change);
    assert!(matches!(
        rig.fence
            .wait(&request("20", FenceLevel::Applied, None))
            .await,
        Ok(FenceOutcome::Unserved { ref lanes, .. }) if lanes == &[(lane("primary"), Unserved::Detached)]
    ));
    assert_eq!(
        rig.fence
            .wait(&on_indexes("20", FenceLevel::Applied, &["orders"]))
            .await,
        reached("20")
    );
}

/// A backfill asked for a detached lane keeps its fence `detached`: reads
/// stay frozen until the lane is attached again (review m5).
#[tokio::test(start_paused = true)]
async fn a_backfill_asked_for_a_detached_lane_keeps_it_detached() {
    let rig = fence(&["primary"]);
    rig.observer.on_lane_health(&lane("primary"), &detached());
    rig.progress.detach(&lane("primary"));
    rig.observer
        .on_backfill_requested(&sink("primary"), &[index("users")]);
    assert!(matches!(
        rig.fence
            .wait(&request("5", FenceLevel::Applied, None))
            .await,
        Ok(FenceOutcome::Unserved { ref lanes, .. }) if lanes == &[(lane("primary"), Unserved::Detached)]
    ));
}

/// A lane that opens again after a detach (its attach, review of step 3,
/// m3) serves its writes from there: a position it reaches after the attach
/// is vouched for. Step 4 narrows this for a rebuild and a reattach.
#[tokio::test(start_paused = true)]
async fn a_lane_attached_again_serves_its_writes() {
    let rig = fence(&["primary"]);
    rig.observer.on_lane_health(&lane("primary"), &detached());
    rig.progress.detach(&lane("primary"));
    assert_eq!(
        rig.progress.attach(&lane("primary"), 1),
        engine::Attach::Attached
    );
    rig.observer
        .on_lane_health(&lane("primary"), &engine::LaneHealth::Open);
    let change = rig.source.emit(20);
    rig.route(change);
    rig.applied("primary", change);
    assert_eq!(
        rig.fence
            .wait(&request("20", FenceLevel::Applied, None))
            .await,
        reached("20")
    );
}

/// Review of step 3, S3, for a rebuild: from the rebuild's start to its
/// seed the fence serves `rebuilding`, keeping what it froze at the detach,
/// even once the lane is attached and open: a change dropped while detached
/// is never vouched for; at the seed reads see every write.
#[tokio::test(start_paused = true)]
async fn a_fence_on_a_rebuilding_lane_vouches_for_no_change_dropped_while_detached() {
    let rig = fence(&["primary"]);
    let before = rig.source.emit(10);
    rig.route(before);
    rig.applied("primary", before);
    rig.searchable("primary", before);
    rig.observer.on_lane_health(&lane("primary"), &detached());
    rig.progress.detach(&lane("primary"));
    let dropped = rig.source.emit(20);
    rig.observer
        .on_index_rebuilding(&sink("primary"), &index("users"));
    assert_eq!(
        rig.progress.attach(&lane("primary"), 1),
        engine::Attach::Attached
    );
    rig.observer
        .on_lane_health(&lane("primary"), &engine::LaneHealth::Open);
    rig.observer
        .on_backfill_requested(&sink("primary"), &[index("users")]);
    let after = rig.source.emit(30);
    rig.route(after);
    rig.applied("primary", after);
    let _ = dropped;
    assert_eq!(
        rig.fence
            .wait(&request("10", FenceLevel::Applied, None))
            .await,
        reached("10"),
        "what the serving generation held"
    );
    assert!(
        matches!(
            rig.fence
                .wait(&request("20", FenceLevel::Applied, None))
                .await,
            Ok(FenceOutcome::Unserved { ref lanes, .. }) if lanes == &[(lane("primary"), Unserved::Rebuilding)]
        ),
        "the change dropped while detached"
    );
    rig.observer
        .on_index_seeded(&sink("primary"), &index("users"));
    assert_eq!(
        rig.fence
            .wait(&request("30", FenceLevel::Applied, None))
            .await,
        reached("30")
    );
}

fn gap(from: Option<&str>, to: Option<&str>) -> sink::Gap {
    sink::Gap {
        from: from.map(str::to_owned),
        to: to.map(str::to_owned),
    }
}

fn in_gap(position: &str, from: &str, to: Option<&str>) -> Result<FenceOutcome, FenceError> {
    Ok(FenceOutcome::InGap {
        position: position.to_owned(),
        lanes: vec![(
            lane("primary"),
            FenceGap {
                from: Some(from.to_owned()),
                to: to.map(str::to_owned),
            },
        )],
    })
}

/// Review of step 3, S3, for `reattach --accept-gap`: the lane vouches as
/// usual from its attach on and as before up to its detach; a position
/// inside the gap between them is refused at once, naming the gap. After a
/// restart the gap is heard from the sink's own records, and refuses the
/// same positions.
#[tokio::test(start_paused = true)]
async fn a_reattached_lane_vouches_only_from_its_attach() {
    let rig = fence(&["primary"]);
    let before = rig.source.emit(10);
    rig.route(before);
    rig.applied("primary", before);
    rig.searchable("primary", before);
    rig.observer.on_lane_health(
        &lane("primary"),
        &engine::LaneHealth::Detached {
            since: Instant::now(),
            reason: "blocked".to_owned(),
            through: Some("10".to_owned()),
        },
    );
    rig.progress.detach(&lane("primary"));
    let dropped = rig.source.emit(20);
    rig.route(dropped);
    rig.progress.routed(dropped);
    assert_eq!(
        rig.progress.attach(&lane("primary"), 1),
        engine::Attach::Attached
    );
    // What the sink engine hears when it takes `Attached`: the gap ends where
    // the lane stands, before it opens.
    let to = rig
        .source
        .reached(rig.progress.lane(&lane("primary"), Level::Applied).unwrap())
        .unwrap();
    assert_eq!(to, SourcePosition(20));
    rig.observer
        .on_lane_gap(&lane("primary"), &gap(Some("10"), Some("20")));
    rig.observer
        .on_lane_health(&lane("primary"), &engine::LaneHealth::Open);
    let after = rig.source.emit(30);
    rig.route(after);
    rig.applied("primary", after);
    rig.searchable("primary", after);

    for level in [FenceLevel::Applied, FenceLevel::Searchable] {
        assert_eq!(
            rig.fence.wait(&request("10", level, None)).await,
            reached("30"),
            "{level}: before the detach"
        );
        let started = Instant::now();
        for inside in ["11", "20"] {
            assert_eq!(
                rig.fence.wait(&request(inside, level, None)).await,
                in_gap(inside, "10", Some("20")),
                "{level}: {inside} is inside the gap"
            );
        }
        assert_eq!(started.elapsed(), Duration::ZERO, "{level}: at once");
        for after in ["21", "30"] {
            assert_eq!(
                rig.fence.wait(&request(after, level, None)).await,
                reached("30"),
                "{level}: {after} is after the attach"
            );
        }
    }

    // A restart: staging hears the gap from the seed marker.
    let rig = fence_unstaged(&["users"], &["primary"]);
    rig.observer
        .on_lane_gap(&lane("primary"), &gap(Some("10"), Some("20")));
    rig.observer.on_sink_started(&sink("primary"));
    let change = rig.source.emit(40);
    rig.route(change);
    rig.applied("primary", change);
    assert_eq!(
        rig.fence
            .wait(&request("15", FenceLevel::Applied, None))
            .await,
        in_gap("15", "10", Some("20"))
    );
    assert_eq!(
        rig.fence
            .wait(&request("40", FenceLevel::Applied, None))
            .await,
        reached("40")
    );
}

/// A gap whose end is not known yet (the reattach died between deleting its
/// record and the attach) refuses every position past its start, until it
/// is heard completed. A seed (a reindex) ends a gap: the index holds every
/// change again.
#[tokio::test(start_paused = true)]
async fn an_open_gap_refuses_everything_past_its_start_and_a_seed_ends_it() {
    let rig = fence(&["primary"]);
    let change = rig.source.emit(50);
    rig.route(change);
    rig.applied("primary", change);
    rig.observer
        .on_lane_gap(&lane("primary"), &gap(Some("10"), None));
    assert_eq!(
        rig.fence
            .wait(&request("50", FenceLevel::Applied, None))
            .await,
        in_gap("50", "10", None)
    );
    assert_eq!(
        rig.fence
            .wait(&request("10", FenceLevel::Applied, None))
            .await,
        reached("50")
    );
    rig.observer
        .on_lane_gap(&lane("primary"), &gap(Some("10"), Some("40")));
    assert_eq!(
        rig.fence
            .wait(&request("50", FenceLevel::Applied, None))
            .await,
        reached("50")
    );
    rig.observer
        .on_index_seeded(&sink("primary"), &index("users"));
    assert_eq!(
        rig.fence
            .wait(&request("20", FenceLevel::Applied, None))
            .await,
        reached("50")
    );
}

/// S3: a detach freezes the fence where the sink recorded reads stood, the
/// start of a later reattach's gap, even when the lane reached further by
/// the time the detach was heard: what is refused after a reattach starts
/// exactly where vouching stopped.
#[tokio::test(start_paused = true)]
async fn a_detach_freezes_where_the_sink_recorded_it() {
    let rig = fence(&["primary"]);
    let change = rig.source.emit(10);
    rig.route(change);
    rig.applied("primary", change);
    rig.source.idle_to(15);
    rig.observer.on_lane_health(
        &lane("primary"),
        &engine::LaneHealth::Detached {
            since: Instant::now(),
            reason: "blocked".to_owned(),
            through: Some("12".to_owned()),
        },
    );
    rig.progress.detach(&lane("primary"));
    assert_eq!(
        rig.fence
            .wait(&request("12", FenceLevel::Applied, None))
            .await,
        reached("12")
    );
    assert_eq!(
        rig.fence
            .wait(&request("13", FenceLevel::Applied, None))
            .await,
        unserved("12", &[("users", "primary", Unserved::Detached)])
    );
}

/// O29: once the live stream opens, a sink that hasn't read its detach
/// records since vouches for nothing: what it read before the slot was held
/// is advisory. Its read lets the wait through.
#[tokio::test(start_paused = true)]
async fn a_sink_that_has_not_read_its_records_since_the_slot_opened_vouches_for_nothing() {
    let rig = fence(&["primary"]);
    let rereads = Arc::new(Rereads::default());
    let fence = rig.fence.clone().with_rereads(Arc::clone(&rereads));
    let change = rig.source.emit(20);
    rig.route(change);
    rig.applied("primary", change);
    assert_eq!(
        fence.wait(&request("20", FenceLevel::Applied, None)).await,
        reached("20"),
        "the slot never opened: nothing to read again"
    );
    rereads.live_opened();
    let after = rig.source.emit(30);
    rig.route(after);
    rig.applied("primary", after);
    assert_eq!(
        fence.wait(&request("30", FenceLevel::Applied, None)).await,
        unserved("30", &[("users", "primary", Unserved::Starting)])
    );
    rereads.read(&sink("primary"), rereads.live_starts());
    assert_eq!(
        fence.wait(&request("30", FenceLevel::Applied, None)).await,
        reached("30")
    );
}

/// Review of step 4, B1: a sink recording its read of the records wakes an
/// open wait, which passes at once instead of at its timeout.
#[tokio::test(start_paused = true)]
async fn a_read_of_the_records_during_a_wait_lets_it_through_at_once() {
    let rig = fence(&["primary"]);
    let rereads = Arc::new(Rereads::default());
    let fence = rig.fence.clone().with_rereads(Arc::clone(&rereads));
    let change = rig.source.emit(20);
    rig.route(change);
    rig.applied("primary", change);
    rereads.live_opened();
    let waiting = tokio::spawn({
        let fence = fence.clone();
        async move { fence.wait(&request("20", FenceLevel::Applied, None)).await }
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    let started = Instant::now();
    rereads.read(&sink("primary"), rereads.live_starts());
    assert_eq!(waiting.await.unwrap(), reached("20"));
    assert!(started.elapsed() < Duration::from_secs(1), "at once");
}

/// Review of step 4, B1: an unread sink masks only a lane serving its
/// writes. `Detached`, `Rebuilding` and `Backfilling` answer as themselves,
/// since they vouch only for what this process saw applied (or nothing),
/// which another process's detach can't take back.
#[tokio::test(start_paused = true)]
async fn an_unread_sink_still_answers_rebuilding_and_detached() {
    let unread = |rig: &Rig| {
        let rereads = Arc::new(Rereads::default());
        rereads.live_opened();
        rig.fence.clone().with_rereads(rereads)
    };

    let rig = fence(&["primary"]);
    let gate = unread(&rig);
    rig.observer
        .on_index_rebuilding(&sink("primary"), &index("users"));
    assert!(matches!(
        gate.wait(&request("5", FenceLevel::Applied, None)).await,
        Ok(FenceOutcome::Unserved { ref lanes, .. }) if lanes == &[(lane("primary"), Unserved::Rebuilding)]
    ));

    let rig = fence(&["primary"]);
    let gate = unread(&rig);
    rig.observer
        .on_backfill_requested(&sink("primary"), &[index("users")]);
    assert!(matches!(
        gate.wait(&request("5", FenceLevel::Applied, None)).await,
        Ok(FenceOutcome::Unserved { ref lanes, .. }) if lanes == &[(lane("primary"), Unserved::Backfilling)]
    ));

    let rig = fence(&["primary"]);
    let gate = unread(&rig);
    rig.observer.on_lane_health(&lane("primary"), &detached());
    rig.progress.detach(&lane("primary"));
    let started = Instant::now();
    assert!(matches!(
        gate.wait(&request("5", FenceLevel::Applied, None)).await,
        Ok(FenceOutcome::Unserved { ref lanes, .. }) if lanes == &[(lane("primary"), Unserved::Detached)]
    ));
    assert_eq!(
        started.elapsed(),
        Duration::ZERO,
        "detached answers at once"
    );
}

/// Review of step 4, B1: while its records are unread after the slot
/// opened again (a sink whose destination is down never reads), a lane
/// serving its writes still vouches for what this process saw it apply
/// before the opening: those writes are in the index, and no detach takes
/// them back. Past that it answers `starting` until the read.
#[tokio::test(start_paused = true)]
async fn an_unread_lane_keeps_vouching_for_what_it_applied_before_the_slot_reopened() {
    let rig = fence(&["primary"]);
    let rereads = Arc::new(Rereads::default());
    let fence = rig.fence.clone().with_rereads(Arc::clone(&rereads));
    rereads.live_opened();
    rereads.read(&sink("primary"), rereads.live_starts());
    let before = rig.source.emit(20);
    rig.route(before);
    rig.applied("primary", before);
    assert_eq!(
        fence.wait(&request("20", FenceLevel::Applied, None)).await,
        reached("20")
    );
    rereads.live_opened();
    let after = rig.source.emit(30);
    rig.route(after);
    rig.applied("primary", after);
    assert_eq!(
        fence.wait(&request("20", FenceLevel::Applied, None)).await,
        reached("20"),
        "applied before the slot reopened"
    );
    assert_eq!(
        fence.wait(&request("30", FenceLevel::Applied, None)).await,
        unserved("30", &[("users", "primary", Unserved::Starting)]),
        "past it, until the read"
    );
}

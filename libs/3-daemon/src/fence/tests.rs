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
    for on in sinks {
        observer.on_sink_started(&sink(on));
    }
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

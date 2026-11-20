//! `changes_committed` per sink (ADR 0012 stage 8a slice 3): a change counts
//! for a sink once every lane of that sink has landed it, or had nothing for
//! it. Read where a user reads it, `/status`, through a real daemon.

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use futures::stream::{self as fstream, BoxStream};
use kernel::{
    ColumnName, Envelope, GenericValue, IndexMapping, IndexName, Position, SinkName, TableName,
};
use sink::{RejectedDocument, Sink, SinkOptions, WriteReport};
use source::cdc::{ChangeCapture, ChangeEvent, Continuity, LiveChange};
use source::document::{BuildFailure, Document, DocumentBuilder, DocumentId};
use source::{RowImage, RowKey};
use stream::{AckHandle, Consumer, Delivery, Lane, LaneItem, LaneKey, RequestLane};
use stream_channel::ChannelStream;
use tokio::sync::{Notify, oneshot};

use super::{MockBackends, fast_restarts, primary, users, wait_until};
use crate::status::LaneHealthState;
use crate::{Daemon, StatusSnapshot};

fn orders() -> IndexName {
    IndexName::try_new("orders").unwrap()
}

fn audit() -> SinkName {
    SinkName::try_new("audit").unwrap()
}

/// A change to row `id` of `table`: `users` touches the `users` document,
/// `orders` the `orders` one, `both` both, `none` neither, `bad` a `users`
/// document that fails to build.
fn change(table: &str, id: i64) -> ChangeEvent {
    ChangeEvent::Upsert {
        table: TableName::try_new(table).unwrap(),
        key: RowKey(vec![(
            ColumnName::try_new("id").unwrap(),
            GenericValue::BigInt(id),
        )]),
        before: None,
        after: None,
    }
}

/// The two indexes, `users` and `orders`, one change per batch, quick
/// restarts.
fn two_indexes() -> config::Config {
    let mut config = fast_restarts();
    let users_entry = config.indexes.get(&users()).unwrap().clone();
    let mut orders_entry = users_entry;
    orders_entry.schema.table = TableName::try_new("orders").unwrap();
    config.indexes.insert(orders(), orders_entry);
    config.batch.max_changes = std::num::NonZeroUsize::new(1);
    config
}

/// Resolves a change by its table (see [`change`]).
#[derive(Debug)]
struct ByTable;

#[async_trait]
impl DocumentBuilder for ByTable {
    async fn resolve(
        &self,
        table: &TableName,
        key: &RowKey,
        _before: Option<&RowImage>,
        _after: Option<&RowImage>,
    ) -> source::Result<Vec<DocumentId>> {
        let indexes = match table.as_ref() {
            "users" | "bad" => vec![users()],
            "orders" => vec![orders()],
            "both" => vec![users(), orders()],
            _ => Vec::new(),
        };
        Ok(indexes
            .into_iter()
            .map(|index| DocumentId {
                index,
                key: if table.as_ref() == "bad" {
                    RowKey(vec![(
                        ColumnName::try_new("bad").unwrap(),
                        GenericValue::Bool(true),
                    )])
                } else {
                    key.clone()
                },
            })
            .collect())
    }

    async fn build(&self, id: &DocumentId) -> source::Result<Document> {
        if id
            .key
            .0
            .first()
            .is_some_and(|(column, _)| column.as_ref() == "bad")
        {
            return Ok(Document::Failed {
                id: id.clone(),
                failure: BuildFailure {
                    field: "label".to_owned(),
                    message: "boom".to_owned(),
                    position: None,
                },
            });
        }
        Ok(Document::Upsert {
            id: id.clone(),
            body: GenericValue::Map(BTreeMap::new()),
        })
    }

    fn backfill_indexes(&self) -> Vec<IndexName> {
        Vec::new()
    }

    async fn last_key(&self, _: &IndexName) -> source::Result<Option<RowKey>> {
        Ok(None)
    }

    async fn keys(
        &self,
        _: &IndexName,
        _: Option<&RowKey>,
        _: &RowKey,
        _: usize,
    ) -> source::Result<Vec<RowKey>> {
        Ok(Vec::new())
    }

    async fn index_mappings(&self) -> source::Result<Vec<IndexMapping>> {
        Ok([users(), orders()]
            .into_iter()
            .map(|index| IndexMapping {
                index,
                hash: kernel::ContentHash::new(1),
                fields: Vec::new(),
            })
            .collect())
    }
}

/// Holds every write touching an index in `held` until it is released;
/// rejects every document of an index in `refused`, reports one in
/// `blocked` blocked and one in `missing` without its target; counts writes.
#[derive(Debug, Default)]
struct Gate {
    held: Mutex<HashSet<String>>,
    released: Notify,
    refused: Mutex<HashSet<String>>,
    blocked: Mutex<HashSet<String>>,
    missing: Mutex<HashSet<String>>,
    writes: AtomicU64,
}

impl Gate {
    fn holding(index: &IndexName) -> Self {
        let gate = Self::default();
        gate.held.lock().unwrap().insert(index.as_ref().to_owned());
        gate
    }

    fn release(&self) {
        self.held.lock().unwrap().clear();
        self.released.notify_waiters();
    }
}

#[async_trait]
impl Sink for Gate {
    async fn write(&self, envelopes: &[Envelope], _: bool) -> sink::Result<WriteReport> {
        loop {
            let released = self.released.notified();
            let held = {
                let held = self.held.lock().unwrap();
                envelopes
                    .iter()
                    .any(|envelope| held.contains(envelope.index.as_ref()))
            };
            if !held {
                break;
            }
            released.await;
        }
        self.writes.fetch_add(1, Ordering::SeqCst);
        let refused = self.refused.lock().unwrap().clone();
        let mut report = WriteReport::clean().rejecting(
            envelopes
                .iter()
                .filter(|envelope| refused.contains(envelope.index.as_ref()))
                .map(|envelope| RejectedDocument {
                    index: envelope.index.as_ref().to_owned(),
                    id: envelope.id.clone(),
                    reason: "refused".to_owned(),
                }),
        );
        let indexes: HashSet<&IndexName> =
            envelopes.iter().map(|envelope| &envelope.index).collect();
        for index in indexes {
            if self.blocked.lock().unwrap().contains(index.as_ref()) {
                report = report.blocking(index.clone(), "blocked");
            }
            if self.missing.lock().unwrap().contains(index.as_ref()) {
                report = report.missing_target(index.clone(), "gone");
            }
        }
        Ok(report)
    }

    async fn is_seeded(&self, _: &IndexName) -> sink::Result<bool> {
        Ok(true)
    }
}

/// Replays `changes` from position 0 on every open; the first open fails
/// after them, the later ones stay open until `end` fires.
#[derive(Debug)]
struct Replaying {
    changes: Vec<ChangeEvent>,
    opened: AtomicU64,
    end: Mutex<Option<oneshot::Receiver<()>>>,
}

#[async_trait]
impl ChangeCapture for Replaying {
    async fn continuity(&self) -> source::Result<Continuity> {
        Ok(Continuity::Resumed)
    }

    async fn prepare(&self) -> source::Result<()> {
        Ok(())
    }

    async fn live(&self) -> source::Result<BoxStream<'static, source::Result<LiveChange>>> {
        let first = self.opened.fetch_add(1, Ordering::SeqCst) == 0;
        let changes: Vec<source::Result<LiveChange>> = self
            .changes
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, event)| Ok((Position::new(i as u64), event)))
            .collect();
        if first {
            let failing = changes
                .into_iter()
                .chain([Err(source::SourceError::Connection(
                    "simulated drop".to_owned(),
                ))]);
            return Ok(Box::pin(fstream::iter(failing.collect::<Vec<_>>())));
        }
        let end = self.end.lock().unwrap().take();
        let tail = fstream::unfold(end, |end| async move {
            if let Some(rx) = end {
                let _ = rx.await;
            }
            None
        });
        Ok(Box::pin(futures::StreamExt::chain(
            fstream::iter(changes),
            tail,
        )))
    }

    fn confirm(&self, _: source::cdc::Landed) {}
}

/// Scripted changes from position 0, then open until `end` fires.
fn scripted(changes: Vec<ChangeEvent>) -> (super::ScriptedSource, oneshot::Sender<()>) {
    super::ScriptedSource::new(changes).held_open()
}

fn backends_over(
    capture: Arc<dyn ChangeCapture>,
    sinks: Vec<(SinkName, Arc<dyn Sink>)>,
) -> Arc<MockBackends> {
    Arc::new(MockBackends {
        capture,
        documents: Arc::new(ByTable),
        sinks: sinks
            .into_iter()
            .map(|(name, sink)| (name, sink, SinkOptions::default()))
            .collect(),
        built: Arc::new(Mutex::new(Vec::new())),
    })
}

fn committed(snapshot: &StatusSnapshot, sink: &str) -> u64 {
    snapshot.sinks[sink].changes_committed
}

/// Long enough for every write that isn't held to land, on a quiet engine.
async fn settle() {
    tokio::time::sleep(Duration::from_millis(300)).await;
}

/// Table row 1: a change with documents for both lanes of a sink counts
/// once both items landed, not when the first lane's (`orders`) lands.
#[tokio::test]
async fn a_change_counts_once_every_lane_of_the_sink_landed_it() {
    let gate = Arc::new(Gate::holding(&users()));
    let (source, _end) = scripted(vec![change("both", 1)]);
    let backends = backends_over(
        Arc::new(source),
        vec![(primary(), Arc::clone(&gate) as Arc<dyn Sink>)],
    );
    let running = Daemon::new(two_indexes(), backends).start().await.unwrap();
    let status = running.status();
    let run = tokio::spawn(running.run(std::future::pending::<()>()));
    wait_until("the orders item to land", || {
        gate.writes.load(Ordering::SeqCst) >= 1
    })
    .await;
    settle().await;
    let snapshot = status.snapshot();
    assert_eq!(snapshot.changes_captured, 1);
    assert_eq!(
        committed(&snapshot, "primary"),
        0,
        "the users document isn't written yet"
    );

    gate.release();
    wait_until("the change to count", || {
        committed(&status.snapshot(), "primary") == 1
    })
    .await;
    assert_eq!(status.snapshot().sinks["primary"].changes_in_flight, 0);
    run.abort();
}

/// Table row 2: a change that builds nothing for a sink counts once the
/// sink has landed everything before it, not at publish.
#[tokio::test]
async fn a_change_with_nothing_for_a_sink_counts_after_its_earlier_changes() {
    let gate = Arc::new(Gate::holding(&users()));
    let (source, _end) = scripted(vec![change("users", 1), change("none", 2)]);
    let backends = backends_over(
        Arc::new(source),
        vec![(primary(), Arc::clone(&gate) as Arc<dyn Sink>)],
    );
    let running = Daemon::new(two_indexes(), backends).start().await.unwrap();
    let status = running.status();
    let run = tokio::spawn(running.run(std::future::pending::<()>()));
    wait_until("both changes read", || {
        status.snapshot().changes_captured == 2
    })
    .await;
    settle().await;
    assert_eq!(
        committed(&status.snapshot(), "primary"),
        0,
        "change 1 isn't written, so change 2 doesn't count either"
    );

    gate.release();
    wait_until("both changes to count", || {
        committed(&status.snapshot(), "primary") == 2
    })
    .await;
    run.abort();
}

/// Table row 3: a stalled lane holds its sink's count for later changes of
/// another index too, as confirmation is held.
#[tokio::test]
async fn a_stalled_lane_holds_its_sinks_count_for_other_indexes() {
    let gate = Arc::new(Gate::holding(&users()));
    let (source, _end) = scripted(vec![change("users", 1), change("orders", 2)]);
    let backends = backends_over(
        Arc::new(source),
        vec![(primary(), Arc::clone(&gate) as Arc<dyn Sink>)],
    );
    let running = Daemon::new(two_indexes(), backends).start().await.unwrap();
    let status = running.status();
    let run = tokio::spawn(running.run(std::future::pending::<()>()));
    wait_until("the orders item to land", || {
        gate.writes.load(Ordering::SeqCst) >= 1
    })
    .await;
    settle().await;
    let snapshot = status.snapshot();
    assert_eq!(snapshot.changes_captured, 2);
    assert_eq!(committed(&snapshot, "primary"), 0);
    assert_eq!(snapshot.sinks["primary"].changes_in_flight, 2);

    gate.release();
    wait_until("both changes to count", || {
        committed(&status.snapshot(), "primary") == 2
    })
    .await;
    run.abort();
}

/// A lane delivery whose first acknowledgement fails (the sink engine's run
/// ends with it, after its write landed), then acknowledges normally.
#[derive(Debug)]
struct FailingFirstAck {
    inner: ChannelStream,
    failed: Arc<AtomicBool>,
}

#[derive(Debug)]
struct FailingConsumer {
    inner: Box<dyn Consumer<LaneItem>>,
    failed: Arc<AtomicBool>,
}

#[derive(Debug)]
struct FailingHandle {
    inner: Box<dyn AckHandle>,
    failed: Arc<AtomicBool>,
}

#[async_trait]
impl AckHandle for FailingHandle {
    async fn ack(self: Box<Self>) -> stream::Result<()> {
        if !self.failed.swap(true, Ordering::SeqCst) {
            return Err(stream::QueueError::Ack("simulated".to_owned()));
        }
        self.inner.ack().await
    }

    async fn nack(self: Box<Self>) -> stream::Result<()> {
        self.inner.nack().await
    }
}

#[async_trait]
impl Consumer<LaneItem> for FailingConsumer {
    async fn recv(&mut self) -> stream::Result<Option<Delivery<LaneItem>>> {
        Ok(self.inner.recv().await?.map(|delivery| {
            let (item, handle) = delivery.into_parts();
            Delivery::new(
                item,
                Box::new(FailingHandle {
                    inner: handle,
                    failed: Arc::clone(&self.failed),
                }),
            )
        }))
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl stream::Stream for FailingFirstAck {
    fn lane(&self, key: &LaneKey) -> stream::Result<Lane> {
        let lane = self.inner.lane(key)?;
        Ok(Lane {
            producer: lane.producer,
            consumer: Box::new(FailingConsumer {
                inner: lane.consumer,
                failed: Arc::clone(&self.failed),
            }),
        })
    }

    fn lanes(&self, sink: &SinkName) -> Vec<LaneKey> {
        self.inner.lanes(sink)
    }

    fn requests(&self) -> stream::Result<RequestLane> {
        self.inner.requests()
    }

    fn is_idle(&self) -> bool {
        self.inner.is_idle()
    }
}

#[derive(Debug)]
struct FailingBackends(Arc<MockBackends>);

#[async_trait]
impl crate::Backends for FailingBackends {
    fn validate(&self, config: &config::Config) -> anyhow::Result<()> {
        self.0.validate(config)
    }

    async fn source(
        &self,
        config: Arc<config::Config>,
        options: &crate::DaemonOptions,
    ) -> anyhow::Result<crate::SourceParts> {
        self.0.source(config, options).await
    }

    fn stream(
        &self,
        _config: &config::Config,
        lanes: &[LaneKey],
    ) -> anyhow::Result<Arc<dyn stream::Stream>> {
        Ok(Arc::new(FailingFirstAck {
            inner: ChannelStream::new(64, lanes.iter().cloned()),
            failed: Arc::new(AtomicBool::new(false)),
        }))
    }

    async fn sinks(&self, config: &config::Config) -> anyhow::Result<Vec<crate::SinkParts>> {
        self.0.sinks(config).await
    }
}

/// Table row 4: an item whose write landed but whose acknowledgement failed
/// is redelivered to the restarted sink engine and written again; its
/// change counts once.
#[tokio::test]
async fn a_redelivered_item_that_had_landed_counts_once() {
    let gate = Arc::new(Gate::default());
    let (source, _end) = scripted(vec![change("users", 1)]);
    let backends = FailingBackends(backends_over(
        Arc::new(source),
        vec![(primary(), Arc::clone(&gate) as Arc<dyn Sink>)],
    ));
    let running = Daemon::new(two_indexes(), Arc::new(backends))
        .start()
        .await
        .unwrap();
    let status = running.status();
    let run = tokio::spawn(running.run(std::future::pending::<()>()));
    wait_until("the item to be written twice", || {
        gate.writes.load(Ordering::SeqCst) >= 2
    })
    .await;
    settle().await;
    let snapshot = status.snapshot();
    assert_eq!(snapshot.changes_captured, 1);
    assert_eq!(committed(&snapshot, "primary"), 1, "counted once");
    run.abort();
}

/// A source that replays changes under the positions they had: the first
/// live stream reads two changes and fails while their live run is still
/// open (three changes per batch), so they are never published; the engine
/// restarts and reads them again. Every read counts once per sink, on both
/// sinks, so each ends at what was read: the two reads of the failed engine
/// run count with their replay.
#[tokio::test]
async fn every_change_read_counts_once_per_sink_across_a_replay() {
    let (end, ends) = oneshot::channel();
    let source = Replaying {
        changes: vec![change("users", 1), change("orders", 2)],
        opened: AtomicU64::new(0),
        end: Mutex::new(Some(ends)),
    };
    let backends = backends_over(
        Arc::new(source),
        vec![
            (primary(), Arc::new(Gate::default()) as Arc<dyn Sink>),
            (audit(), Arc::new(Gate::default()) as Arc<dyn Sink>),
        ],
    );
    let mut config = two_indexes();
    config.batch.max_changes = std::num::NonZeroUsize::new(3);
    let running = Daemon::new(config, backends).start().await.unwrap();
    let status = running.status();
    let run = tokio::spawn(running.run(std::future::pending::<()>()));
    wait_until("the replay read", || {
        status.snapshot().changes_captured >= 4
    })
    .await;
    wait_until("every sink to count every read", || {
        let snapshot = status.snapshot();
        snapshot.changes_captured == committed(&snapshot, "primary")
            && snapshot.changes_captured == committed(&snapshot, "audit")
    })
    .await;
    settle().await;
    let snapshot = status.snapshot();
    assert_eq!(committed(&snapshot, "primary"), snapshot.changes_captured);
    assert_eq!(committed(&snapshot, "audit"), snapshot.changes_captured);
    assert_eq!(snapshot.changes_in_flight, 0);
    drop(end);
    run.abort();
}

/// Every document a change built failed under `on_error = "skip"`: no lane
/// gets an item, and the change still counts once the sink landed what came
/// before it.
#[tokio::test]
async fn a_change_whose_documents_were_all_skipped_still_counts() {
    let gate = Arc::new(Gate::default());
    let (source, _end) = scripted(vec![change("bad", 1), change("orders", 2)]);
    let backends = backends_over(
        Arc::new(source),
        vec![(primary(), Arc::clone(&gate) as Arc<dyn Sink>)],
    );
    let mut config = two_indexes();
    config.on_error = kernel::FailurePolicy::Skip;
    let running = Daemon::new(config, backends).start().await.unwrap();
    let status = running.status();
    let run = tokio::spawn(running.run(std::future::pending::<()>()));
    wait_until("both changes to count", || {
        let snapshot = status.snapshot();
        snapshot.changes_captured == 2 && committed(&snapshot, "primary") == 2
    })
    .await;
    run.abort();
}

/// A lane that isn't written (parked on a `stop` refusal, blocked, or
/// staged again because its target is gone) holds its sink's count, while a
/// sibling sink counts everything; the held sink's changes stay in flight.
/// Many grants pass it with documents for `orders` only, and every lane's
/// record stays within its bound. Once the lane is written again the sink
/// has every read.
async fn a_lane_not_written_holds_its_sinks_count(
    park: impl FnOnce(&Gate),
    unpark: impl FnOnce(&Gate),
    health: LaneHealthState,
) {
    const PASSING: i64 = 200;
    let parked = Arc::new(Gate::default());
    park(&parked);
    let healthy = Arc::new(Gate::default());
    let mut changes = vec![change("users", 0)];
    changes.extend((1..=PASSING).map(|id| change("orders", id)));
    let (source, _end) = scripted(changes);
    let backends = backends_over(
        Arc::new(source),
        vec![
            (primary(), Arc::clone(&parked) as Arc<dyn Sink>),
            (audit(), Arc::clone(&healthy) as Arc<dyn Sink>),
        ],
    );
    let running = Daemon::new(two_indexes(), backends).start().await.unwrap();
    let status = running.status();
    let progress = running.progress();
    let run = tokio::spawn(running.run(std::future::pending::<()>()));
    let total = PASSING as u64 + 1;
    wait_until("the healthy sink to count every change", || {
        committed(&status.snapshot(), "audit") == total
    })
    .await;
    wait_until("the users lane on primary to be held", || {
        status.snapshot().sinks["primary"].lanes["users"].health == health
    })
    .await;
    settle().await;
    let snapshot = status.snapshot();
    assert_eq!(
        committed(&snapshot, "primary"),
        0,
        "held by the {} lane",
        health.name()
    );
    assert_eq!(snapshot.sinks["primary"].changes_in_flight, total);
    assert_eq!(snapshot.changes_in_flight, total, "the slowest sink");
    for lane in [
        LaneKey::new(users(), primary()),
        LaneKey::new(orders(), primary()),
        LaneKey::new(users(), audit()),
        LaneKey::new(orders(), audit()),
    ] {
        let (kept, unlanded) = progress.steps_kept(&lane).unwrap();
        assert!(
            kept <= unlanded + 2,
            "{lane} keeps {kept} steps after {total} grants"
        );
    }

    unpark(&parked);
    wait_until("the held sink to count every change", || {
        committed(&status.snapshot(), "primary") == total
    })
    .await;
    assert_eq!(status.snapshot().changes_captured, total);
    run.abort();
}

#[tokio::test]
async fn a_refused_lane_holds_its_sinks_count_and_a_sibling_counts_on() {
    a_lane_not_written_holds_its_sinks_count(
        |gate| {
            gate.refused
                .lock()
                .unwrap()
                .insert(users().as_ref().to_owned());
        },
        |gate| gate.refused.lock().unwrap().clear(),
        LaneHealthState::Refused,
    )
    .await;
}

#[tokio::test]
async fn a_blocked_lane_holds_its_sinks_count_and_a_sibling_counts_on() {
    a_lane_not_written_holds_its_sinks_count(
        |gate| {
            gate.blocked
                .lock()
                .unwrap()
                .insert(users().as_ref().to_owned());
        },
        |gate| gate.blocked.lock().unwrap().clear(),
        LaneHealthState::Blocked,
    )
    .await;
}

#[tokio::test]
async fn a_restaging_lane_holds_its_sinks_count_and_a_sibling_counts_on() {
    a_lane_not_written_holds_its_sinks_count(
        |gate| {
            gate.missing
                .lock()
                .unwrap()
                .insert(users().as_ref().to_owned());
        },
        |gate| gate.missing.lock().unwrap().clear(),
        LaneHealthState::Restaging,
    )
    .await;
}

/// A graceful stop ends with every change counted on every sink, `skip`ped
/// and empty changes included.
#[tokio::test]
async fn a_graceful_stop_ends_with_nothing_in_flight_on_any_sink() {
    let (source, _end) = scripted(vec![
        change("both", 1),
        change("none", 2),
        change("users", 3),
        change("orders", 4),
    ]);
    let backends = backends_over(
        Arc::new(source),
        vec![
            (primary(), Arc::new(Gate::default()) as Arc<dyn Sink>),
            (audit(), Arc::new(Gate::default()) as Arc<dyn Sink>),
        ],
    );
    let running = Daemon::new(two_indexes(), backends).start().await.unwrap();
    let status = running.status();
    let (stop, stopped) = oneshot::channel::<()>();
    let run = tokio::spawn(running.run(async {
        let _ = stopped.await;
    }));
    wait_until("every change read", || {
        status.snapshot().changes_captured == 4
    })
    .await;
    stop.send(()).unwrap();
    run.await.unwrap().unwrap();
    let snapshot = status.snapshot();
    assert_eq!(snapshot.changes_in_flight, 0);
    for sink in ["primary", "audit"] {
        assert_eq!(committed(&snapshot, sink), 4, "{sink}");
        assert_eq!(snapshot.sinks[sink].changes_in_flight, 0, "{sink}");
    }
}

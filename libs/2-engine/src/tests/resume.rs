//! A sink engine records how far each lane's backfill durably got, and asks
//! for a resumed backfill on its next start (ADR 0012 stage 8b,
//! `docs/adr/0012-stage-notes/stage-8b-resume.md`):
//!
//! - the record is the landed prefix of the walk, never ahead of an item
//!   still out, written at most every ten seconds;
//! - a stop's skipped items and a failed write hold it back;
//! - staging asks for a backfill after the recorded mark, forgets it under a
//!   fresh source, and a reindex asks from the first key.
//!
//! Each test drives one engine over real channel lanes on paused time. The
//! randomized test replays with `FLUSSO_SCHEDULE_SEED=<seed>`.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use kernel::{ContentHash, Envelope, GenericValue, IndexMapping, IndexName};
use sink::{Seeding, Sink, WriteReport};
use source::cdc::Continuity;
use stream::{Batch, LaneItem, LaneKey, Producer, Request, Stream, WalkMark};
use stream_channel::ChannelStream;
use tokio::sync::Notify;

use super::sink_name;
use crate::{Progress, Result, SinkControl, SinkEngine, Stop};

fn users() -> IndexName {
    IndexName::try_new("users").unwrap()
}

/// A small deterministic generator, so a schedule replays from its seed.
#[derive(Debug)]
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
}

/// A sink that stores a checkpoint as the OpenSearch sink does, holds the
/// writes of the ids a test names, and checks every record against what it
/// has written: each key up to the mark must have landed before the mark is
/// recorded. Keys are zero-padded numbers, so their text order is key order.
#[derive(Debug, Default)]
struct Store {
    walked: Mutex<Option<String>>,
    seeded: Mutex<bool>,
    records: Mutex<Vec<String>>,
    forgotten: Mutex<usize>,
    written: Mutex<BTreeSet<String>>,
    /// Every key the walk published, so a record can be checked against it.
    published: Mutex<BTreeSet<String>>,
    held: Mutex<HashSet<String>>,
    released: Notify,
    fail_once: Mutex<HashSet<String>>,
    delays: Option<Mutex<Rng>>,
    ahead: Mutex<Vec<String>>,
}

impl Store {
    fn holding(self, ids: &[&str]) -> Self {
        self.held
            .lock()
            .unwrap()
            .extend(ids.iter().map(|id| (*id).to_owned()));
        self
    }

    fn release(&self, id: &str) {
        self.held.lock().unwrap().remove(id);
        self.released.notify_waiters();
    }

    fn records(&self) -> Vec<String> {
        self.records.lock().unwrap().clone()
    }
}

#[async_trait]
impl Sink for Store {
    async fn write(&self, envelopes: &[Envelope], _: bool) -> sink::Result<WriteReport> {
        let ids: Vec<String> = envelopes.iter().map(|e| e.id.clone()).collect();
        loop {
            let released = self.released.notified();
            if !ids.iter().any(|id| self.held.lock().unwrap().contains(id)) {
                break;
            }
            released.await;
        }
        let delay = self
            .delays
            .as_ref()
            .map(|rng| rng.lock().unwrap().below(20));
        if let Some(delay) = delay {
            tokio::time::sleep(Duration::from_millis(delay * 100)).await;
        }
        if ids
            .iter()
            .any(|id| self.fail_once.lock().unwrap().remove(id))
        {
            return Err(sink::SinkError::Write("failed once".to_owned()));
        }
        self.written.lock().unwrap().extend(ids);
        Ok(WriteReport::clean())
    }

    async fn seeding(&self, _: &IndexName) -> sink::Result<Seeding> {
        Ok(if *self.seeded.lock().unwrap() {
            Seeding::Seeded
        } else {
            Seeding::Unseeded {
                walked: self.walked.lock().unwrap().clone(),
            }
        })
    }

    async fn record_walked(&self, _: &IndexName, mark: &str) -> sink::Result<()> {
        let written = self.written.lock().unwrap().clone();
        let missing: Vec<String> = self
            .published
            .lock()
            .unwrap()
            .iter()
            .filter(|key| key.as_str() <= mark && !written.contains(*key))
            .cloned()
            .collect();
        if !missing.is_empty() {
            self.ahead
                .lock()
                .unwrap()
                .push(format!("{mark} recorded before {missing:?} landed"));
        }
        self.records.lock().unwrap().push(mark.to_owned());
        *self.walked.lock().unwrap() = Some(mark.to_owned());
        Ok(())
    }

    async fn forget_walked(&self, _: &IndexName) -> sink::Result<()> {
        *self.forgotten.lock().unwrap() += 1;
        *self.walked.lock().unwrap() = None;
        Ok(())
    }

    async fn mark_seeded(&self, _: &IndexName) -> sink::Result<()> {
        *self.seeded.lock().unwrap() = true;
        *self.walked.lock().unwrap() = None;
        Ok(())
    }

    async fn reindex(&self, _: &IndexMapping) -> sink::Result<()> {
        *self.seeded.lock().unwrap() = false;
        *self.walked.lock().unwrap() = None;
        Ok(())
    }
}

/// One sink engine over the `users` lane.
struct Rig {
    store: Arc<Store>,
    engine: Arc<SinkEngine>,
    stream: Arc<ChannelStream>,
    lane: Box<dyn Producer<LaneItem>>,
    control: tokio::sync::mpsc::Sender<SinkControl>,
    running: Option<tokio::task::JoinHandle<Result<()>>>,
    stop: Stop,
}

impl Rig {
    fn new(store: Store, writes: usize) -> Self {
        let name = sink_name("primary");
        let stream = Arc::new(ChannelStream::over(1024, [users()], [name.clone()]));
        let lane = stream
            .lane(&LaneKey::new(users(), name.clone()))
            .unwrap()
            .producer;
        let store = Arc::new(store);
        let stop = Stop::new();
        let engine = Arc::new(
            SinkEngine::new(
                name.clone(),
                Arc::clone(&store) as Arc<dyn Sink>,
                Arc::clone(&stream) as Arc<dyn Stream>,
                vec![IndexMapping {
                    index: users(),
                    hash: ContentHash::new(1),
                    fields: Vec::new(),
                }],
                Arc::new(Progress::of([users()], [name])),
            )
            .with_writes(writes)
            .with_write_documents(1)
            .with_stop(stop.clone()),
        );
        let (control, _) = tokio::sync::mpsc::channel(4);
        Self {
            store,
            engine,
            stream,
            lane,
            control,
            running: None,
            stop,
        }
    }

    fn start(&mut self) {
        let engine = Arc::clone(&self.engine);
        let (control, mut rx) = tokio::sync::mpsc::channel(4);
        self.control = control;
        self.running = Some(tokio::spawn(async move { engine.run(&mut rx).await }));
    }

    async fn ended(&mut self) -> Result<()> {
        self.running.take().unwrap().await.unwrap()
    }

    /// A backfill item of one key, its walk mark the key itself.
    async fn page(&self, key: &str) {
        self.store.published.lock().unwrap().insert(key.to_owned());
        self.lane
            .publish(LaneItem::Batch(Batch {
                position: None,
                changes: 0,
                envelopes: vec![Envelope::upsert(
                    users(),
                    key,
                    GenericValue::Map(BTreeMap::new()),
                    None,
                    chrono::Utc::now(),
                )]
                .into(),
                hold: Default::default(),
                walked: Some(WalkMark::new(key)),
            }))
            .await
            .unwrap();
    }

    async fn complete(&self) {
        self.lane
            .publish(LaneItem::SnapshotComplete { index: users() })
            .await
            .unwrap();
    }

    /// The backfill requests staging sent, with their marks.
    async fn requests(&self) -> Vec<Option<String>> {
        let mut consumer = self.stream.requests().unwrap().consumer;
        let mut seen = Vec::new();
        while !consumer.is_empty() {
            let delivery = consumer.recv().await.unwrap().unwrap();
            let Request::Backfill { after, .. } = delivery.item().clone();
            seen.push(after.map(|mark| mark.as_str().to_owned()));
            delivery.into_parts().1.ack().await.unwrap();
        }
        seen
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        if let Some(running) = self.running.take() {
            running.abort();
        }
    }
}

/// Wait (in paused time) until `condition` holds, failing after ten minutes.
async fn until(what: &str, mut condition: impl FnMut() -> bool) {
    for _ in 0..600_000 {
        if condition() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    panic!("timed out waiting for {what}");
}

async fn settle() {
    for _ in 0..200 {
        tokio::task::yield_now().await;
    }
}

/// k02's write is held: after ten seconds the record is k01, not the keys
/// after k02 that landed. Once k02 lands, the next record covers them all.
#[tokio::test(start_paused = true)]
async fn a_record_is_the_landed_prefix_never_past_an_item_still_out() {
    let mut rig = Rig::new(Store::default().holding(&["k02"]), 4);
    rig.start();
    for key in ["k01", "k02", "k03", "k04"] {
        rig.page(key).await;
    }
    until("k04", || rig.store.written.lock().unwrap().contains("k04")).await;
    tokio::time::sleep(Duration::from_secs(11)).await;
    rig.page("k05").await;
    until("a record", || !rig.store.records().is_empty()).await;
    assert_eq!(rig.store.records(), ["k01"]);

    rig.store.release("k02");
    until("k02", || rig.store.written.lock().unwrap().contains("k02")).await;
    tokio::time::sleep(Duration::from_secs(11)).await;
    rig.page("k06").await;
    until("a second record", || rig.store.records().len() == 2).await;
    assert_eq!(rig.store.records(), ["k01", "k06"]);
    assert!(rig.store.ahead.lock().unwrap().is_empty());
}

/// A backfill shorter than ten seconds records nothing: its
/// `SnapshotComplete` seeds it.
#[tokio::test(start_paused = true)]
async fn a_short_backfill_records_nothing() {
    let mut rig = Rig::new(Store::default(), 4);
    rig.start();
    for key in ["k01", "k02", "k03"] {
        rig.page(key).await;
    }
    rig.complete().await;
    until("the seed", || *rig.store.seeded.lock().unwrap()).await;
    assert!(rig.store.records().is_empty());
}

/// A failed write holds the mark back: what lands after it in the same run
/// is never recorded.
#[tokio::test(start_paused = true)]
async fn a_failed_write_holds_the_mark_back() {
    let store = Store::default();
    store.fail_once.lock().unwrap().insert("k02".to_owned());
    let mut rig = Rig::new(store.holding(&["k02"]), 4);
    rig.start();
    for key in ["k01", "k02", "k03"] {
        rig.page(key).await;
    }
    until("k03", || rig.store.written.lock().unwrap().contains("k03")).await;
    tokio::time::sleep(Duration::from_secs(11)).await;
    rig.page("k04").await;
    until("a record", || !rig.store.records().is_empty()).await;
    assert_eq!(rig.store.records(), ["k01"]);
    rig.store.release("k02");
    assert!(rig.ended().await.is_err());
    assert!(
        rig.store.records().iter().all(|mark| mark.as_str() < "k02"),
        "{:?}",
        rig.store.records()
    );
}

/// A stop acknowledges the waiting backfill unwritten: nothing past it is
/// recorded.
#[tokio::test(start_paused = true)]
async fn a_stop_records_nothing_past_the_skipped_items() {
    let mut rig = Rig::new(Store::default().holding(&["k03"]), 1);
    rig.start();
    for key in ["k01", "k02", "k03", "k04", "k05"] {
        rig.page(key).await;
    }
    until("k03's write", || {
        rig.store.written.lock().unwrap().contains("k02")
    })
    .await;
    rig.stop.request();
    settle().await;
    tokio::time::sleep(Duration::from_secs(11)).await;
    rig.store.release("k03");
    until("the lane to drain", || rig.stream.is_idle()).await;
    assert!(
        rig.store.records().iter().all(|mark| mark.as_str() < "k04"),
        "{:?}",
        rig.store.records()
    );
    assert!(rig.store.ahead.lock().unwrap().is_empty());
}

/// Staging asks for a backfill after the recorded mark.
#[tokio::test(start_paused = true)]
async fn the_request_carries_the_recorded_mark() {
    let store = Store::default();
    *store.walked.lock().unwrap() = Some("k07".to_owned());
    let rig = Rig::new(store, 4);
    rig.engine.stage(Continuity::Resumed).await.unwrap();
    assert_eq!(rig.requests().await, [Some("k07".to_owned())]);
    assert_eq!(*rig.store.forgotten.lock().unwrap(), 0);
}

/// A fresh source lost the changes since the mark: staging forgets it before
/// asking, and asks from the first key.
#[tokio::test(start_paused = true)]
async fn a_fresh_source_forgets_the_mark_before_requesting() {
    let store = Store::default();
    *store.walked.lock().unwrap() = Some("k07".to_owned());
    let rig = Rig::new(store, 4);
    rig.engine.stage(Continuity::Fresh).await.unwrap();
    assert_eq!(rig.requests().await, [None]);
    assert_eq!(*rig.store.forgotten.lock().unwrap(), 1);
    assert_eq!(*rig.store.walked.lock().unwrap(), None);
}

/// A reindex asks for a backfill from the first key.
#[tokio::test(start_paused = true)]
async fn a_reindex_requests_from_the_first_key() {
    let store = Store::default();
    *store.seeded.lock().unwrap() = true;
    let mut rig = Rig::new(store, 4);
    rig.engine.stage(Continuity::Resumed).await.unwrap();
    assert!(rig.requests().await.is_empty());
    rig.start();
    rig.control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    let mut seen = Vec::new();
    for _ in 0..10_000 {
        seen = rig.requests().await;
        if !seen.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    assert_eq!(seen, [None]);
}

fn seeds() -> Vec<u64> {
    match std::env::var("FLUSSO_SCHEDULE_SEED") {
        Ok(seed) => vec![seed.parse().expect("FLUSSO_SCHEDULE_SEED is a number")],
        Err(_) => (1..=50).collect(),
    }
}

/// Writes take random times with four at once, so items land out of order:
/// no record is ever ahead of what landed, and records happen.
#[test]
fn random_landing_never_records_a_mark_ahead_of_what_landed() {
    let mut recorded = 0;
    for seed in seeds() {
        let runtime = super::paused_runtime();
        recorded += runtime.block_on(async {
            let store = Store {
                delays: Some(Mutex::new(Rng(seed.max(1)))),
                ..Store::default()
            };
            let mut rig = Rig::new(store, 4);
            rig.start();
            for key in 0..300 {
                rig.page(&format!("k{key:04}")).await;
            }
            rig.complete().await;
            until("the seed", || *rig.store.seeded.lock().unwrap()).await;
            let ahead = rig.store.ahead.lock().unwrap().clone();
            assert!(ahead.is_empty(), "seed {seed}: {ahead:?}");
            rig.store.records().len()
        });
    }
    assert!(recorded > 0, "no schedule recorded anything");
}

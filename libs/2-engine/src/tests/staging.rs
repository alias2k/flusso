//! Staging a sink with many indexes (issue #398): the indexes are staged
//! concurrently, at most [`STAGE_CONCURRENCY`] at a time, each index's own
//! steps in order; one backfill request names every unseeded index; a failure
//! names its index, the indexes in flight finish, and no new one is started.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use kernel::{ContentHash, Envelope, IndexMapping, IndexName};
use sink::{Sink, SinkError, SinkOptions, WriteReport};
use source::cdc::Continuity;
use stream::{Request, Stream};
use stream_channel::ChannelStream;

use super::sink_name;
use crate::sink_engine::STAGE_CONCURRENCY;
use crate::{EngineError, SinkControl, SinkEngine};

const INDEXES: usize = 24;

// A bound of 1 would pass every `peak == STAGE_CONCURRENCY` check below.
const _: () = assert!(STAGE_CONCURRENCY > 1);

/// How long each staging call of [`SlowSink`] takes.
const CALL: Duration = Duration::from_millis(10);

fn name(i: usize) -> String {
    format!("index_{i:02}")
}

fn mappings(n: usize) -> Vec<IndexMapping> {
    (0..n)
        .map(|i| IndexMapping {
            index: IndexName::try_new(name(i)).unwrap(),
            hash: ContentHash::new(1),
            fields: Vec::new(),
        })
        .collect()
}

/// A sink whose staging calls each take [`CALL`]. Records every call as
/// `"<call> <index>"`, how many were in flight at once, and whether two calls
/// for one index ever overlapped.
#[derive(Debug, Default)]
struct SlowSink {
    seeded: Mutex<BTreeSet<String>>,
    failing: BTreeSet<String>,
    calls: Mutex<Vec<String>>,
    in_flight: AtomicUsize,
    peak: AtomicUsize,
    busy: Mutex<BTreeSet<String>>,
    one_index_overlapped: AtomicBool,
}

impl SlowSink {
    fn seeded(indexes: impl IntoIterator<Item = String>) -> Self {
        Self {
            seeded: Mutex::new(indexes.into_iter().collect()),
            ..Self::default()
        }
    }

    fn failing(mut self, indexes: impl IntoIterator<Item = String>) -> Self {
        self.failing = indexes.into_iter().collect();
        self
    }

    async fn call(&self, call: &str, index: &str) {
        let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
        if !self.busy.lock().unwrap().insert(index.to_owned()) {
            self.one_index_overlapped.store(true, Ordering::SeqCst);
        }
        self.calls.lock().unwrap().push(format!("{call} {index}"));
        tokio::time::sleep(CALL).await;
        self.busy.lock().unwrap().remove(index);
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    fn calls_for(&self, index: &str) -> Vec<String> {
        self.calls()
            .into_iter()
            .filter_map(|call| {
                let (what, of) = call.split_once(' ')?;
                (of == index).then(|| what.to_owned())
            })
            .collect()
    }
}

#[async_trait]
impl Sink for SlowSink {
    async fn ensure_index(&self, mapping: &IndexMapping) -> sink::Result<sink::Ensured> {
        let index = mapping.index.as_ref();
        self.call("ensure", index).await;
        if self.failing.contains(index) {
            return Err(SinkError::Write(format!("{index} refused")));
        }
        Ok(sink::Ensured::default())
    }

    async fn write(&self, _: &[Envelope], _: bool) -> sink::Result<WriteReport> {
        Ok(WriteReport::clean())
    }

    async fn is_seeded(&self, index: &IndexName) -> sink::Result<bool> {
        self.call("seeded?", index.as_ref()).await;
        Ok(self.seeded.lock().unwrap().contains(index.as_ref()))
    }

    async fn reindex(&self, mapping: &IndexMapping) -> sink::Result<()> {
        let index = mapping.index.as_ref();
        self.call("reindex", index).await;
        self.seeded.lock().unwrap().remove(index);
        Ok(())
    }
}

struct Staged {
    sink: Arc<SlowSink>,
    stream: Arc<ChannelStream>,
    engine: SinkEngine,
}

fn staged(sink: SlowSink, indexes: usize) -> Staged {
    let sink = Arc::new(sink);
    let mappings = mappings(indexes);
    let names: Vec<IndexName> = mappings.iter().map(|m| m.index.clone()).collect();
    let stream = Arc::new(ChannelStream::over(
        64,
        names.clone(),
        vec![sink_name("primary")],
    ));
    let engine = SinkEngine::new(
        sink_name("primary"),
        Arc::clone(&sink) as Arc<dyn Sink>,
        Arc::clone(&stream) as Arc<dyn Stream>,
        mappings,
        Arc::new(crate::Progress::of(names, [sink_name("primary")])),
    );
    Staged {
        sink,
        stream,
        engine,
    }
}

impl Staged {
    async fn requests(&self) -> Vec<Vec<String>> {
        requests(&self.stream).await
    }
}

/// Every backfill request published so far, as the sorted index names.
async fn requests(stream: &ChannelStream) -> Vec<Vec<String>> {
    {
        let mut consumer = stream.requests().unwrap().consumer;
        let mut requests: Vec<String> = Vec::new();
        while let Ok(Ok(Some(delivery))) =
            tokio::time::timeout(Duration::from_millis(1), consumer.recv()).await
        {
            let (request, handle) = delivery.into_parts();
            handle.ack().await.unwrap();
            let Request::Backfill { index, .. } = request;
            requests.push(index.as_ref().to_owned());
        }
        // One staging's requests, one per index, read together.
        requests.sort();
        if requests.is_empty() {
            Vec::new()
        } else {
            vec![requests]
        }
    }
}

#[tokio::test(start_paused = true)]
async fn a_sinks_indexes_are_staged_concurrently_up_to_the_bound() {
    let staged = staged(SlowSink::default(), INDEXES);
    let started = tokio::time::Instant::now();
    staged.engine.stage(Continuity::Resumed).await.unwrap();

    assert_eq!(
        staged.sink.peak.load(Ordering::SeqCst),
        STAGE_CONCURRENCY,
        "as many indexes in flight as the bound allows, never more"
    );
    // Two calls per index (ensure, is_seeded), 24 indexes, 8 at a time.
    let rounds = (INDEXES * 2).div_ceil(STAGE_CONCURRENCY);
    assert!(
        started.elapsed() <= CALL * u32::try_from(rounds).unwrap(),
        "took {:?}",
        started.elapsed()
    );
}

#[tokio::test(start_paused = true)]
async fn one_indexs_steps_never_overlap_and_keep_their_order() {
    let stale: Vec<String> = (0..INDEXES).step_by(2).map(name).collect();
    let staged = staged(SlowSink::seeded(stale.clone()), INDEXES);
    staged.engine.stage(Continuity::Fresh).await.unwrap();

    assert!(!staged.sink.one_index_overlapped.load(Ordering::SeqCst));
    for i in 0..INDEXES {
        let expected: &[&str] = if stale.contains(&name(i)) {
            &["ensure", "seeded?", "reindex", "ensure", "seeded?"]
        } else {
            &["ensure", "seeded?"]
        };
        assert_eq!(staged.sink.calls_for(&name(i)), expected, "{}", name(i));
    }
    assert!(
        staged.sink.peak.load(Ordering::SeqCst) > 1,
        "the rebuilds overlap too"
    );
}

#[tokio::test(start_paused = true)]
async fn one_backfill_request_names_every_unseeded_index() {
    let seeded: Vec<String> = (0..INDEXES).filter(|i| i % 3 == 0).map(name).collect();
    let staged = staged(SlowSink::seeded(seeded.clone()), INDEXES);
    staged.engine.stage(Continuity::Resumed).await.unwrap();

    let unseeded: Vec<String> = (0..INDEXES)
        .map(name)
        .filter(|n| !seeded.contains(n))
        .collect();
    assert_eq!(staged.requests().await, vec![unseeded]);
}

#[tokio::test(start_paused = true)]
async fn a_fresh_source_requests_the_rebuilt_and_the_unseeded_indexes_once() {
    let stale: Vec<String> = (0..INDEXES).filter(|i| i % 4 == 0).map(name).collect();
    let staged = staged(SlowSink::seeded(stale), INDEXES);
    staged.engine.stage(Continuity::Fresh).await.unwrap();

    assert_eq!(
        staged.requests().await,
        vec![(0..INDEXES).map(name).collect::<Vec<_>>()]
    );
}

#[tokio::test(start_paused = true)]
async fn every_failure_seen_is_named_and_the_chains_in_flight_finish() {
    let failing = [name(3), name(5)];
    let staged = staged(SlowSink::default().failing(failing.clone()), INDEXES);
    let error = staged.engine.stage(Continuity::Resumed).await.unwrap_err();

    let EngineError::Staging(failures) = &error else {
        panic!("expected a staging error, got {error:?}");
    };
    let named: Vec<&str> = failures.iter().map(|(index, _)| index.as_ref()).collect();
    assert_eq!(named, [failing[0].as_str(), failing[1].as_str()]);
    let message = error.to_string();
    for index in &failing {
        assert!(
            message.contains(&format!("`{index}`")),
            "{message:?} names {index}"
        );
        assert!(message.contains(&format!("{index} refused")), "{message:?}");
    }
    for i in 0..STAGE_CONCURRENCY {
        assert!(
            staged.sink.calls().contains(&format!("ensure {}", name(i))),
            "{} was in flight and finished",
            name(i)
        );
        if !failing.contains(&name(i)) {
            assert!(
                staged
                    .sink
                    .calls()
                    .contains(&format!("seeded? {}", name(i))),
                "{} ran its chain to the end",
                name(i)
            );
        }
    }
    assert!(
        staged.requests().await.is_empty(),
        "a failed stage requests nothing; it is retried whole"
    );
}

/// A cluster that takes connections and never answers: every call fails after
/// its timeout. The stage gives up after one wave, not one per eight indexes.
#[tokio::test(start_paused = true)]
async fn after_a_failure_no_new_index_is_started() {
    let staged = staged(SlowSink::default().failing((0..INDEXES).map(name)), INDEXES);
    let started = tokio::time::Instant::now();
    let error = staged.engine.stage(Continuity::Resumed).await.unwrap_err();

    assert!(
        started.elapsed() < CALL * 2,
        "took {:?}, more than one failure's duration",
        started.elapsed()
    );
    let EngineError::Staging(failures) = &error else {
        panic!("expected a staging error, got {error:?}");
    };
    let named: Vec<String> = failures
        .iter()
        .map(|(index, _)| index.to_string())
        .collect();
    let in_flight: Vec<String> = (0..STAGE_CONCURRENCY).map(name).collect();
    assert_eq!(named, in_flight, "only the indexes started are reported");
    for i in STAGE_CONCURRENCY..INDEXES {
        assert!(
            staged.sink.calls_for(&name(i)).is_empty(),
            "{} was never started",
            name(i)
        );
    }
}

#[tokio::test(start_paused = true)]
async fn a_stateless_sink_stages_its_indexes_concurrently_and_asks_nothing() {
    let staged = staged(SlowSink::default(), INDEXES);
    let engine = staged.engine.with_options(SinkOptions { backfill: false });
    engine.stage(Continuity::Fresh).await.unwrap();

    assert_eq!(staged.sink.peak.load(Ordering::SeqCst), STAGE_CONCURRENCY);
    assert!(
        staged
            .sink
            .calls()
            .iter()
            .all(|call| call.starts_with("ensure "))
    );
}

#[tokio::test(start_paused = true)]
async fn a_reindex_control_stages_its_indexes_concurrently() {
    let all: Vec<String> = (0..INDEXES).map(name).collect();
    let staged = staged(SlowSink::seeded(all.clone()), INDEXES);
    staged.engine.stage(Continuity::Resumed).await.unwrap();
    assert!(staged.requests().await.is_empty());

    let (control, mut receiver) = tokio::sync::mpsc::channel(1);
    control
        .send(SinkControl::Reindex {
            indexes: mappings(INDEXES).into_iter().map(|m| m.index).collect(),
        })
        .await
        .unwrap();
    drop(control);
    let Staged {
        sink,
        stream,
        engine,
    } = staged;
    let staged = (sink, stream);
    let run = tokio::spawn(async move { engine.run(&mut receiver).await });
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while staged.0.calls_for(&name(INDEXES - 1)).len() < 4 && tokio::time::Instant::now() < deadline
    {
        tokio::time::sleep(CALL).await;
    }
    tokio::time::sleep(CALL).await;
    run.abort();

    assert_eq!(staged.0.peak.load(Ordering::SeqCst), STAGE_CONCURRENCY);
    for index in &all {
        assert_eq!(
            staged.0.calls_for(index),
            ["ensure", "seeded?", "reindex", "ensure"]
        );
    }
    assert_eq!(requests(&staged.1).await, vec![all]);
}

#[tokio::test(start_paused = true)]
async fn a_reindex_naming_an_index_twice_stages_it_once() {
    let all: Vec<String> = (0..2).map(name).collect();
    let staged = staged(SlowSink::seeded(all.clone()), 2);
    staged.engine.stage(Continuity::Resumed).await.unwrap();

    let first = mappings(1).remove(0).index;
    let second = mappings(2).remove(1).index;
    let (control, mut receiver) = tokio::sync::mpsc::channel(1);
    control
        .send(SinkControl::Reindex {
            indexes: vec![first.clone(), second, first],
        })
        .await
        .unwrap();
    drop(control);
    let Staged {
        sink,
        stream,
        engine,
    } = staged;
    let run = tokio::spawn(async move { engine.run(&mut receiver).await });
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while sink.calls_for(&name(1)).len() < 4 && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(CALL).await;
    }
    tokio::time::sleep(CALL * 4).await;
    run.abort();

    assert!(!sink.one_index_overlapped.load(Ordering::SeqCst));
    for index in &all {
        assert_eq!(
            sink.calls_for(index),
            ["ensure", "seeded?", "reindex", "ensure"]
        );
    }
    assert_eq!(requests(&stream).await, vec![all]);
}

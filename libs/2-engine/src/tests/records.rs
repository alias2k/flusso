//! Reading the detach records at staging (ADR 0012 stage 10, slice 1): the
//! sink's [`Sink::detachment`] is read at the top of every staging step,
//! before anything else touches the index. A record found is logged and kept
//! for its lane, and the index is staged as before (O24). A read that fails
//! fails that index's step alone under `Resumed` (#471) and the staging under
//! `Fresh`. With no record, the sink is called as it was before, plus the one
//! read.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use kernel::{ContentHash, Envelope, IndexMapping, IndexName};
use sink::{DetachRecord, Detachment, Sink, SinkError, WriteReport};
use source::cdc::Continuity;
use stream::{LaneKey, Request, Stream};
use stream_channel::ChannelStream;

use super::sink_name;
use crate::{SinkControl, SinkEngine, Stop};

fn name(i: usize) -> String {
    format!("index_{i:02}")
}

fn index(i: usize) -> IndexName {
    IndexName::try_new(name(i)).unwrap()
}

fn mappings(n: usize) -> Vec<IndexMapping> {
    (0..n)
        .map(|i| IndexMapping {
            index: index(i),
            hash: ContentHash::new(1),
            fields: Vec::new(),
        })
        .collect()
}

/// Every staging call of a sink, as `"<call> <index>"`, in order.
#[derive(Debug, Default)]
struct Calls(Mutex<Vec<String>>);

impl Calls {
    fn push(&self, call: &str, index: &str) {
        self.0.lock().unwrap().push(format!("{call} {index}"));
    }

    fn all(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }

    fn of(&self, index: &str) -> Vec<String> {
        self.all()
            .into_iter()
            .filter_map(|call| {
                let (what, of) = call.split_once(' ')?;
                (of == index).then(|| what.to_owned())
            })
            .collect()
    }
}

/// A sink that keeps nothing of its own about detaching (the port's default
/// `detachment`), recording every staging call: the calls `main` made.
#[derive(Debug, Default)]
struct Plain {
    seeded: BTreeSet<String>,
    calls: Calls,
}

#[async_trait]
impl Sink for Plain {
    async fn ensure_index(&self, mapping: &IndexMapping) -> sink::Result<sink::Ensured> {
        self.calls.push("ensure", mapping.index.as_ref());
        Ok(sink::Ensured::default())
    }

    async fn write(&self, _: &[Envelope], _: bool) -> sink::Result<WriteReport> {
        Ok(WriteReport::clean())
    }

    async fn is_seeded(&self, index: &IndexName) -> sink::Result<bool> {
        self.calls.push("seeded?", index.as_ref());
        Ok(self.seeded.contains(index.as_ref()))
    }

    async fn reindex(&self, mapping: &IndexMapping) -> sink::Result<()> {
        self.calls.push("reindex", mapping.index.as_ref());
        Ok(())
    }
}

/// What [`Kept`] answers for one index's detachment read.
#[derive(Debug, Clone)]
enum Read {
    Kept(Detachment),
    Fails,
}

/// [`Plain`], keeping detach records: each index's read answers what
/// `reads` says (nothing kept when not named), recorded as `"detachment"`.
#[derive(Debug, Default)]
struct Kept {
    plain: Plain,
    reads: Mutex<BTreeMap<String, Read>>,
}

#[async_trait]
impl Sink for Kept {
    async fn detachment(&self, mapping: &IndexMapping) -> sink::Result<Detachment> {
        let index = mapping.index.as_ref();
        self.plain.calls.push("detachment", index);
        let read = self.reads.lock().unwrap().get(index).cloned();
        match read {
            None => Ok(Detachment::default()),
            Some(Read::Kept(kept)) => Ok(kept.clone()),
            Some(Read::Fails) => Err(SinkError::Write(format!(
                "{index}: the meta index answered 503"
            ))),
        }
    }

    async fn ensure_index(&self, mapping: &IndexMapping) -> sink::Result<sink::Ensured> {
        self.plain.ensure_index(mapping).await
    }

    async fn write(&self, envelopes: &[Envelope], caught_up: bool) -> sink::Result<WriteReport> {
        self.plain.write(envelopes, caught_up).await
    }

    async fn is_seeded(&self, index: &IndexName) -> sink::Result<bool> {
        self.plain.is_seeded(index).await
    }

    async fn reindex(&self, mapping: &IndexMapping) -> sink::Result<()> {
        self.plain.reindex(mapping).await
    }
}

fn engine(sink: Arc<dyn Sink>, n: usize) -> (SinkEngine, Arc<ChannelStream>) {
    let mappings = mappings(n);
    let names: Vec<IndexName> = mappings.iter().map(|m| m.index.clone()).collect();
    let stream = Arc::new(ChannelStream::over(
        64,
        names.clone(),
        vec![sink_name("primary")],
    ));
    let engine = SinkEngine::new(
        sink_name("primary"),
        sink,
        Arc::clone(&stream) as Arc<dyn Stream>,
        mappings,
        Arc::new(crate::Progress::of(names, [sink_name("primary")])),
    );
    (engine, stream)
}

/// The backfill requests published so far, as `(index, after)`, sorted.
async fn requests(stream: &ChannelStream) -> Vec<(String, Option<String>)> {
    let mut consumer = stream.requests().unwrap().consumer;
    let mut requests = Vec::new();
    while let Ok(Ok(Some(delivery))) =
        tokio::time::timeout(Duration::from_millis(1), consumer.recv()).await
    {
        let (request, handle) = delivery.into_parts();
        handle.ack().await.unwrap();
        let Request::Backfill { index, after, .. } = request else {
            panic!("a backfill request")
        };
        requests.push((
            index.as_ref().to_owned(),
            after.map(|after| after.as_str().to_owned()),
        ));
    }
    requests.sort();
    requests
}

fn lane(i: usize) -> LaneKey {
    LaneKey::new(index(i), sink_name("primary"))
}

fn detached(live_only: bool) -> Detachment {
    let mut kept = Detachment::default();
    kept.record = Some(DetachRecord::Detached { live_only });
    kept
}

/// The read comes first in every index's steps: before `ensure_index` at a
/// start, and before `reindex` in a rebuild after a `Fresh` source.
#[tokio::test(start_paused = true)]
async fn the_records_are_read_before_anything_else_touches_the_index() {
    for continuity in [Continuity::Resumed, Continuity::Fresh] {
        let sink = Arc::new(Kept {
            plain: Plain {
                seeded: [name(0)].into(),
                ..Plain::default()
            },
            ..Kept::default()
        });
        let (engine, _stream) = engine(Arc::clone(&sink) as Arc<dyn Sink>, 2);
        engine.stage(continuity).await.unwrap();
        let rebuilt: &[&str] = if continuity == Continuity::Fresh {
            &[
                "detachment",
                "ensure",
                "seeded?",
                "detachment",
                "reindex",
                "ensure",
                "seeded?",
            ]
        } else {
            &["detachment", "ensure", "seeded?"]
        };
        assert_eq!(sink.plain.calls.of(&name(0)), rebuilt, "{continuity:?}");
        assert_eq!(
            sink.plain.calls.of(&name(1)),
            ["detachment", "ensure", "seeded?"],
            "{continuity:?}"
        );
    }
}

/// A reindex is a staging step too: its read comes before `reindex`.
#[tokio::test(start_paused = true)]
async fn a_reindex_reads_the_records_before_it_reindexes() {
    let sink = Arc::new(Kept {
        plain: Plain {
            seeded: [name(0)].into(),
            ..Plain::default()
        },
        ..Kept::default()
    });
    let (engine, _stream) = engine(Arc::clone(&sink) as Arc<dyn Sink>, 1);
    engine.stage(Continuity::Resumed).await.unwrap();
    let (control, mut receiver) = tokio::sync::mpsc::channel(1);
    control
        .send(SinkControl::Reindex {
            indexes: vec![index(0)],
        })
        .await
        .unwrap();
    drop(control);
    let run = tokio::spawn(async move { engine.run(&mut receiver).await });
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while sink.plain.calls.of(&name(0)).len() < 6 && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    run.abort();
    assert_eq!(
        sink.plain.calls.of(&name(0)),
        [
            "detachment",
            "ensure",
            "seeded?",
            "detachment",
            "reindex",
            "ensure"
        ]
    );
}

/// Under `Resumed` a read that fails leaves its index restaging alone, with
/// why, and touches nothing else of it; the other indexes are staged. The
/// failure is never taken for "nothing kept" (H6).
#[tokio::test(start_paused = true)]
async fn a_failed_read_leaves_its_index_restaging_alone_under_resumed() {
    let sink = Arc::new(Kept {
        reads: Mutex::new([(name(1), Read::Fails)].into()),
        ..Kept::default()
    });
    let (engine, stream) = engine(Arc::clone(&sink) as Arc<dyn Sink>, 3);
    engine.stage(Continuity::Resumed).await.unwrap();

    assert_eq!(sink.plain.calls.of(&name(1)), ["detachment"]);
    let health = engine.lane_health(&lane(1));
    assert!(
        matches!(&health, crate::LaneHealth::Restaging { reason, .. } if reason.contains("answered 503")),
        "{health:?}"
    );
    for i in [0, 2] {
        assert_eq!(engine.lane_health(&lane(i)).kind(), "open");
    }
    assert_eq!(
        requests(&stream).await,
        [(name(0), None), (name(2), None)],
        "the others are staged and asked for their backfill"
    );
}

/// Under `Fresh` a read that fails fails the staging, as every other failed
/// step does: the source must not be prepared before every index is staged
/// (#120).
#[tokio::test(start_paused = true)]
async fn a_failed_read_fails_a_fresh_staging() {
    let sink = Arc::new(Kept {
        reads: Mutex::new([(name(1), Read::Fails)].into()),
        ..Kept::default()
    });
    let (engine, _stream) = engine(Arc::clone(&sink) as Arc<dyn Sink>, 3);
    let error = engine.stage(Continuity::Fresh).await.unwrap_err();
    assert!(error.to_string().contains("answered 503"), "{error}");
}

/// A record left by a newer flusso, or one this version can't read, is kept
/// for its lane and the index is staged as before (O24): ensured, asked
/// whether seeded, its backfill asked for when unseeded.
#[tokio::test(start_paused = true)]
async fn a_record_found_is_kept_for_its_lane_and_staged_as_before() {
    let mut rebuilding = Detachment::default();
    rebuilding.record = Some(DetachRecord::Rebuilding { live_only: false });
    let mut unreadable = Detachment::default();
    unreadable.record = Some(DetachRecord::Unreadable {
        reason: "state `frozen`".to_owned(),
    });
    let reads: BTreeMap<String, Read> = [
        (name(0), Read::Kept(detached(false))),
        (name(1), Read::Kept(detached(true))),
        (name(2), Read::Kept(rebuilding.clone())),
        (name(3), Read::Kept(unreadable.clone())),
    ]
    .into();
    let sink = Arc::new(Kept {
        plain: Plain {
            seeded: [name(1)].into(),
            ..Plain::default()
        },
        reads: Mutex::new(reads),
    });
    let (engine, stream) = engine(Arc::clone(&sink) as Arc<dyn Sink>, 5);
    engine.stage(Continuity::Resumed).await.unwrap();

    for i in 0..5 {
        assert_eq!(
            sink.plain.calls.of(&name(i)),
            ["detachment", "ensure", "seeded?"],
            "{} is staged as before",
            name(i)
        );
        assert_eq!(engine.lane_health(&lane(i)).kind(), "open");
    }
    assert_eq!(engine.detachment(&index(0)), Some(detached(false)));
    assert_eq!(engine.detachment(&index(1)), Some(detached(true)));
    assert_eq!(engine.detachment(&index(2)), Some(rebuilding));
    assert_eq!(engine.detachment(&index(3)), Some(unreadable));
    assert_eq!(engine.detachment(&index(4)), None, "nothing kept");
    assert_eq!(
        requests(&stream).await,
        [
            (name(0), None),
            (name(2), None),
            (name(3), None),
            (name(4), None)
        ],
        "every unseeded index is asked for, the seeded one is not"
    );
}

/// Each staging keeps what it read last: a record gone by the next staging
/// (a restart of the sink engine's run stages again) is forgotten.
#[tokio::test(start_paused = true)]
async fn each_staging_keeps_what_it_read_last() {
    let sink = Arc::new(Kept {
        reads: Mutex::new([(name(0), Read::Kept(detached(false)))].into()),
        ..Kept::default()
    });
    let (engine, _stream) = engine(Arc::clone(&sink) as Arc<dyn Sink>, 1);
    engine.stage(Continuity::Resumed).await.unwrap();
    assert_eq!(engine.detachment(&index(0)), Some(detached(false)));

    sink.reads.lock().unwrap().clear();
    engine.stage(Continuity::Resumed).await.unwrap();
    assert_eq!(engine.detachment(&index(0)), None);
}

/// With nothing kept, staging calls the sink exactly as a sink keeping no
/// records (the port's default `detachment`, which makes no call) is called,
/// plus the one read first: the same calls in the same order, and the same
/// backfill requests, for a start under either source. Both sinks run on this
/// code; the engine change is the read at the top of `step` alone.
#[tokio::test(start_paused = true)]
async fn with_nothing_kept_the_sink_is_called_as_before_plus_the_read() {
    for continuity in [Continuity::Resumed, Continuity::Fresh] {
        let seeded: BTreeSet<String> = [name(0), name(2)].into();
        let plain = Arc::new(Plain {
            seeded: seeded.clone(),
            ..Plain::default()
        });
        let kept = Arc::new(Kept {
            plain: Plain {
                seeded,
                ..Plain::default()
            },
            ..Kept::default()
        });
        let (before, before_stream) = engine(Arc::clone(&plain) as Arc<dyn Sink>, 4);
        let (after, after_stream) = engine(Arc::clone(&kept) as Arc<dyn Sink>, 4);
        before.stage(continuity).await.unwrap();
        after.stage(continuity).await.unwrap();
        for i in 0..4 {
            let read: Vec<String> = kept
                .plain
                .calls
                .of(&name(i))
                .into_iter()
                .filter(|call| call != "detachment")
                .collect();
            assert_eq!(read, plain.calls.of(&name(i)), "{continuity:?} {}", name(i));
            assert_eq!(
                kept.plain.calls.of(&name(i)).first().map(String::as_str),
                Some("detachment"),
                "{continuity:?}: the read comes first"
            );
        }
        assert_eq!(
            requests(&after_stream).await,
            requests(&before_stream).await,
            "{continuity:?}"
        );
        for i in 0..4 {
            assert_eq!(after.detachment(&index(i)), None);
        }
    }
}

/// A lane staged again after its target went missing reads first: a read
/// that fails fails the restage before anything else touches the index, and
/// the lane's restage path keeps it restaging on its schedule.
#[tokio::test(start_paused = true)]
async fn a_failed_read_fails_a_restage_before_it_ensures() {
    let sink = Arc::new(Kept::default());
    let (engine, _stream) = engine(Arc::clone(&sink) as Arc<dyn Sink>, 1);
    engine.stage(Continuity::Resumed).await.unwrap();
    sink.reads.lock().unwrap().insert(name(0), Read::Fails);

    let error = engine.restage(&index(0)).await.unwrap_err();
    assert!(error.to_string().contains("answered 503"), "{error}");
    assert_eq!(
        sink.plain.calls.of(&name(0)),
        ["detachment", "ensure", "seeded?", "detachment"],
        "nothing after the failed read"
    );
}

/// An index a staging left out is ensured again through the same step: a
/// read that fails fails it before it ensures.
#[tokio::test(start_paused = true)]
async fn a_failed_read_fails_an_ensure_again_before_it_ensures() {
    let sink = Arc::new(Kept::default());
    let (engine, _stream) = engine(Arc::clone(&sink) as Arc<dyn Sink>, 1);
    engine.stage(Continuity::Resumed).await.unwrap();
    sink.reads.lock().unwrap().insert(name(0), Read::Fails);

    let error = engine.ensure_again(&index(0)).await.unwrap_err();
    assert!(error.to_string().contains("answered 503"), "{error}");
    assert_eq!(
        sink.plain.calls.of(&name(0)),
        ["detachment", "ensure", "seeded?", "detachment"]
    );
}

/// A reindex retried after its staging failed (X15) reads first too: a read
/// that fails leaves it owed, with why, and nothing reindexed.
#[tokio::test(start_paused = true)]
async fn a_failed_read_leaves_a_reindex_retry_owed() {
    let sink = Arc::new(Kept {
        plain: Plain {
            seeded: [name(0)].into(),
            ..Plain::default()
        },
        ..Kept::default()
    });
    let (engine, _stream) = engine(Arc::clone(&sink) as Arc<dyn Sink>, 1);
    engine.stage(Continuity::Resumed).await.unwrap();
    sink.reads.lock().unwrap().insert(name(0), Read::Fails);

    let retried = engine.reindex_again(&index(0)).await;
    assert!(retried.staged.is_empty());
    let [(failed, why)] = retried.failed.as_slice() else {
        panic!("one failure: {:?}", retried.failed)
    };
    assert_eq!(failed, &index(0));
    assert!(
        why.starts_with("the reindex could not stage") && why.contains("answered 503"),
        "{why}"
    );
    assert_eq!(
        sink.plain.calls.of(&name(0)),
        ["detachment", "ensure", "seeded?", "detachment"],
        "nothing reindexed"
    );
}

/// A staging during the stop reads first as well: a read that fails leaves
/// its index restaging, with why, for the stop to release, and the other
/// indexes are ensured.
#[tokio::test(start_paused = true)]
async fn a_failed_read_during_the_stop_leaves_its_index_for_the_stop() {
    let sink = Arc::new(Kept {
        reads: Mutex::new([(name(1), Read::Fails)].into()),
        ..Kept::default()
    });
    let (engine, _stream) = engine(Arc::clone(&sink) as Arc<dyn Sink>, 2);
    let stop = Stop::new();
    stop.request();
    let engine = engine.with_stop(stop);
    engine.stage(Continuity::Resumed).await.unwrap();

    assert_eq!(sink.plain.calls.of(&name(0)), ["detachment", "ensure"]);
    assert_eq!(sink.plain.calls.of(&name(1)), ["detachment"]);
    let health = engine.lane_health(&lane(1));
    assert!(
        matches!(&health, crate::LaneHealth::Restaging { reason, .. }
            if reason.contains("during the stop") && reason.contains("answered 503")),
        "{health:?}"
    );
}

/// A seed marker the sink couldn't read nor put back is kept like a record
/// and staged as before (O24).
#[tokio::test(start_paused = true)]
async fn an_unreadable_marker_is_kept_and_staged_as_before() {
    let mut unreadable = Detachment::default();
    unreadable.unreadable_marker = Some("detaching_from names no generation".to_owned());
    let sink = Arc::new(Kept {
        reads: Mutex::new([(name(0), Read::Kept(unreadable.clone()))].into()),
        ..Kept::default()
    });
    let (engine, stream) = engine(Arc::clone(&sink) as Arc<dyn Sink>, 1);
    engine.stage(Continuity::Resumed).await.unwrap();
    assert_eq!(
        sink.plain.calls.of(&name(0)),
        ["detachment", "ensure", "seeded?"]
    );
    assert_eq!(engine.detachment(&index(0)), Some(unreadable));
    assert_eq!(requests(&stream).await, [(name(0), None)]);
}

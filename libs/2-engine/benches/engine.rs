//! In-process benchmark of the two engines over the real channel stream, with
//! the source, builder, and sink mocked — no Docker, no I/O.
//!
//! This isolates the engines' own cost: batching, resolve/dedup, the one
//! `build_many` per batch, publishing an `Arc<[Envelope]>` to every lane, the
//! sink engine's recv → write → ack loop, and the channel's ticketed
//! in-flight ledger. Every mock answers at once, so a movement here is a
//! movement in the engines, not in Postgres or OpenSearch.
//!
//! Measured:
//!
//! - `live_drain_held`: a burst of `CHANGES` live changes drained end to end
//!   at several `max_changes`, with one sink and with two (the fan-out cost).
//!   Each sink holds its write of batch *n* until the ingest engine has
//!   started building batch *n + 1* (or has ended), so the sink always holds
//!   the batch's last reference and frees it, as every real sink does. The
//!   hold is an order, not a timing margin: the ingest engine drops its copy
//!   at the end of the batch's commit, before it builds the next one.
//!
//!   The release lands on the first line of build *n + 1*, so the free
//!   overlaps the next build on every batch: the contended upper bound, not
//!   production's ratio (a real build mostly waits on Postgres). With an
//!   instant sink, which side freed the batch was a race of about a
//!   microsecond, worth about 2.4× locally and about 3× on CI's runner, so
//!   the old `live_drain` measured that coin flip.
//!
//!   What it still catches: allocation volume, and the ingest engine's work
//!   per batch. What it can't: sink-side cost smaller than that work, and
//!   gains from larger batches (`/1`, `/64` and `/256` sit within about 15%).
//! - `snapshot`: a backfill of `ROWS` rows through the same path.
//!
//! Run with:
//!
//! ```text
//! cargo bench -p flusso-engine --bench engine
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    unused_crate_dependencies,
    missing_debug_implementations
)]

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use engine::{BatchPolicy, IngestEngine, SinkControl, SinkEngine};
use futures::stream::{self as fstream, BoxStream};
use kernel::{
    ColumnName, ContentHash, Envelope, FieldName, GenericValue, IndexMapping, IndexName, Mapping,
    MappingType, Position, ResolvedField, SinkName, TableName,
};
use sink::{Sink, WriteReport};
use source::cdc::{ChangeCapture, ChangeEvent, Continuity, LiveChange};
use source::document::{Document, DocumentBuilder, DocumentId};
use source::{Result as SourceResult, RowImage, RowKey};
use stream::Stream;
use stream_channel::ChannelStream;
use tokio::runtime::Runtime;

/// Live changes per `live_drain_held` iteration.
const CHANGES: usize = 10_000;
/// Rows per `snapshot` iteration.
const ROWS: usize = 10_000;
/// Distinct documents the changes spread over; a repeat within a batch is
/// deduplicated, as in production.
const DOCUMENTS: usize = 2_000;

fn index() -> IndexName {
    IndexName::try_new("users").unwrap()
}

fn table() -> TableName {
    TableName::try_new("users").unwrap()
}

fn key(id: i64) -> RowKey {
    RowKey(vec![(
        ColumnName::try_new("id").unwrap(),
        GenericValue::BigInt(id),
    )])
}

/// A source whose live stream is a fixed burst of upserts; `rows` is how many
/// root keys the builder pages for a backfill. `confirm` is a no-op.
#[derive(Debug)]
struct MockCapture {
    live: usize,
    rows: usize,
}

#[async_trait]
impl ChangeCapture for MockCapture {
    async fn continuity(&self) -> SourceResult<Continuity> {
        Ok(Continuity::Resumed)
    }

    async fn prepare(&self) -> SourceResult<()> {
        Ok(())
    }

    async fn live(&self) -> SourceResult<BoxStream<'static, SourceResult<LiveChange>>> {
        let n = self.live;
        Ok(Box::pin(fstream::iter((0..n).map(|i| {
            Ok((
                Position(i as u64),
                ChangeEvent::Upsert {
                    table: table(),
                    key: key((i % DOCUMENTS) as i64),
                    before: None,
                    after: None,
                },
            ))
        }))))
    }

    fn confirm(&self, _position: Position) {}
}

/// Resolves every change to one document and builds a realistic mid-size body
/// (a dozen scalars, an object, a nested array) with no I/O. Counts its
/// batches for the sinks' [`Pace`].
#[derive(Debug)]
struct MockDocuments {
    pace: Arc<Pace>,
    /// Root keys a backfill pages: `0..rows`.
    rows: usize,
}

/// How far the ingest engine has got: batches it started building, and
/// whether it ended. A sink holds its write of batch *n* until batch *n + 1*
/// is being built, after the ingest engine dropped its copy of batch *n*.
#[derive(Debug, Default)]
struct Pace {
    builds: AtomicUsize,
    ingest_ended: std::sync::atomic::AtomicBool,
    moved: tokio::sync::Notify,
}

impl Pace {
    fn build_started(&self) {
        self.builds.fetch_add(1, Ordering::SeqCst);
        self.moved.notify_waiters();
    }

    fn ingest_ended(&self) {
        self.ingest_ended.store(true, Ordering::SeqCst);
        self.moved.notify_waiters();
    }

    /// Wait until the ingest engine is past batch `batch` (counted from 1).
    async fn past(&self, batch: usize) {
        loop {
            let moved = self.moved.notified();
            tokio::pin!(moved);
            moved.as_mut().enable();
            if self.builds.load(Ordering::SeqCst) > batch
                || self.ingest_ended.load(Ordering::SeqCst)
            {
                return;
            }
            moved.await;
        }
    }
}

fn body(id: i64) -> GenericValue {
    let mut orders = Vec::new();
    for n in 0..5i64 {
        let mut order = BTreeMap::new();
        order.insert("id".to_owned(), GenericValue::BigInt(id * 1000 + n));
        order.insert("status".to_owned(), GenericValue::String("paid".into()));
        order.insert("total".to_owned(), GenericValue::Double(42.5 * n as f64));
        orders.push(GenericValue::Map(order));
    }
    let mut account = BTreeMap::new();
    account.insert("tier".to_owned(), GenericValue::String("pro".into()));
    account.insert("country".to_owned(), GenericValue::String("PL".into()));
    let mut map = BTreeMap::new();
    map.insert("id".to_owned(), GenericValue::BigInt(id));
    map.insert(
        "email".to_owned(),
        GenericValue::String(format!("user{id}@example.com")),
    );
    map.insert(
        "fullName".to_owned(),
        GenericValue::String(format!("Customer Number {id}")),
    );
    map.insert("active".to_owned(), GenericValue::Bool(id % 2 == 0));
    map.insert("orderCount".to_owned(), GenericValue::BigInt(5));
    map.insert("lifetimeValue".to_owned(), GenericValue::Double(425.0));
    map.insert("account".to_owned(), GenericValue::Map(account));
    map.insert("orders".to_owned(), GenericValue::Array(orders));
    GenericValue::Map(map)
}

#[async_trait]
impl DocumentBuilder for MockDocuments {
    async fn resolve(
        &self,
        _table: &TableName,
        key: &RowKey,
        _before: Option<&RowImage>,
        _after: Option<&RowImage>,
    ) -> SourceResult<Vec<DocumentId>> {
        Ok(vec![DocumentId {
            index: index(),
            key: key.clone(),
        }])
    }

    async fn build_many(&self, ids: &[DocumentId]) -> SourceResult<Vec<Document>> {
        self.pace.build_started();
        let mut documents = Vec::with_capacity(ids.len());
        for id in ids {
            documents.push(self.build(id).await?);
        }
        Ok(documents)
    }

    async fn build(&self, id: &DocumentId) -> SourceResult<Document> {
        let row = match id.key.0.first() {
            Some((_, GenericValue::BigInt(v))) => *v,
            _ => 0,
        };
        Ok(Document::Upsert {
            id: id.clone(),
            body: body(row),
        })
    }

    fn backfill_indexes(&self) -> Vec<IndexName> {
        vec![index()]
    }

    async fn keys(
        &self,
        _index: &IndexName,
        after: Option<&RowKey>,
        limit: usize,
    ) -> SourceResult<Vec<RowKey>> {
        let start = after.map_or(0, |after| match after.0.first() {
            Some((_, GenericValue::BigInt(id))) => usize::try_from(*id).unwrap() + 1,
            _ => self.rows,
        });
        Ok((start..self.rows.min(start + limit))
            .map(|i| key(i as i64))
            .collect())
    }

    async fn index_mappings(&self) -> SourceResult<Vec<IndexMapping>> {
        Ok(vec![IndexMapping {
            index: index(),
            hash: ContentHash::new(1),
            fields: vec![ResolvedField {
                name: FieldName::try_new("id").unwrap(),
                mapping: Mapping {
                    mapping_type: MappingType::Long,
                    extra: BTreeMap::new(),
                    map_values: None,
                    decimal: false,
                    enum_order: None,
                    subfields: true,
                    indexed: true,
                },
                nullable: false,
                array: false,
                children: Vec::new(),
            }],
        }])
    }
}

/// Counts written envelopes and writes; reports `seeded` as configured so the
/// live benches skip the backfill and the snapshot bench requests one. With
/// a [`Pace`], each write of documents waits until the ingest engine is past
/// that batch.
#[derive(Debug)]
struct CountingSink {
    seeded: bool,
    applied: AtomicUsize,
    writes: AtomicUsize,
    pace: Option<Arc<Pace>>,
}

#[async_trait]
impl Sink for CountingSink {
    async fn write(&self, envelopes: &[Envelope], _caught_up: bool) -> sink::Result<WriteReport> {
        if !envelopes.is_empty() {
            self.applied.fetch_add(envelopes.len(), Ordering::Relaxed);
            let batch = self.writes.fetch_add(1, Ordering::Relaxed) + 1;
            if let Some(pace) = &self.pace {
                pace.past(batch).await;
            }
        }
        Ok(WriteReport::clean())
    }

    async fn is_seeded(&self, _: &IndexName) -> sink::Result<bool> {
        Ok(self.seeded)
    }

    async fn mark_seeded(&self, _: &IndexName) -> sink::Result<()> {
        Ok(())
    }
}

/// One deployment the way the daemon runs it: stage every sink engine, run them
/// beside the ingest engine until the live stream ends and the lanes drain.
async fn run_deployment(capture: MockCapture, sinks: usize, batch: BatchPolicy) {
    let names: Vec<SinkName> = (0..sinks)
        .map(|i| SinkName::try_new(format!("sink{i}")).unwrap())
        .collect();
    let stream = Arc::new(ChannelStream::new(1024, names.clone()));
    let pace = Arc::new(Pace::default());
    let documents: Arc<dyn DocumentBuilder> = Arc::new(MockDocuments {
        pace: Arc::clone(&pace),
        rows: capture.rows,
    });
    let mappings = documents.index_mappings().await.unwrap();
    let seeded = capture.rows == 0;

    let mut tasks = Vec::new();
    let mut controls = Vec::new();
    for name in &names {
        let sink: Arc<dyn Sink> = Arc::new(CountingSink {
            seeded,
            applied: AtomicUsize::new(0),
            writes: AtomicUsize::new(0),
            pace: seeded.then(|| Arc::clone(&pace)),
        });
        let engine = SinkEngine::new(
            name.clone(),
            sink,
            Arc::clone(&stream) as Arc<dyn Stream>,
            mappings.clone(),
        );
        engine.stage(Continuity::Resumed).await.unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<SinkControl>(1);
        controls.push(tx);
        tasks.push(tokio::spawn(async move { engine.run(&mut rx).await }));
    }

    IngestEngine::new(
        Arc::new(capture),
        documents,
        Arc::clone(&stream) as Arc<dyn Stream>,
        names,
    )
    .with_batch(batch)
    .run()
    .await
    .unwrap();
    pace.ingest_ended();

    while !stream.is_idle() {
        assert!(
            !tasks.iter().any(|t| t.is_finished()),
            "a sink engine stopped early"
        );
        tokio::task::yield_now().await;
    }
    for task in tasks {
        task.abort();
    }
    drop(controls);
}

fn runtime() -> Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap()
}

fn bench(c: &mut Criterion) {
    let rt = runtime();

    let mut group = c.benchmark_group("live_drain_held");
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(5));
    group.throughput(Throughput::Elements(CHANGES as u64));
    for &sinks in &[1usize, 2] {
        for &max_changes in &[1usize, 64, 256] {
            group.bench_with_input(
                BenchmarkId::new(format!("sinks{sinks}"), max_changes),
                &(sinks, max_changes),
                |b, &(sinks, max_changes)| {
                    b.to_async(&rt).iter(|| {
                        run_deployment(
                            MockCapture {
                                live: CHANGES,
                                rows: 0,
                            },
                            sinks,
                            BatchPolicy {
                                max_changes,
                                max_delay: Duration::from_secs(10),
                            },
                        )
                    });
                },
            );
        }
    }
    group.finish();

    let mut group = c.benchmark_group("snapshot");
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(5));
    group.throughput(Throughput::Elements(ROWS as u64));
    group.bench_function("rows", |b| {
        b.to_async(&rt).iter(|| {
            run_deployment(
                MockCapture {
                    live: 0,
                    rows: ROWS,
                },
                1,
                BatchPolicy::default(),
            )
        });
    });
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);

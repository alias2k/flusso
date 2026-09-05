//! In-process benchmark of the two engines over the real channel stream, with
//! the source, builder, and sink mocked — no Docker, no I/O.
//!
//! This isolates the engines' own cost: batching, resolve/dedup, the one
//! `build_many` per batch, publishing an `Arc<[Envelope]>` to every lane, the
//! sink engine's recv → apply → flush → ack loop, and the channel's ticketed
//! in-flight ledger. Every mock answers instantly, so a movement here is a
//! movement in the engines, not in Postgres or OpenSearch.
//!
//! Measured:
//!
//! - `live_drain`: a burst of `CHANGES` live changes drained end to end at
//!   several `max_changes`, with one sink and with two (the fan-out cost).
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
    ColumnName, ContentHash, DatabaseSchema, Envelope, FieldName, GenericValue, IndexMapping,
    IndexName, Mapping, MappingType, Position, ResolvedField, SinkName, TableName,
};
use sink::{FlushReport, Sink};
use source::cdc::{ChangeCapture, ChangeEvent, Continuity, LiveChange};
use source::document::{Document, DocumentBuilder, DocumentId, IndexScope};
use source::{Result as SourceResult, RowKey, SnapshotTable};
use stream::Stream;
use stream_channel::ChannelStream;
use tokio::runtime::Runtime;

/// Live changes per `live_drain` iteration.
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

/// A source whose live stream is a fixed burst of upserts, or a snapshot of a
/// fixed row set; `confirm` is a no-op.
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
                },
            ))
        }))))
    }

    fn confirm(&self, _position: Position) {}

    async fn snapshot(
        &self,
        _tables: &[SnapshotTable],
    ) -> SourceResult<BoxStream<'static, SourceResult<ChangeEvent>>> {
        let n = self.rows;
        Ok(Box::pin(fstream::iter((0..n).map(|i| {
            Ok(ChangeEvent::Upsert {
                table: table(),
                key: key(i as i64),
            })
        }))))
    }
}

/// Resolves every change to one document and builds a realistic mid-size body
/// (a dozen scalars, an object, a nested array) with no I/O.
#[derive(Debug)]
struct MockDocuments;

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
    async fn resolve(&self, _table: &TableName, key: &RowKey) -> SourceResult<Vec<DocumentId>> {
        Ok(vec![DocumentId {
            index: index(),
            key: key.clone(),
        }])
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

    fn backfill_scopes(&self) -> Vec<IndexScope> {
        vec![IndexScope {
            index: index(),
            root: SnapshotTable {
                db_schema: DatabaseSchema::try_new("public").unwrap(),
                table: table(),
            },
        }]
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
                },
                nullable: false,
                array: false,
                children: Vec::new(),
            }],
        }])
    }
}

/// Counts applied envelopes and flushes; reports `seeded` as configured so the
/// live benches skip the backfill and the snapshot bench requests one.
#[derive(Debug)]
struct CountingSink {
    seeded: bool,
    applied: AtomicUsize,
    flushes: AtomicUsize,
}

#[async_trait]
impl Sink for CountingSink {
    async fn apply(&self, _envelope: &Envelope) -> sink::Result<()> {
        self.applied.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    async fn flush(&self, _caught_up: bool) -> sink::Result<FlushReport> {
        self.flushes.fetch_add(1, Ordering::Relaxed);
        Ok(FlushReport::clean())
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
    let documents: Arc<dyn DocumentBuilder> = Arc::new(MockDocuments);
    let mappings = documents.index_mappings().await.unwrap();
    let seeded = capture.rows == 0;

    let mut tasks = Vec::new();
    let mut controls = Vec::new();
    for name in &names {
        let sink: Arc<dyn Sink> = Arc::new(CountingSink {
            seeded,
            applied: AtomicUsize::new(0),
            flushes: AtomicUsize::new(0),
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

    let mut group = c.benchmark_group("live_drain");
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

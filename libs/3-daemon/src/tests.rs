use super::*;

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use engine::{BuildStats, CommitStats};
use futures::StreamExt;
use futures::stream::{self, BoxStream};
use kernel::{
    ColumnName, DatabaseSchema, Envelope, GenericValue, IndexMapping, PortEntry, Position,
    TableName,
};
use sink::{FlushReport, Sink, SinkError, SinkOptions};
use source::cdc::{ChangeEvent, LiveChange};
use source::document::{Document, DocumentBuilder, DocumentId, IndexScope};
use source::{RowKey, SnapshotTable};
use stream_channel::ChannelStream;
use tokio::sync::{Notify, oneshot};

use crate::observer::StatusObserver;
use crate::status::{IndexState, Phase, SinkPhase};

fn users() -> IndexName {
    IndexName::try_new("users").unwrap()
}

fn primary() -> SinkName {
    SinkName::try_new("primary").unwrap()
}

fn commit(changes: usize, envelopes: usize) -> CommitStats {
    CommitStats {
        envelopes,
        changes,
        flush: Duration::from_millis(5),
    }
}

/// The observer drives the status surface through a full lifecycle — one
/// ingest side, one sink — and the snapshot serializes to the `/status` shape.
#[test]
fn observer_drives_status_through_its_lifecycle() {
    let status = Arc::new(Status::new([users()], [primary()], Instant::now()));
    let observer = StatusObserver::new(Arc::clone(&status));

    let snap = status.snapshot();
    assert_eq!(snap.phase, Phase::Starting);
    assert_eq!(snap.indexes.get("users"), Some(&IndexState::Pending));
    assert!(!status.is_ready());

    // The sink engine stages: ensures its index, requests a backfill, follows.
    observer.on_indexes_ensured(&primary(), 1);
    observer.on_backfill_requested(&primary(), &[users()]);
    observer.on_sink_started(&primary());
    observer.on_live_started();
    let snap = status.snapshot();
    assert_eq!(snap.phase, Phase::Backfilling);
    assert_eq!(snap.indexes.get("users"), Some(&IndexState::Backfilling));
    assert_eq!(snap.sinks["primary"].phase, SinkPhase::Backfilling);
    assert!(status.is_ready(), "backfilling counts as ready");

    // The snapshot lands and the sink records the seed.
    observer.on_index_seeded(&primary(), &users());
    let snap = status.snapshot();
    assert_eq!(snap.phase, Phase::Live);
    assert_eq!(snap.sinks["primary"].phase, SinkPhase::Live);

    // Three changes captured, two distinct documents built, one commit.
    observer.on_change_captured();
    observer.on_change_captured();
    observer.on_change_captured();
    observer.on_batch_built(BuildStats {
        changes: 3,
        documents: 2,
        documents_by_index: vec![(users(), 2)],
        build: Duration::from_millis(1),
    });
    assert_eq!(
        status.in_flight(),
        3,
        "built but not yet committed by the sink"
    );
    observer.on_batch_committed(&primary(), commit(3, 2));
    observer.on_slot_lag(4096);

    let snap = status.snapshot();
    assert_eq!(snap.indexes.get("users"), Some(&IndexState::Seeded));
    assert_eq!(snap.changes_captured, 3);
    assert_eq!(snap.changes_in_flight, 0);
    assert_eq!(snap.documents_built, 2);
    assert_eq!(snap.slot_lag_bytes, Some(4096));
    assert_eq!(snap.errors, 0);
    let sink = &snap.sinks["primary"];
    assert_eq!(sink.changes_committed, 3);
    assert_eq!(sink.envelopes_applied, 2);
    assert_eq!(sink.batches, 1);
    assert_eq!(sink.indexes.get("users"), Some(&IndexState::Seeded));

    let json = serde_json::to_value(&snap).unwrap();
    assert_eq!(json["phase"], "live");
    assert_eq!(json["indexes"]["users"], "seeded");
    assert_eq!(json["changes_in_flight"], 0);
    assert_eq!(json["slot_lag_bytes"], 4096);
    assert_eq!(json["sinks"]["primary"]["phase"], "live");
    assert_eq!(json["sinks"]["primary"]["indexes"]["users"], "seeded");
    assert_eq!(json["sinks"]["primary"]["changes_committed"], 3);
}

/// With two sinks, the deployment is `backfilling` while either sink still
/// seeds, in-flight is measured against the slowest sink, and a failing sink
/// is visible on its own without stopping the deployment.
#[test]
fn status_tracks_each_sink_separately() {
    let audit = SinkName::try_new("audit").unwrap();
    let status = Arc::new(Status::new(
        [users()],
        [primary(), audit.clone()],
        Instant::now(),
    ));
    let observer = StatusObserver::new(Arc::clone(&status));

    // `audit` never backfills (`backfill = false`); `primary` seeds.
    observer.on_sink_started(&audit);
    observer.on_backfill_requested(&primary(), &[users()]);
    observer.on_sink_started(&primary());
    observer.on_live_started();
    let snap = status.snapshot();
    assert_eq!(snap.phase, Phase::Backfilling);
    assert_eq!(snap.sinks["audit"].phase, SinkPhase::Live);
    assert_eq!(
        snap.sinks["audit"].indexes.get("users"),
        Some(&IndexState::Seeded)
    );
    assert_eq!(snap.indexes.get("users"), Some(&IndexState::Backfilling));

    observer.on_index_seeded(&primary(), &users());
    assert_eq!(status.snapshot().phase, Phase::Live);

    observer.on_change_captured();
    observer.on_change_captured();
    observer.on_batch_committed(&audit, commit(2, 2));
    assert_eq!(
        status.in_flight(),
        2,
        "the slowest sink has committed nothing"
    );
    assert_eq!(status.in_flight_for(&audit), 0);
    assert_eq!(status.in_flight_for(&primary()), 2);

    observer.on_engine_error(&EngineId::Sink(primary()), "flush failed");
    let snap = status.snapshot();
    assert_eq!(snap.sinks["primary"].phase, SinkPhase::Failed);
    assert_eq!(snap.phase, Phase::Live, "the deployment keeps running");
    assert_eq!(snap.errors, 1);
    assert_eq!(snap.last_error.as_deref(), Some("flush failed"));
    assert!(!status.is_ready(), "a failed sink engine is not ready");

    observer.on_engine_error(&EngineId::Ingest, "source gone");
    let snap = status.snapshot();
    assert_eq!(
        snap.phase,
        Phase::Starting,
        "the daemon is restarting the ingest engine"
    );
    assert_eq!(snap.errors, 2);
    assert!(!status.is_ready());

    observer.on_live_started();
    assert_eq!(
        status.snapshot().phase,
        Phase::Live,
        "recovered; not stuck on the error"
    );
    assert!(
        !status.is_ready(),
        "the failed sink still holds readiness back"
    );
    observer.on_sink_started(&primary());
    assert!(status.is_ready());

    observer.on_engine_stopped(&EngineId::Ingest);
    assert_eq!(
        status.snapshot().phase,
        Phase::Stopped,
        "a clean end is final"
    );
    observer.on_live_started();
    assert_eq!(status.snapshot().phase, Phase::Stopped);
}

/// A source that reports a fixed lag and an empty live stream.
#[derive(Debug)]
struct LaggySource(Option<u64>);

#[async_trait]
impl ChangeCapture for LaggySource {
    async fn continuity(&self) -> source::Result<Continuity> {
        Ok(Continuity::Resumed)
    }

    async fn prepare(&self) -> source::Result<()> {
        Ok(())
    }

    async fn live(&self) -> source::Result<BoxStream<'static, source::Result<LiveChange>>> {
        Ok(Box::pin(stream::empty()))
    }

    fn confirm(&self, _: Position) {}

    async fn lag(&self) -> source::Result<Option<u64>> {
        Ok(self.0)
    }
}

/// Records the slot lag it's told and signals each report, so the poller
/// test can await a real report instead of sleeping a fixed duration.
#[derive(Debug, Default)]
struct LagObserver {
    last: Mutex<Option<u64>>,
    reported: Notify,
}

impl Observer for LagObserver {
    fn on_slot_lag(&self, bytes: u64) {
        *self.last.lock().unwrap() = Some(bytes);
        self.reported.notify_one();
    }
}

/// The lag poller samples the source and reports each known value to the
/// observer. Deterministic: it awaits an actual report (the poller's first
/// interval tick fires immediately), with a generous timeout as a backstop.
#[tokio::test]
async fn lag_poller_reports_each_sampled_value() {
    let observer = Arc::new(LagObserver::default());
    let source: Arc<dyn ChangeCapture> = Arc::new(LaggySource(Some(8192)));

    let handle = tokio::spawn(lag::poll(
        source,
        Arc::clone(&observer) as Arc<dyn Observer>,
        Duration::from_millis(5),
    ));
    tokio::time::timeout(Duration::from_secs(5), observer.reported.notified())
        .await
        .expect("the poller should report a lag sample");
    handle.abort();

    assert_eq!(*observer.last.lock().unwrap(), Some(8192));
}

// --- The daemon driven end-to-end over injected backends -----------------
//
// These exercise `Daemon::start`/`run` with no Postgres/OpenSearch by
// supplying a `Backends` that hands back test doubles — the seam the daemon
// exists to keep adapter-free.

/// A `Backends` that returns pre-built test doubles, ignoring the `Config`.
/// Counts how often each edge was asked for, to prove the daemon builds its
/// adapters *through* the seam rather than naming any concrete one.
#[derive(Debug)]
struct MockBackends {
    capture: Arc<dyn ChangeCapture>,
    documents: Arc<dyn DocumentBuilder>,
    sinks: Vec<(SinkName, Arc<dyn Sink>, SinkOptions)>,
    built: Arc<Mutex<Vec<&'static str>>>,
}

#[async_trait]
impl Backends for MockBackends {
    fn validate(&self, _config: &Config) -> anyhow::Result<()> {
        self.built.lock().unwrap().push("validate");
        Ok(())
    }

    async fn source(
        &self,
        _config: Arc<Config>,
        _options: &DaemonOptions,
    ) -> anyhow::Result<SourceParts> {
        self.built.lock().unwrap().push("source");
        Ok(SourceParts {
            capture: Arc::clone(&self.capture),
            documents: Arc::clone(&self.documents),
        })
    }

    fn stream(&self, _config: &Config, sinks: &[SinkName]) -> anyhow::Result<Arc<dyn Stream>> {
        self.built.lock().unwrap().push("stream");
        Ok(Arc::new(ChannelStream::new(64, sinks.iter().cloned())))
    }

    async fn sinks(&self, _config: &Config) -> anyhow::Result<Vec<SinkParts>> {
        self.built.lock().unwrap().push("sinks");
        Ok(self
            .sinks
            .iter()
            .map(|(name, sink, options)| SinkParts {
                name: name.clone(),
                sink: Arc::clone(sink),
                options: *options,
            })
            .collect())
    }
}

/// Replays a fixed list of changes on the live stream (positions `0..n`),
/// then either ends (so the run completes on its own) or stays open until
/// `end` fires. Records what was confirmed.
#[derive(Debug)]
struct ScriptedSource {
    changes: Mutex<Option<Vec<ChangeEvent>>>,
    end: Mutex<Option<oneshot::Receiver<()>>>,
    continuity: Continuity,
    /// Shared with the sinks in a test, so the order of `prepare`/`snapshot`
    /// against the sinks' staging can be asserted.
    events: Arc<Mutex<Vec<String>>>,
    confirmed: Arc<Mutex<Vec<Position>>>,
}

impl ScriptedSource {
    fn new(changes: Vec<ChangeEvent>) -> Self {
        Self {
            changes: Mutex::new(Some(changes)),
            end: Mutex::new(None),
            continuity: Continuity::Resumed,
            events: Arc::new(Mutex::new(Vec::new())),
            confirmed: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Keep the live stream open after the scripted changes until the returned
    /// sender fires (or is dropped).
    fn held_open(mut self) -> (Self, oneshot::Sender<()>) {
        let (tx, rx) = oneshot::channel();
        self.end = Mutex::new(Some(rx));
        (self, tx)
    }

    fn with_continuity(mut self, continuity: Continuity) -> Self {
        self.continuity = continuity;
        self
    }
}

#[async_trait]
impl ChangeCapture for ScriptedSource {
    async fn continuity(&self) -> source::Result<Continuity> {
        Ok(self.continuity)
    }

    async fn prepare(&self) -> source::Result<()> {
        self.events.lock().unwrap().push("prepare".to_owned());
        Ok(())
    }

    async fn live(&self) -> source::Result<BoxStream<'static, source::Result<LiveChange>>> {
        let changes = self.changes.lock().unwrap().take().unwrap_or_default();
        let scripted = stream::iter(
            changes
                .into_iter()
                .enumerate()
                .map(|(i, event)| Ok((Position(i as u64), event))),
        );
        let end = self.end.lock().unwrap().take();
        let tail = stream::unfold(end, |end| async move {
            if let Some(rx) = end {
                let _ = rx.await;
            }
            None
        });
        Ok(Box::pin(scripted.chain(tail)))
    }

    fn confirm(&self, position: Position) {
        self.confirmed.lock().unwrap().push(position);
    }

    async fn snapshot(
        &self,
        _tables: &[SnapshotTable],
    ) -> source::Result<BoxStream<'static, source::Result<ChangeEvent>>> {
        self.events.lock().unwrap().push("snapshot".to_owned());
        Ok(Box::pin(stream::iter(
            [row_event(false, 10), row_event(false, 11)].map(Ok),
        )))
    }
}

/// A source whose first live stream yields one change and then fails. The
/// reopened stream behaves as the Postgres adapter does: the unconfirmed change
/// is redelivered under a *new* position (numbering continues across streams),
/// then the next change follows and the stream ends. Records what was confirmed.
#[derive(Debug)]
struct FlakySource {
    opened: AtomicU64,
    confirmed: Arc<Mutex<Vec<Position>>>,
}

#[async_trait]
impl ChangeCapture for FlakySource {
    async fn continuity(&self) -> source::Result<Continuity> {
        Ok(Continuity::Resumed)
    }

    async fn prepare(&self) -> source::Result<()> {
        Ok(())
    }

    async fn live(&self) -> source::Result<BoxStream<'static, source::Result<LiveChange>>> {
        let opened = self.opened.fetch_add(1, Ordering::SeqCst);
        Ok(if opened == 0 {
            Box::pin(stream::iter([
                Ok((Position(0), row_event(false, 1))),
                Err(source::SourceError::Connection("simulated drop".to_owned())),
            ]))
        } else {
            Box::pin(stream::iter([
                Ok((Position(1), row_event(false, 1))),
                Ok((Position(2), row_event(false, 3))),
            ]))
        })
    }

    fn confirm(&self, position: Position) {
        self.confirmed.lock().unwrap().push(position);
    }
}

/// Resolves each change to one `users` document; key value `2` is a delete.
#[derive(Debug)]
struct ScriptedDocuments;

#[async_trait]
impl DocumentBuilder for ScriptedDocuments {
    async fn resolve(&self, _table: &TableName, key: &RowKey) -> source::Result<Vec<DocumentId>> {
        Ok(vec![DocumentId {
            index: users(),
            key: key.clone(),
        }])
    }

    async fn build(&self, id: &DocumentId) -> source::Result<Document> {
        let deleted = matches!(id.key.0.first(), Some((_, GenericValue::BigInt(2))));
        Ok(if deleted {
            Document::Delete { id: id.clone() }
        } else {
            Document::Upsert {
                id: id.clone(),
                body: GenericValue::Map(Default::default()),
            }
        })
    }

    fn backfill_scopes(&self) -> Vec<IndexScope> {
        vec![IndexScope {
            index: users(),
            root: SnapshotTable {
                db_schema: DatabaseSchema::try_new("public").unwrap(),
                table: TableName::try_new("users").unwrap(),
            },
        }]
    }

    async fn index_mappings(&self) -> source::Result<Vec<IndexMapping>> {
        Ok(vec![IndexMapping {
            index: users(),
            hash: kernel::ContentHash::new(1),
            fields: Vec::new(),
        }])
    }
}

/// Records the envelopes it receives as `"<op> <index> <id>"`, the seed hooks,
/// and fails the first `failing_flushes` flushes with a flush-wide error.
#[derive(Debug, Default)]
struct RecordingSink {
    ops: Arc<Mutex<Vec<String>>>,
    seeded: AtomicBool,
    marked: Arc<Mutex<Vec<String>>>,
    reindexed: Arc<Mutex<Vec<String>>>,
    events: Arc<Mutex<Vec<String>>>,
    failing_flushes: AtomicU64,
}

impl RecordingSink {
    fn seeded() -> Self {
        let sink = Self::default();
        sink.seeded.store(true, Ordering::SeqCst);
        sink
    }

    fn with_events(mut self, events: &Arc<Mutex<Vec<String>>>) -> Self {
        self.events = Arc::clone(events);
        self
    }
}

#[async_trait]
impl Sink for RecordingSink {
    async fn apply(&self, envelope: &Envelope) -> sink::Result<()> {
        self.ops.lock().unwrap().push(format!(
            "{} {} {}",
            envelope.op,
            envelope.index.as_ref(),
            envelope.id
        ));
        Ok(())
    }

    async fn flush(&self, _caught_up: bool) -> sink::Result<FlushReport> {
        let remaining = self.failing_flushes.load(Ordering::SeqCst);
        if remaining > 0 {
            self.failing_flushes.store(remaining - 1, Ordering::SeqCst);
            return Err(SinkError::Write("simulated outage".to_owned()));
        }
        Ok(FlushReport::clean())
    }

    async fn is_seeded(&self, _: &IndexName) -> sink::Result<bool> {
        Ok(self.seeded.load(Ordering::SeqCst))
    }

    async fn mark_seeded(&self, index: &IndexName) -> sink::Result<()> {
        self.marked.lock().unwrap().push(index.as_ref().to_owned());
        self.seeded.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn reindex(&self, mapping: &IndexMapping) -> sink::Result<()> {
        let index = mapping.index.as_ref().to_owned();
        self.events.lock().unwrap().push(format!("reindex {index}"));
        self.reindexed.lock().unwrap().push(index);
        self.seeded.store(false, Ordering::SeqCst);
        Ok(())
    }
}

fn row_event(delete: bool, id: i64) -> ChangeEvent {
    let table = TableName::try_new("users").unwrap();
    let key = RowKey(vec![(
        ColumnName::try_new("id").unwrap(),
        GenericValue::BigInt(id),
    )]);
    if delete {
        ChangeEvent::Delete { table, key }
    } else {
        ChangeEvent::Upsert { table, key }
    }
}

/// A config the `MockBackends` ignores — the daemon reads only `indexes` (for
/// the status surface) and the entries' kinds (for its startup log).
fn backendless_config() -> Config {
    let schema = kernel::IndexSchema {
        version: 1,
        table: TableName::try_new("users").unwrap(),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(ColumnName::try_new("id").unwrap()),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: Vec::new(),
    };
    Config {
        source: PortEntry::new("mock"),
        stream: PortEntry::new(config::DEFAULT_STREAM_KIND),
        sinks: BTreeMap::new(),
        indexes: BTreeMap::from([(
            users(),
            config::Index {
                enabled: true,
                schema,
                on_error: None,
            },
        )]),
        on_error: Default::default(),
        server: Default::default(),
        prefix: String::new(),
    }
}

fn backends(
    source: ScriptedSource,
    sinks: Vec<(SinkName, Arc<dyn Sink>, SinkOptions)>,
) -> Arc<MockBackends> {
    Arc::new(MockBackends {
        capture: Arc::new(source),
        documents: Arc::new(ScriptedDocuments),
        sinks,
        built: Arc::new(Mutex::new(Vec::new())),
    })
}

fn fast_restarts() -> DaemonOptions {
    DaemonOptions {
        max_restart_backoff: Duration::from_millis(10),
        ..DaemonOptions::default()
    }
}

async fn wait_until(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// `Daemon::start` builds every edge **through** the injected `Backends`, in
/// order (validate, source, sinks, stream), never naming a concrete adapter;
/// a run over an empty live stream returns cleanly with the status `Stopped`.
#[tokio::test]
async fn start_builds_backends_through_the_seam() {
    let sink = Arc::new(RecordingSink::seeded());
    let backends = backends(
        ScriptedSource::new(Vec::new()),
        vec![(primary(), sink, SinkOptions::default())],
    );
    let built = Arc::clone(&backends.built);

    let running = Daemon::new(backendless_config(), backends)
        .start()
        .await
        .unwrap();
    let status = running.status();
    assert_eq!(
        *built.lock().unwrap(),
        vec!["validate", "source", "sinks", "stream"]
    );
    assert_eq!(status.sinks().collect::<Vec<_>>(), vec![&primary()]);
    assert_eq!(
        running.control().sinks().collect::<Vec<_>>(),
        vec![&primary()]
    );

    running.run(std::future::pending::<()>()).await.unwrap();

    let snap = status.snapshot();
    assert_eq!(snap.phase, Phase::Stopped);
    assert_eq!(snap.changes_captured, 0);
}

/// A run over a non-empty live stream drives changes through the injected
/// builder to **every** sink — capture, build once, publish, apply, flush, ack
/// — confirms the watermark to the source, and the status reflects it.
#[tokio::test]
async fn drives_changes_through_injected_backends() {
    let primary_sink = Arc::new(RecordingSink::seeded());
    let audit_sink = Arc::new(RecordingSink::default());
    let audit = SinkName::try_new("audit").unwrap();
    let source = ScriptedSource::new(vec![row_event(false, 1), row_event(true, 2)]);
    let confirmed = Arc::clone(&source.confirmed);
    let backends = backends(
        source,
        vec![
            (
                primary(),
                Arc::clone(&primary_sink) as Arc<dyn Sink>,
                SinkOptions::default(),
            ),
            (
                audit.clone(),
                Arc::clone(&audit_sink) as Arc<dyn Sink>,
                SinkOptions { backfill: false },
            ),
        ],
    );

    let running = Daemon::new(backendless_config(), backends)
        .start()
        .await
        .unwrap();
    let status = running.status();
    running.run(std::future::pending::<()>()).await.unwrap();

    let expected = vec!["upsert users 1".to_owned(), "delete users 2".to_owned()];
    assert_eq!(*primary_sink.ops.lock().unwrap(), expected);
    assert_eq!(*audit_sink.ops.lock().unwrap(), expected);
    assert!(
        audit_sink.marked.lock().unwrap().is_empty(),
        "backfill = false: the sink is never seeded"
    );
    assert_eq!(
        confirmed.lock().unwrap().last(),
        Some(&Position(1)),
        "the last position reaches the source once every lane acked it"
    );

    let snap = status.snapshot();
    assert_eq!(snap.phase, Phase::Stopped);
    assert_eq!(snap.changes_captured, 2);
    assert_eq!(snap.changes_in_flight, 0);
    for name in ["primary", "audit"] {
        assert_eq!(snap.sinks[name].changes_committed, 2, "{name}");
        assert_eq!(snap.sinks[name].envelopes_applied, 2, "{name}");
    }
}

/// An unseeded sink gets its backfill served: the sink engine requests it
/// before the ingest engine prepares the source, the snapshot rows land only on
/// that sink, and the seed is recorded — while a seeded sibling sees nothing.
#[tokio::test]
async fn unseeded_sink_is_backfilled_without_touching_its_sibling() {
    let fresh = Arc::new(RecordingSink::default());
    let seeded = Arc::new(RecordingSink::seeded());
    let audit = SinkName::try_new("audit").unwrap();
    let backends = backends(
        ScriptedSource::new(Vec::new()),
        vec![
            (
                primary(),
                Arc::clone(&fresh) as Arc<dyn Sink>,
                SinkOptions::default(),
            ),
            (
                audit,
                Arc::clone(&seeded) as Arc<dyn Sink>,
                SinkOptions::default(),
            ),
        ],
    );

    let running = Daemon::new(backendless_config(), backends)
        .start()
        .await
        .unwrap();
    let status = running.status();
    running.run(std::future::pending::<()>()).await.unwrap();

    assert_eq!(
        *fresh.ops.lock().unwrap(),
        vec!["upsert users 10".to_owned(), "upsert users 11".to_owned()]
    );
    assert_eq!(*fresh.marked.lock().unwrap(), vec!["users".to_owned()]);
    assert!(seeded.ops.lock().unwrap().is_empty());
    assert!(seeded.marked.lock().unwrap().is_empty());
    assert_eq!(
        status.snapshot().sinks["primary"].indexes.get("users"),
        Some(&IndexState::Seeded)
    );
}

/// A `Fresh` source retires every seed: the seeded sink is rebuilt — its
/// `reindex` staged **before** the source is prepared, and the snapshot taken
/// **after** — then refilled and re-recorded as seeded (the #120 ordering).
#[tokio::test]
async fn fresh_source_rebuilds_seeded_sinks_before_preparing() {
    let source = ScriptedSource::new(Vec::new()).with_continuity(Continuity::Fresh);
    let events = Arc::clone(&source.events);
    let sink = Arc::new(RecordingSink::seeded().with_events(&events));
    let backends = backends(
        source,
        vec![(
            primary(),
            Arc::clone(&sink) as Arc<dyn Sink>,
            SinkOptions::default(),
        )],
    );

    let running = Daemon::new(backendless_config(), backends)
        .start()
        .await
        .unwrap();
    running.run(std::future::pending::<()>()).await.unwrap();

    assert_eq!(
        *events.lock().unwrap(),
        vec![
            "reindex users".to_owned(),
            "prepare".to_owned(),
            "snapshot".to_owned()
        ]
    );
    assert_eq!(
        *sink.ops.lock().unwrap(),
        vec!["upsert users 10".to_owned(), "upsert users 11".to_owned()]
    );
    assert_eq!(*sink.marked.lock().unwrap(), vec!["users".to_owned()]);
}

/// A reindex operation reaches the targeted sink engine while it runs: the sink
/// stages a fresh generation, requests its snapshot, and re-records the seed.
/// The sibling sink is untouched.
#[tokio::test]
async fn reindex_operation_targets_one_sink() {
    let primary_sink = Arc::new(RecordingSink::seeded());
    let audit_sink = Arc::new(RecordingSink::seeded());
    let audit = SinkName::try_new("audit").unwrap();
    let (source, end) = ScriptedSource::new(Vec::new()).held_open();
    let backends = backends(
        source,
        vec![
            (
                primary(),
                Arc::clone(&primary_sink) as Arc<dyn Sink>,
                SinkOptions::default(),
            ),
            (
                audit.clone(),
                Arc::clone(&audit_sink) as Arc<dyn Sink>,
                SinkOptions::default(),
            ),
        ],
    );

    let running = Daemon::new(backendless_config(), backends)
        .start()
        .await
        .unwrap();
    let status = running.status();
    let control = running.control();
    let run = tokio::spawn(running.run(std::future::pending::<()>()));

    wait_until("the deployment to be live", || status.is_ready()).await;
    assert!(matches!(
        control.reindex(users(), Some(&SinkName::try_new("nope").unwrap())),
        Err(ControlError::UnknownSink(_))
    ));
    control.reindex(users(), Some(&primary())).unwrap();

    wait_until("the reindex to complete", || {
        primary_sink.marked.lock().unwrap().len() == 1
    })
    .await;
    assert_eq!(
        *primary_sink.reindexed.lock().unwrap(),
        vec!["users".to_owned()]
    );
    assert_eq!(
        *primary_sink.ops.lock().unwrap(),
        vec!["upsert users 10".to_owned(), "upsert users 11".to_owned()]
    );
    assert!(audit_sink.reindexed.lock().unwrap().is_empty());
    assert!(audit_sink.ops.lock().unwrap().is_empty());
    assert_eq!(
        status.snapshot().sinks["primary"].indexes.get("users"),
        Some(&IndexState::Seeded)
    );

    let _ = end.send(());
    run.await.unwrap().unwrap();
}

/// A sink engine that stops on a flush-wide error is restarted with backoff and
/// redelivered the batch it left unacknowledged, while the deployment keeps
/// running; the error is counted and the sink recovers to `Live`.
#[tokio::test]
async fn failed_sink_engine_restarts_and_redelivers() {
    let sink = Arc::new(RecordingSink::seeded());
    sink.failing_flushes.store(1, Ordering::SeqCst);
    let source = ScriptedSource::new(vec![row_event(false, 1)]);
    let confirmed = Arc::clone(&source.confirmed);
    let backends = backends(
        source,
        vec![(
            primary(),
            Arc::clone(&sink) as Arc<dyn Sink>,
            SinkOptions::default(),
        )],
    );

    let running = Daemon::new(backendless_config(), backends)
        .with_options(fast_restarts())
        .start()
        .await
        .unwrap();
    let status = running.status();
    running.run(std::future::pending::<()>()).await.unwrap();

    assert_eq!(
        *sink.ops.lock().unwrap(),
        vec!["upsert users 1".to_owned(), "upsert users 1".to_owned()],
        "the batch is applied again after the restart"
    );
    assert_eq!(confirmed.lock().unwrap().last(), Some(&Position(0)));
    let snap = status.snapshot();
    assert_eq!(snap.errors, 1);
    assert_eq!(snap.sinks["primary"].changes_committed, 1);
    assert_eq!(snap.sinks["primary"].batches, 1);
}

/// The caller's shutdown future stops the run even while the source stream is
/// open.
#[tokio::test]
async fn shutdown_future_stops_an_open_run() {
    let sink = Arc::new(RecordingSink::seeded());
    let (source, _end) = ScriptedSource::new(Vec::new()).held_open();
    let backends = backends(source, vec![(primary(), sink, SinkOptions::default())]);

    let running = Daemon::new(backendless_config(), backends)
        .start()
        .await
        .unwrap();
    let status = running.status();
    let (shutdown, shutdown_rx) = oneshot::channel::<()>();
    let run = tokio::spawn(running.run(async move {
        let _ = shutdown_rx.await;
    }));

    wait_until("the deployment to be live", || status.is_ready()).await;
    shutdown.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), run)
        .await
        .expect("the run stops on shutdown")
        .unwrap()
        .unwrap();
    assert_eq!(status.snapshot().phase, Phase::Stopped);
}

/// The ingest engine is restarted with backoff when its live stream fails: the
/// change the failed stream never committed is redelivered and reaches the sink
/// with the change after it, the error is counted, and the source is confirmed
/// only positions the sink acked.
#[tokio::test]
async fn failed_ingest_engine_restarts_and_resumes_the_stream() {
    let sink = Arc::new(RecordingSink::seeded());
    let confirmed = Arc::new(Mutex::new(Vec::new()));
    let backends = Arc::new(MockBackends {
        capture: Arc::new(FlakySource {
            opened: AtomicU64::new(0),
            confirmed: Arc::clone(&confirmed),
        }),
        documents: Arc::new(ScriptedDocuments),
        sinks: vec![(
            primary(),
            Arc::clone(&sink) as Arc<dyn Sink>,
            SinkOptions::default(),
        )],
        built: Arc::new(Mutex::new(Vec::new())),
    });

    let running = Daemon::new(backendless_config(), backends)
        .with_options(fast_restarts())
        .start()
        .await
        .unwrap();
    let status = running.status();
    running.run(std::future::pending::<()>()).await.unwrap();

    assert_eq!(
        *sink.ops.lock().unwrap(),
        vec!["upsert users 1".to_owned(), "upsert users 3".to_owned()]
    );
    let confirmed = confirmed.lock().unwrap();
    assert_eq!(confirmed.last(), Some(&Position(2)));
    assert!(
        !confirmed.contains(&Position(0)),
        "the change the failed stream never committed is not confirmed"
    );
    let snap = status.snapshot();
    assert_eq!(snap.errors, 1);
    assert_eq!(snap.sinks["primary"].changes_committed, 2);
    assert_eq!(snap.phase, Phase::Stopped);
}

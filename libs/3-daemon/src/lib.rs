//! The `flusso` daemon — the supervisor around the [`engine`].
//!
//! It builds the pluggable parts from a validated [`Config`], wires a
//! [`StatusObserver`] that updates a shared [`Status`], runs the engine, and
//! polls source lag out of band.
//!
//! It owns the **domain**: the pipeline and its observable state, and it is
//! telemetry-agnostic — it depends only on the engine's [`Observer`] trait, not
//! on any metrics backend. It does *not* own **transport**: the HTTP surface,
//! process signals, the telemetry exporter, *and the metrics recording itself*
//! live in the binary (the CLI), which installs a meter provider, attaches its
//! own metrics observer via [`Daemon::with_observer`], reads the [`Status`]
//! handle this exposes, serves it, and drives shutdown:
//!
//! ```text
//!   CLI ── install meter provider ─▶ Daemon::start ──▶ RunningDaemon
//!    │                                                   │  .status() ─▶ Arc<Status>  (CLI serves it)
//!    └── shutdown future (signals) ─▶ RunningDaemon::run(shutdown)
//! ```

mod backends;
mod lag;
mod observer;
pub mod status;

pub use backends::{Backends, SourceParts};
pub use observer::StatusObserver;
pub use status::{IndexState, Phase, Status, StatusSnapshot};

// Re-exported so a binary can attach its own observer (e.g. a metrics recorder)
// without depending on `engine`/`schema-core` directly — these are part of the
// daemon's observe-the-pipeline surface.
pub use engine::{BatchStats, Observer};
pub use schema_core::IndexName;

use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use engine::{Engine, FailurePolicies, FanOut};
use schema::Config;
use sources_core::cdc::ChangeCapture;

/// How a [`Daemon`] run is parameterized — the pipeline knobs the CLI exposes as
/// flags. Transport settings (HTTP address, …) are the binary's concern, not the
/// daemon's, so they are not here.
#[derive(Debug, Clone)]
pub struct DaemonOptions {
    /// Logical replication slot to consume. Must already exist or be creatable.
    pub slot: String,
    /// Publication to subscribe to.
    pub publication: String,
    /// Skip the initial backfill and resume live capture only.
    pub skip_backfill: bool,
    /// Changes buffered between capture and processing.
    pub queue_capacity: usize,
    /// Pretty-print documents on the stdout fallback sink (no sink configured).
    pub pretty: bool,
    /// How often to sample source capture lag.
    pub lag_poll_interval: Duration,
}

impl Default for DaemonOptions {
    fn default() -> Self {
        Self {
            slot: "flusso".to_owned(),
            publication: "flusso".to_owned(),
            skip_backfill: false,
            queue_capacity: 1024,
            pretty: false,
            lag_poll_interval: Duration::from_secs(15),
        }
    }
}

/// A configured-but-not-yet-running sync daemon over one [`Config`].
#[derive(Debug)]
pub struct Daemon {
    config: Config,
    options: DaemonOptions,
    backends: Arc<dyn Backends>,
    extra_observers: Vec<Arc<dyn Observer>>,
}

impl Daemon {
    /// Create a daemon for `config` with default [`DaemonOptions`].
    ///
    /// `backends` builds the concrete source/sink the engine drives; the daemon
    /// itself never names a backend (see [`Backends`]). The composition root
    /// supplies it.
    pub fn new(config: Config, backends: Arc<dyn Backends>) -> Self {
        Self {
            config,
            options: DaemonOptions::default(),
            backends,
            extra_observers: Vec::new(),
        }
    }

    /// Override the run options.
    pub fn with_options(mut self, options: DaemonOptions) -> Self {
        self.options = options;
        self
    }

    /// Attach an additional [`Observer`] alongside the daemon's own status
    /// observer — e.g. a metrics recorder the binary owns. All attached
    /// observers receive every event (the engine drives a [`FanOut`]).
    pub fn with_observer(mut self, observer: Arc<dyn Observer>) -> Self {
        self.extra_observers.push(observer);
        self
    }

    /// Build the pipeline and its observable state, returning a [`RunningDaemon`]
    /// whose [`status`](RunningDaemon::status) can be read (e.g. served over HTTP)
    /// while it runs.
    ///
    /// If an attached observer (via [`with_observer`](Self::with_observer)) records
    /// to the global OpenTelemetry meter, install a meter provider *before* calling
    /// this; otherwise its instruments are no-ops.
    #[tracing::instrument(name = "daemon.start", skip_all)]
    pub async fn start(self) -> anyhow::Result<RunningDaemon> {
        let Daemon {
            config,
            options,
            backends,
            extra_observers,
        } = self;

        tracing::info!(
            slot = %options.slot,
            publication = %options.publication,
            indexes = config.indexes.len(),
            "starting sync",
        );

        let status = Arc::new(Status::new(config.indexes.keys().cloned(), Instant::now()));
        // The daemon's own observer updates status; any binary-supplied observers
        // (metrics, …) ride alongside it through one fan-out.
        let mut observers: Vec<Arc<dyn Observer>> =
            vec![Arc::new(StatusObserver::new(Arc::clone(&status)))];
        observers.extend(extra_observers);
        let observer: Arc<dyn Observer> = Arc::new(FanOut::new(observers));

        // The concrete source/sink are the composition root's choice — built
        // here through the `Backends` seam, resolving connection/credentials in
        // this (the running) environment.
        let config = Arc::new(config);
        let SourceParts { capture, documents } =
            backends.source(Arc::clone(&config), &options).await?;
        let sink = backends.sink(&config, &options).await?;

        // The item-level failure policy is config-driven: a global default plus
        // optional per-index overrides, keyed by logical index name.
        let mut failure_policies = FailurePolicies::new(config.on_error);
        for (name, index) in &config.indexes {
            if let Some(policy) = index.on_error {
                failure_policies = failure_policies.with_override(name.as_ref(), policy);
            }
        }

        let engine = Engine::new(Arc::clone(&capture), documents, sink)
            .with_observer(Arc::clone(&observer))
            .with_queue_capacity(options.queue_capacity)
            .skip_backfill(options.skip_backfill)
            .with_failure_policies(failure_policies);

        Ok(RunningDaemon {
            status,
            engine,
            source: capture,
            observer,
            lag_poll_interval: options.lag_poll_interval,
        })
    }
}

/// A built sync daemon, ready to run. Exposes its live [`Status`] so a transport
/// the binary owns can serve it concurrently with the run.
#[derive(Debug)]
pub struct RunningDaemon {
    status: Arc<Status>,
    engine: Engine,
    source: Arc<dyn ChangeCapture>,
    observer: Arc<dyn Observer>,
    lag_poll_interval: Duration,
}

impl RunningDaemon {
    /// A handle to the live operational status, for a transport (HTTP, a TUI, …)
    /// to read while the daemon runs. Cheap to clone.
    pub fn status(&self) -> Arc<Status> {
        Arc::clone(&self.status)
    }

    /// Run until the live stream ends, an error stops the pipeline, or `shutdown`
    /// resolves — typically a signal future the binary owns. A pending batch on
    /// shutdown is simply redelivered on the next run (at-least-once), so
    /// dropping the run mid-flight is safe.
    #[tracing::instrument(name = "daemon.run", skip_all)]
    pub async fn run(self, shutdown: impl Future<Output = ()> + Send) -> anyhow::Result<()> {
        let RunningDaemon {
            status,
            engine,
            source,
            observer,
            lag_poll_interval,
        } = self;

        // Poll capture lag alongside the run; aborted when the run returns.
        let lag = tokio::spawn(lag::poll(source, observer, lag_poll_interval));

        let result = tokio::select! {
            res = engine.run() => res.context("sync engine stopped"),
            () = shutdown => {
                tracing::info!("shutdown requested; stopping pipeline");
                Ok(())
            }
        };

        lag.abort();
        status.set_phase(Phase::Stopped);
        result
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    use std::collections::BTreeMap;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    use async_trait::async_trait;
    use engine::BatchStats;
    use futures::stream::{self, BoxStream};
    use schema::{Source, SourceType};
    use schema_core::{ColumnName, DatabaseSchema, GenericValue, IndexName, TableName};
    use sinks_core::{FlushReport, Sink};
    use sources_core::cdc::{Ack, AckSink, Change, ChangeEvent};
    use sources_core::document::{Document, DocumentBuilder, DocumentId, IndexScope};
    use sources_core::{RowKey, SnapshotTable};
    use tokio::sync::Notify;

    use crate::observer::StatusObserver;
    use crate::status::{IndexState, Phase};

    fn users() -> IndexName {
        IndexName::try_new("users").unwrap()
    }

    /// The observer drives the status surface through a full lifecycle, and the
    /// snapshot serializes to the expected JSON shape.
    #[test]
    fn observer_drives_status_through_its_lifecycle() {
        let status = Arc::new(Status::new([users()], Instant::now()));
        let observer = StatusObserver::new(Arc::clone(&status));

        // Starts pending, before any events.
        let snap = status.snapshot();
        assert_eq!(snap.phase, Phase::Starting);
        assert_eq!(snap.indexes.get("users"), Some(&IndexState::Pending));

        observer.on_indexes_ensured(1);
        observer.on_backfill_started(&[users()]);
        let snap = status.snapshot();
        assert_eq!(snap.phase, Phase::Backfilling);
        assert_eq!(snap.indexes.get("users"), Some(&IndexState::Backfilling));

        observer.on_index_seeded(&users());
        observer.on_backfill_completed();
        observer.on_live_started();

        // Three changes captured, two distinct documents built in one batch.
        observer.on_change_captured();
        observer.on_change_captured();
        observer.on_change_captured();
        observer.on_batch_committed(BatchStats {
            changes: 3,
            documents: 2,
            documents_by_index: vec![(users(), 2)],
            flush: Duration::from_millis(5),
        });
        observer.on_slot_lag(4096);

        let snap = status.snapshot();
        assert_eq!(snap.phase, Phase::Live);
        assert_eq!(snap.indexes.get("users"), Some(&IndexState::Seeded));
        assert_eq!(snap.changes_captured, 3);
        assert_eq!(snap.changes_committed, 3);
        assert_eq!(snap.changes_in_flight, 0);
        assert_eq!(snap.documents_built, 2);
        assert_eq!(snap.batches, 1);
        assert_eq!(snap.slot_lag_bytes, Some(4096));
        assert_eq!(snap.errors, 0);

        // The JSON the `/status` endpoint returns.
        let json = serde_json::to_value(&snap).unwrap();
        assert_eq!(json["phase"], "live");
        assert_eq!(json["indexes"]["users"], "seeded");
        assert_eq!(json["changes_in_flight"], 0);
        assert_eq!(json["slot_lag_bytes"], 4096);
    }

    /// Reaching live with a never-backfilled index (already seeded on start)
    /// still reports it seeded, and an error moves the phase to `Stopped`.
    #[test]
    fn already_seeded_index_and_error_phase() {
        let status = Arc::new(Status::new([users()], Instant::now()));
        let observer = StatusObserver::new(Arc::clone(&status));

        // No backfill_started for `users` — it was already seeded.
        observer.on_live_started();
        assert_eq!(
            status.snapshot().indexes.get("users"),
            Some(&IndexState::Seeded),
            "an index live without a backfill this run is reported seeded",
        );

        observer.on_error("boom");
        let snap = status.snapshot();
        assert_eq!(snap.phase, Phase::Stopped);
        assert_eq!(snap.errors, 1);
        assert_eq!(snap.last_error.as_deref(), Some("boom"));
    }

    /// A source that reports a fixed lag and an empty live stream.
    #[derive(Debug)]
    struct LaggySource(Option<u64>);

    #[async_trait]
    impl ChangeCapture for LaggySource {
        async fn live(
            &self,
        ) -> sources_core::Result<BoxStream<'static, sources_core::Result<Change>>> {
            Ok(Box::pin(stream::empty()))
        }

        async fn lag(&self) -> sources_core::Result<Option<u64>> {
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
    // supplying a `Backends` that hands back test doubles — the seam the
    // pluggable-backends refactor exists to enable.

    /// A `Backends` that returns pre-built test doubles, ignoring the `Config`.
    /// Counts how often each edge was asked for, to prove the daemon builds its
    /// backends *through* the seam rather than naming any concrete one.
    #[derive(Debug)]
    struct MockBackends {
        capture: Arc<dyn ChangeCapture>,
        documents: Arc<dyn DocumentBuilder>,
        sink: Arc<dyn Sink>,
        source_built: Arc<AtomicU64>,
        sink_built: Arc<AtomicU64>,
    }

    #[async_trait]
    impl Backends for MockBackends {
        async fn source(
            &self,
            _config: Arc<Config>,
            _options: &DaemonOptions,
        ) -> anyhow::Result<SourceParts> {
            self.source_built.fetch_add(1, Ordering::SeqCst);
            Ok(SourceParts {
                capture: Arc::clone(&self.capture),
                documents: Arc::clone(&self.documents),
            })
        }

        async fn sink(
            &self,
            _config: &Config,
            _options: &DaemonOptions,
        ) -> anyhow::Result<Arc<dyn Sink>> {
            self.sink_built.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::clone(&self.sink))
        }
    }

    /// Replays a fixed list of changes on the live stream, once, then ends — so
    /// `engine.run()` completes on its own without a shutdown signal.
    #[derive(Debug)]
    struct ScriptedSource {
        changes: Mutex<Option<Vec<Change>>>,
    }

    #[async_trait]
    impl ChangeCapture for ScriptedSource {
        async fn live(
            &self,
        ) -> sources_core::Result<BoxStream<'static, sources_core::Result<Change>>> {
            let changes = self.changes.lock().unwrap().take().unwrap_or_default();
            Ok(Box::pin(stream::iter(
                changes
                    .into_iter()
                    .map(Ok::<Change, sources_core::SourceError>),
            )))
        }

        async fn lag(&self) -> sources_core::Result<Option<u64>> {
            Ok(None)
        }
    }

    /// Resolves each change to one `users` document; key value `2` is a delete.
    #[derive(Debug)]
    struct ScriptedDocuments;

    #[async_trait]
    impl DocumentBuilder for ScriptedDocuments {
        async fn resolve(
            &self,
            _table: &TableName,
            key: &RowKey,
        ) -> sources_core::Result<Vec<DocumentId>> {
            Ok(vec![DocumentId {
                index: users(),
                key: key.clone(),
            }])
        }

        async fn build(&self, id: &DocumentId) -> sources_core::Result<Document> {
            let deleted = matches!(id.key.0.first(), Some((_, GenericValue::Int(2))));
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
    }

    /// Records the sink ops it receives, in order.
    #[derive(Debug, Default)]
    struct RecordingSink {
        ops: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl Sink for RecordingSink {
        async fn upsert(
            &self,
            index: &IndexName,
            id: &str,
            _document: &GenericValue,
        ) -> sinks_core::Result<()> {
            self.ops
                .lock()
                .unwrap()
                .push(format!("upsert {} {id}", index.as_ref()));
            Ok(())
        }

        async fn delete(&self, index: &IndexName, id: &str) -> sinks_core::Result<()> {
            self.ops
                .lock()
                .unwrap()
                .push(format!("delete {} {id}", index.as_ref()));
            Ok(())
        }

        async fn flush(&self, _caught_up: bool) -> sinks_core::Result<FlushReport> {
            Ok(FlushReport::clean())
        }
    }

    /// Counts the changes confirmed back to the source.
    #[derive(Debug)]
    struct CountingAck(Arc<AtomicU64>);

    impl AckSink for CountingAck {
        fn confirm(&self, _seq: u64) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn row_change(delete: bool, id: i64, seq: u64, acks: &Arc<AtomicU64>) -> Change {
        let table = TableName::try_new("users").unwrap();
        let key = RowKey(vec![(
            ColumnName::try_new("id").unwrap(),
            GenericValue::Int(id),
        )]);
        let event = if delete {
            ChangeEvent::Delete { table, key }
        } else {
            ChangeEvent::Upsert { table, key }
        };
        Change {
            event,
            ack: Ack::new(seq, Arc::new(CountingAck(Arc::clone(acks)))),
        }
    }

    /// A config the `MockBackends` ignores — only `indexes` is read by the
    /// daemon (for the status surface), and it's intentionally empty.
    fn backendless_config() -> Config {
        Config {
            source: Source {
                source_type: SourceType::Postgres,
                connection: None,
            },
            sinks: BTreeMap::new(),
            indexes: BTreeMap::new(),
            on_error: Default::default(),
            server: Default::default(),
        }
    }

    fn daemon_over(backends: Arc<MockBackends>) -> Daemon {
        Daemon::new(backendless_config(), backends).with_options(DaemonOptions {
            // No backfill: the test drives the live path directly.
            skip_backfill: true,
            ..DaemonOptions::default()
        })
    }

    /// `Daemon::start` builds both edges **through** the injected `Backends`
    /// (never naming a concrete backend itself), and a run over an empty live
    /// stream returns cleanly with the status surface left at `Stopped`.
    #[tokio::test]
    async fn start_builds_backends_through_the_seam() {
        let source_built = Arc::new(AtomicU64::new(0));
        let sink_built = Arc::new(AtomicU64::new(0));

        let backends = Arc::new(MockBackends {
            capture: Arc::new(ScriptedSource {
                changes: Mutex::new(Some(Vec::new())),
            }),
            documents: Arc::new(ScriptedDocuments),
            sink: Arc::new(RecordingSink::default()),
            source_built: Arc::clone(&source_built),
            sink_built: Arc::clone(&sink_built),
        });

        let running = daemon_over(backends).start().await.unwrap();
        let status = running.status();

        // The daemon asked the seam — not a hardcoded backend — for each edge.
        assert_eq!(source_built.load(Ordering::SeqCst), 1);
        assert_eq!(sink_built.load(Ordering::SeqCst), 1);

        // An empty live stream completes on its own; the shutdown never fires.
        running.run(std::future::pending::<()>()).await.unwrap();

        let snap = status.snapshot();
        assert_eq!(snap.phase, Phase::Stopped);
        assert_eq!(snap.changes_committed, 0);
    }

    /// A run over a non-empty live stream drives changes all the way through the
    /// injected document builder and sink — capture, build, write, flush, ack —
    /// with no real source or sink, and the status counters reflect it.
    #[tokio::test]
    async fn drives_changes_through_injected_backends() {
        let acks = Arc::new(AtomicU64::new(0));
        let ops = Arc::new(Mutex::new(Vec::new()));

        let changes = vec![
            row_change(false, 1, 0, &acks),
            row_change(true, 2, 1, &acks),
        ];

        let backends = Arc::new(MockBackends {
            capture: Arc::new(ScriptedSource {
                changes: Mutex::new(Some(changes)),
            }),
            documents: Arc::new(ScriptedDocuments),
            sink: Arc::new(RecordingSink {
                ops: Arc::clone(&ops),
            }),
            source_built: Arc::new(AtomicU64::new(0)),
            sink_built: Arc::new(AtomicU64::new(0)),
        });

        let running = daemon_over(backends).start().await.unwrap();
        let status = running.status();
        running.run(std::future::pending::<()>()).await.unwrap();

        assert_eq!(
            *ops.lock().unwrap(),
            vec!["upsert users 1".to_owned(), "delete users 2".to_owned()],
        );
        assert_eq!(acks.load(Ordering::SeqCst), 2, "both changes acked");

        let snap = status.snapshot();
        assert_eq!(snap.changes_captured, 2);
        assert_eq!(snap.changes_committed, 2);
        assert_eq!(snap.changes_in_flight, 0);
        assert_eq!(snap.phase, Phase::Stopped);
    }
}

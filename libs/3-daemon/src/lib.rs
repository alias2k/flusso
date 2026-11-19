#![doc = include_str!("../README.md")]
#![cfg_attr(test, allow(unused_crate_dependencies))]

mod backends;
mod fence;
mod lag;
mod observer;
mod stall;
pub mod status;
mod supervise;

pub use backends::{Backends, SinkParts, SourceParts};
pub use engine::{Budget, Held, Progress};
pub use fence::{
    Fence, FenceError, FenceLevel, FenceObserver, FenceOutcome, FencePositions, FenceRequest,
    LanePositions, SinkPositions, Unserved,
};
pub use observer::StatusObserver;
pub use stall::{LaneStall, SAMPLE_EVERY, STALL_WARN, StallEvent, Stalls};
pub use status::{
    IndexState, LastScriptFailure, Phase, ScriptFailures, SinkPhase, SinkSnapshot, Status,
    StatusSnapshot,
};
pub use supervise::{ControlError, DaemonControl};

use supervise::ControlReceivers;

pub use engine::{
    BuildStats, CommitStats, EngineId, Observer, Stage, StageTiming, StartupStep, Wake, Work,
};
pub use kernel::{IndexName, SinkName};

use std::future::Future;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use config::Config;
use engine::{BatchPolicy, FailurePolicies, FanOut, IngestEngine, SinkEngine};
use kernel::IndexMapping;
use source::cdc::{ChangeCapture, Continuity};
use stream::{LaneKey, Stream};

/// How a [`Daemon`] run is parameterized: the knobs that belong to the
/// deployment as a whole. Adapter settings (the slot, the publication,
/// pretty-printing, the channel capacity, …) live in each port entry's options
/// and reach the adapter through [`Backends`]; transport settings (HTTP address,
/// …) are the binary's.
#[derive(Debug, Clone)]
pub struct DaemonOptions {
    /// Skip every backfill and follow live changes only.
    pub skip_backfill: bool,
    /// How often to sample source capture lag.
    pub lag_poll_interval: Duration,
    /// The longest pause between restarts of a failed engine, and between two
    /// probes of a parked lane. Backoff starts at one second and doubles up to
    /// this.
    pub max_restart_backoff: Duration,
}

impl Default for DaemonOptions {
    fn default() -> Self {
        Self {
            skip_backfill: false,
            lag_poll_interval: Duration::from_secs(15),
            max_restart_backoff: Duration::from_secs(60),
        }
    }
}

/// The engine's batch policy: the defaults with whatever `[batch]` set laid
/// over them.
pub fn batch_policy(config: &Config) -> BatchPolicy {
    let defaults = BatchPolicy::default();
    BatchPolicy {
        max_changes: config
            .batch
            .max_changes
            .map_or(defaults.max_changes, NonZeroUsize::get),
        max_delay: config
            .batch
            .max_delay_ms
            .map_or(defaults.max_delay, Duration::from_millis),
    }
}

/// Lane items each sink writes at once when `[concurrency] writes` is unset.
/// End to end on `reference` (#408) backfill ran about 1.4× one write at 2
/// and 4, slower at 8; the 3× of ADR 0012 was raw OpenSearch bulk throughput
/// in its prototype.
pub const DEFAULT_WRITES: usize = 4;

/// Lane items each sink writes at once: `[concurrency] writes`, else
/// [`DEFAULT_WRITES`].
pub fn writes(config: &Config) -> usize {
    config
        .concurrency
        .writes
        .map_or(DEFAULT_WRITES, NonZeroUsize::get)
}

/// Document builds the ingest engine runs at once when `[concurrency] builds`
/// is unset. End to end on `reference` (ADR 0012 stage 4) a backfill ran about
/// 15% faster at 2 and 4 than at 1, then the sink's writes bound it; the
/// prototype's 3× at 4 built with no sink behind it.
pub const DEFAULT_BUILDS: usize = 4;

/// Document builds the ingest engine runs at once: `[concurrency] builds`,
/// else [`DEFAULT_BUILDS`].
pub fn builds(config: &Config) -> usize {
    config
        .concurrency
        .builds
        .map_or(DEFAULT_BUILDS, NonZeroUsize::get)
}

/// MiB of built documents held when `[concurrency] max_built_mib` is unset
/// (ADR 0012 §5). A smaller budget saves memory but lengthens the tail of
/// visible latency (`stage-5.md`, "The default").
pub const DEFAULT_MAX_BUILT_MIB: usize = 512;

/// Bytes of built documents the ingest engine holds before it waits:
/// `[concurrency] max_built_mib`, else [`DEFAULT_MAX_BUILT_MIB`].
pub fn max_built_bytes(config: &Config) -> usize {
    config
        .concurrency
        .max_built_mib
        .map_or(DEFAULT_MAX_BUILT_MIB, NonZeroUsize::get)
        .saturating_mul(1024 * 1024)
}

/// A configured-but-not-yet-running deployment over one [`Config`].
#[derive(Debug)]
pub struct Daemon {
    config: Config,
    options: DaemonOptions,
    backends: Arc<dyn Backends>,
    extra_observers: Vec<Arc<dyn Observer>>,
}

impl Daemon {
    /// A deployment over `config`, built through `backends`, with default
    /// [`DaemonOptions`].
    pub fn new(config: Config, backends: Arc<dyn Backends>) -> Self {
        Self {
            config,
            options: DaemonOptions::default(),
            backends,
            extra_observers: Vec::new(),
        }
    }

    /// Set the deployment-wide knobs.
    pub fn with_options(mut self, options: DaemonOptions) -> Self {
        self.options = options;
        self
    }

    /// Attach an observer (a metrics recorder, a log) beside the status
    /// observer the daemon always wires.
    pub fn with_observer(mut self, observer: Arc<dyn Observer>) -> Self {
        self.extra_observers.push(observer);
        self
    }

    /// Build every adapter through the seam, read the source's continuity, and
    /// return a [`RunningDaemon`] whose [`status`](RunningDaemon::status) and
    /// [`control`](RunningDaemon::control) a transport can use while it runs.
    ///
    /// If an attached observer records to the global OpenTelemetry meter,
    /// install a meter provider *before* calling this; otherwise its
    /// instruments are no-ops.
    #[tracing::instrument(name = "daemon.start", skip_all)]
    pub async fn start(self) -> anyhow::Result<RunningDaemon> {
        let Daemon {
            config,
            options,
            backends,
            extra_observers,
        } = self;

        backends.validate(&config)?;
        tracing::info!(
            source = %config.source.kind,
            stream = %config.stream.kind,
            sinks = config.sinks.len(),
            indexes = config.indexes.len(),
            "starting deployment",
        );

        let config = Arc::new(config);
        let source_started = Instant::now();
        let SourceParts {
            capture,
            documents,
            resolver,
        } = backends.source(Arc::clone(&config), &options).await?;
        let source_took = source_started.elapsed();
        let sinks = backends.sinks(&config).await?;
        let sink_names: Vec<SinkName> = sinks.iter().map(|s| s.name.clone()).collect();
        let mappings: Vec<IndexMapping> = documents
            .index_mappings()
            .await
            .context("resolving the index mappings")?;
        // One lane per (index, sink): every index a sink engine maintains.
        let lanes: Vec<LaneKey> = mappings
            .iter()
            .flat_map(|mapping| {
                sink_names
                    .iter()
                    .map(|sink| LaneKey::new(mapping.index.clone(), sink.clone()))
            })
            .collect();
        let stream: Arc<dyn Stream> = backends.stream(&config, &lanes)?;

        let progress = Arc::new(Progress::new(lanes.iter().cloned()));
        let fence = Fence::new(
            Arc::clone(&capture),
            Arc::clone(&progress),
            lanes.iter().cloned(),
        );
        let budget = Arc::new(Budget::new(max_built_bytes(&config)));
        let stalls = Arc::new(Stalls::new(lanes.iter().cloned()));
        let status = Arc::new(
            Status::new(
                config.indexes.keys().cloned(),
                sink_names.iter().cloned(),
                Instant::now(),
            )
            .with_fence(fence.clone())
            .with_source(Arc::clone(&capture))
            .with_budget(Arc::clone(&budget))
            .with_stalls(Arc::clone(&stalls)),
        );
        let mut observers: Vec<Arc<dyn Observer>> = vec![
            Arc::new(StatusObserver::new(Arc::clone(&status))),
            Arc::new(FenceObserver::new(fence.clone())),
        ];
        observers.extend(extra_observers);
        let observer: Arc<dyn Observer> = Arc::new(FanOut::new(observers));
        observer.on_startup_step(StartupStep::Source, None, source_took);

        let mut failure_policies = FailurePolicies::new(config.on_error);
        for (name, index) in &config.indexes {
            if let Some(policy) = index.on_error {
                failure_policies = failure_policies.with_override(name.as_ref(), policy);
            }
        }

        // Read before anything is staged: a missing resume point means every
        // seed the sinks hold is stale, and each sink engine stages its
        // rebuilds before the ingest engine creates the point.
        let continuity = capture.continuity().await?;

        let stop = engine::Stop::new();
        let ingest = IngestEngine::new(
            Arc::clone(&capture),
            Arc::clone(&documents),
            Arc::clone(&stream),
            sink_names.clone(),
            Arc::clone(&progress),
        )
        .with_resolver(resolver)
        .with_batch(batch_policy(&config))
        .with_builds(builds(&config))
        .with_budget(Arc::clone(&budget))
        .with_observer(Arc::clone(&observer))
        .with_failure_policies(failure_policies.clone())
        .with_stop(stop.clone());

        let sink_engines: Vec<SinkEngine> = sinks
            .into_iter()
            .map(|parts| {
                SinkEngine::new(
                    parts.name,
                    parts.sink,
                    Arc::clone(&stream),
                    mappings.clone(),
                    Arc::clone(&progress),
                )
                .with_options(parts.options)
                .with_writes(writes(&config))
                .with_write_documents(batch_policy(&config).max_changes)
                .with_merge_wait(batch_policy(&config).max_delay)
                .with_max_probe_backoff(options.max_restart_backoff)
                .with_observer(Arc::clone(&observer))
                .with_failure_policies(failure_policies.clone())
                .skip_backfill(options.skip_backfill)
                .with_stop(stop.clone())
            })
            .collect();

        let (control, control_receivers) = DaemonControl::new(&sink_names, stop.clone());
        Ok(RunningDaemon {
            status,
            fence,
            control,
            control_receivers,
            ingest,
            sink_engines,
            continuity,
            source: capture,
            stream,
            observer,
            options,
            stop,
            budget,
            stalls,
            progress,
        })
    }
}

/// A started deployment: engines built, nothing running yet, so a transport
/// the binary owns can take its handles before [`run`](RunningDaemon::run).
#[derive(Debug)]
pub struct RunningDaemon {
    status: Arc<Status>,
    fence: Fence,
    control: DaemonControl,
    control_receivers: ControlReceivers,
    ingest: IngestEngine,
    sink_engines: Vec<SinkEngine>,
    continuity: Continuity,
    source: Arc<dyn ChangeCapture>,
    stream: Arc<dyn Stream>,
    observer: Arc<dyn Observer>,
    options: DaemonOptions,
    stop: engine::Stop,
    budget: Arc<Budget>,
    stalls: Arc<Stalls>,
    progress: Arc<Progress>,
}

impl RunningDaemon {
    /// A handle to the live operational status, for a transport (HTTP, a TUI, …)
    /// to read while the daemon runs. Cheap to clone.
    pub fn status(&self) -> Arc<Status> {
        Arc::clone(&self.status)
    }

    /// The memory budget the ingest engine builds within, for a transport to
    /// report (see [`Budget`]). Cheap to clone.
    pub fn budget(&self) -> Arc<Budget> {
        Arc::clone(&self.budget)
    }

    /// The fence: what a transport waits on so a client can read its own
    /// write (see [`Fence`]). Cheap to clone.
    pub fn fence(&self) -> Fence {
        self.fence.clone()
    }

    /// The operations handle: what a transport calls to act on the running
    /// deployment (a reindex). Cheap to clone.
    pub fn control(&self) -> DaemonControl {
        self.control.clone()
    }

    /// Supervise every engine until the live stream ends, or `shutdown`
    /// resolves (typically a signal future the binary owns) and the
    /// deployment has stopped gracefully. A failed engine restarts with
    /// backoff while the others keep running.
    ///
    /// The stop: the phase turns `stopping` (not ready, no new operation);
    /// the ingest engine reads nothing more, publishes the batch it was
    /// buffering, waits until every sink flushed and acknowledged its lane,
    /// hands what every lane landed to the source and closes the live stream.
    /// A snapshot in progress is abandoned; its sink asks again on the next
    /// start. Nothing here bounds the stop: a stalled sink holds it, so the
    /// caller owns the deadline, and dropping this future past it is safe
    /// (at-least-once: whatever was not acknowledged is read again).
    #[tracing::instrument(name = "daemon.run", skip_all)]
    pub async fn run(self, shutdown: impl Future<Output = ()> + Send) -> anyhow::Result<()> {
        let RunningDaemon {
            status,
            fence,
            control: _,
            control_receivers,
            ingest,
            sink_engines,
            continuity,
            source,
            stream,
            observer,
            options,
            stop,
            budget: _,
            stalls,
            progress,
        } = self;

        let _lag = LagGuard(tokio::spawn(lag::poll(
            Arc::clone(&source),
            Arc::clone(&observer),
            options.lag_poll_interval,
        )));

        let _stalls = LagGuard(tokio::spawn(stall::watch(
            stalls,
            progress,
            Arc::clone(&source),
            Arc::clone(&status),
        )));

        let supervised = supervise::run_all(
            ingest,
            sink_engines,
            control_receivers,
            continuity,
            &source,
            &stream,
            &options,
            &stop,
            &fence,
        );
        tokio::pin!(supervised);
        let result = tokio::select! {
            res = &mut supervised => res,
            () = shutdown => {
                tracing::info!("shutdown requested; stopping the deployment gracefully");
                status.set_phase(Phase::Stopping);
                stop.request();
                supervised.await
            }
        };

        status.set_phase(Phase::Stopped);
        result
    }
}

/// Aborts the lag poller when dropped — on a normal stop or on cancellation
/// alike. Its result is discarded, so there's nothing to join.
#[derive(Debug)]
struct LagGuard(tokio::task::JoinHandle<()>);

impl Drop for LagGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;

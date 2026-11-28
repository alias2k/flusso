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
pub use engine::{Backoff, Budget, Held, MaxWaiting, Pin, Progress};
pub use fence::FenceGap;
pub use fence::{
    Fence, FenceError, FenceLevel, FenceObserver, FenceOutcome, FencePositions, FenceRequest,
    LanePositions, SinkPositions, Unserved,
};
pub use observer::StatusObserver;
pub use stall::{BACKLOG_SPAN, LaneStall, SAMPLE_EVERY, STALL_WARN, StallEvent, Stalls};
pub use status::LaneGap;
pub use status::{
    BackfillSnapshot, BackfillState, DetachedLane, Exemption, IndexErrors, IndexFailure, IndexHold,
    IndexIngestSnapshot, IndexRestarts, IndexState, IngestSnapshot, IngestState, LaneHealthState,
    LaneHealthView, LaneSnapshot, LastScriptFailure, Phase, PinnedBy, ScriptFailures, SinkPhase,
    SinkSnapshot, Status, StatusSnapshot,
};
pub use supervise::{ControlError, DaemonControl};

use supervise::ControlReceivers;

pub use engine::{
    BuildStats, CommitStats, EngineId, ExemptionState, IndexStates, LaneHealth, Observer,
    ParkReason, ProbeOutcome, ReaderBound, ReattachVerdict, Reply, SinkControl, Stage, StageTiming,
    StartupStep, Stop, Wake, Work,
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
}

impl Default for DaemonOptions {
    fn default() -> Self {
        Self {
            skip_backfill: false,
            lag_poll_interval: Duration::from_secs(15),
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

/// How long the engines wait before trying again, resolved and checked: the
/// one judge `run`, `check`, `build` and [`Daemon::start`] share.
///
/// - `initial_ms`: the value set (flag > env > file), else 1000.
/// - `max_ms`: the value set, else 60000 or `initial_ms`, whichever is larger,
///   so raising the first wait alone is always valid.
/// - A `max_ms` set below `initial_ms` is refused; the message says where each
///   value came from and how to change it. Equal values wait a constant
///   interval.
pub fn backoff(config: &Config) -> anyhow::Result<Backoff> {
    let defaults = Backoff::default();
    let initial = config
        .backoff
        .initial_ms
        .map_or(defaults.initial(), |value| {
            Duration::from_millis(value.ms.get())
        });
    let max = match config.backoff.max_ms {
        Some(value) => Duration::from_millis(value.ms.get()),
        None => defaults.max().max(initial),
    };
    Backoff::new(initial, max).map_err(|_| {
        anyhow::anyhow!(
            "[backoff] max_ms = {} ({}) is below initial_ms = {} ({}): the longest wait must be \
             at least the first (equal values wait a constant interval). Raise max_ms with {} or \
             lower initial_ms with {}; a flag wins over its variable, a variable over the file",
            max.as_millis(),
            origin(config.backoff.max_ms, "max_ms"),
            initial.as_millis(),
            origin(config.backoff.initial_ms, "initial_ms"),
            ways("max_ms"),
            ways("initial_ms"),
        )
    })
}

/// Where a `[backoff]` value came from, in the words a user set it with.
fn origin(value: Option<config::BackoffValue>, key: &str) -> String {
    match value.map(|value| value.origin) {
        None => "the default".to_owned(),
        Some(config::Origin::File) => format!("set in flusso.toml's [backoff] {key}"),
        Some(config::Origin::Flag) => format!("set by {}", flag(key)),
        Some(config::Origin::Env) => format!("set by {}", variable(key)),
    }
}

/// The share of `max_built_mib` one index's live documents may hold when
/// `[concurrency] index_budget_share` is unset.
pub const DEFAULT_INDEX_BUDGET_SHARE: f64 = 0.5;

/// The share of the memory budget one index's live documents may hold while two or more
/// indexes run, resolved and checked: `[concurrency] index_budget_share` as
/// set (flag > env > file), else [`DEFAULT_INDEX_BUDGET_SHARE`]. A value
/// outside (0, 1] (`nan` included) is refused, the message saying where it
/// came from and how to change it. The one judge `run`, `check`, `build` and
/// [`Daemon::start`] share.
pub fn index_budget_share(config: &Config) -> anyhow::Result<f64> {
    let Some(value) = config.concurrency.index_budget_share else {
        return Ok(DEFAULT_INDEX_BUDGET_SHARE);
    };
    if value.share > 0.0 && value.share <= 1.0 {
        return Ok(value.share);
    }
    let set = match value.origin {
        config::Origin::File => "set in flusso.toml's [concurrency] index_budget_share",
        config::Origin::Flag => "set by --index-budget-share",
        config::Origin::Env => "set by FLUSSO_INDEX_BUDGET_SHARE",
    };
    anyhow::bail!(
        "[concurrency] index_budget_share = {} ({set}) is outside (0, 1]: it is the share of \
         max_built_mib one index's live documents may hold while two or more indexes run. Set a number above 0 \
         and at most 1 (default 0.5) with flusso.toml's [concurrency] index_budget_share, \
         --index-budget-share or FLUSSO_INDEX_BUDGET_SHARE; a flag wins over its variable, a \
         variable over the file",
        value.share
    )
}

/// How far the ingest engine's reader may read ahead of its builds (ADR 0012
/// stage 8a, slice 6a), resolved and checked: `[concurrency]
/// max_waiting_per_index` and `max_waiting` as set (flag > env > file), each
/// unset one derived from `[batch] max_changes` and `builds`
/// ([`MaxWaiting::derived`]: 512 and 2,048 at the defaults), then kept in
/// order as `[backoff]` keeps its pair: an unset `max_waiting` rises to
/// `max_waiting_per_index`, an unset `max_waiting_per_index` falls to
/// `max_waiting`, so setting one alone is always valid. Refused: either at
/// 0, or both set with `max_waiting` below `max_waiting_per_index`; the
/// message says where each value came from and how to change it. The one
/// judge `run`, `check`, `build` and [`Daemon::start`] share.
pub fn max_waiting(config: &Config) -> anyhow::Result<MaxWaiting> {
    let derived = MaxWaiting::derived(batch_policy(config), builds(config));
    let per_index = config.concurrency.max_waiting_per_index;
    let total = config.concurrency.max_waiting;
    for (value, key) in [(per_index, "max_waiting_per_index"), (total, "max_waiting")] {
        if let Some(value) = value
            && value.count == 0
        {
            anyhow::bail!(
                "[concurrency] {key} = 0 ({}) would stop the reading for good: it is how many \
                 documents may wait to be built before flusso stops reading changes. Set at \
                 least 1 with {}; a flag wins over its variable, a variable over the file",
                waiting_origin(value, key),
                waiting_ways(key),
            );
        }
    }
    match (per_index, total) {
        (Some(per_index), Some(total)) if total.count < per_index.count => anyhow::bail!(
            "[concurrency] max_waiting = {} ({}) is below max_waiting_per_index = {} ({}): the \
             documents every index together may have waiting can't be fewer than one index's. \
             Raise max_waiting with {} or lower max_waiting_per_index with {}, or leave one \
             unset (it then follows the other); a flag wins over its variable, a variable over \
             the file",
            total.count,
            waiting_origin(total, "max_waiting"),
            per_index.count,
            waiting_origin(per_index, "max_waiting_per_index"),
            waiting_ways("max_waiting"),
            waiting_ways("max_waiting_per_index"),
        ),
        (Some(per_index), Some(total)) => Ok(MaxWaiting {
            per_index: count(per_index.count),
            total: count(total.count),
        }),
        (Some(per_index), None) => {
            let per_index = count(per_index.count);
            Ok(MaxWaiting {
                per_index,
                total: derived.total.max(per_index),
            })
        }
        (None, Some(total)) => {
            let total = count(total.count);
            Ok(MaxWaiting {
                per_index: derived.per_index.min(total),
                total,
            })
        }
        (None, None) => Ok(derived),
    }
}

/// How many documents held indexes together may have waiting before reading
/// stops for every index (ADR 0012 stage 8a, slice 6b-2): `[concurrency]
/// max_pending_documents` as set (flag > env > file), else
/// [`engine::DEFAULT_MAX_PENDING_DOCUMENTS`]. Refused at 0, naming where it
/// came from and how to change it. The one judge `run`, `check`, `build` and
/// [`Daemon::start`] share.
pub fn max_pending_documents(config: &Config) -> anyhow::Result<usize> {
    match config.concurrency.max_pending_documents {
        Some(value) if value.count == 0 => anyhow::bail!(
            "[concurrency] max_pending_documents = 0 ({}) would stop the reading as soon as an \
             index is held: it is how many documents held indexes together may have waiting \
             before flusso stops reading changes for every index. Set at least 1 with {}; a flag \
             wins over its variable, a variable over the file",
            waiting_origin(value, "max_pending_documents"),
            waiting_ways("max_pending_documents"),
        ),
        Some(value) => Ok(count(value.count)),
        None => Ok(engine::DEFAULT_MAX_PENDING_DOCUMENTS),
    }
}

/// A count as the engine takes it (a larger one than memory could ever hold
/// is the same as no bound).
fn count(value: u64) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

/// Where a `[concurrency]` count came from, in the words a user set it with.
fn waiting_origin(value: config::CountValue, key: &str) -> String {
    match value.origin {
        config::Origin::File => format!("set in flusso.toml's [concurrency] {key}"),
        config::Origin::Flag => format!("set by --{}", key.replace('_', "-")),
        config::Origin::Env => format!("set by FLUSSO_{}", key.to_ascii_uppercase()),
    }
}

/// Every way to set a `[concurrency]` count.
fn waiting_ways(key: &str) -> String {
    format!(
        "flusso.toml's [concurrency] {key}, --{} or FLUSSO_{}",
        key.replace('_', "-"),
        key.to_ascii_uppercase()
    )
}

/// Every way to set a `[backoff]` key.
fn ways(key: &str) -> String {
    format!(
        "flusso.toml's [backoff] {key}, {} or {}",
        flag(key),
        variable(key)
    )
}

fn flag(key: &str) -> String {
    format!("--backoff-{}", key.replace('_', "-"))
}

fn variable(key: &str) -> String {
    format!("FLUSSO_BACKOFF_{}", key.to_uppercase())
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
        let backoff = backoff(&config)?;
        let index_share = index_budget_share(&config)?;
        let max_waiting = max_waiting(&config)?;
        let max_pending_documents = max_pending_documents(&config)?;
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
        // O29: the ingest engine counts the slot's openings, the sink engines
        // read their records again after each, and the fence vouches for no
        // sink that hasn't.
        let rereads = Arc::new(engine::Rereads::default());
        progress.hold_while_unread(Arc::clone(&rereads));
        let fence = Fence::new(
            Arc::clone(&capture),
            Arc::clone(&progress),
            lanes.iter().cloned(),
        )
        .with_rereads(Arc::clone(&rereads));
        // Shared per index only with two or more: `mappings` are the
        // indexes that have lanes, the ones a build can hold bytes for.
        let budget = Arc::new(
            Budget::new(max_built_bytes(&config)).with_index_share(mappings.len(), index_share),
        );
        let stalls = Arc::new(Stalls::new(lanes.iter().cloned()));
        let status = Arc::new(
            Status::new(
                config.indexes.keys().cloned(),
                sink_names.iter().cloned(),
                Instant::now(),
            )
            .with_ingest_indexes(
                config
                    .indexes
                    .iter()
                    .filter(|(_, index)| index.enabled)
                    .map(|(name, _)| name.clone()),
            )
            .with_fence(fence.clone())
            .with_progress(Arc::clone(&progress))
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
        .with_max_waiting(max_waiting)
        .with_max_pending_documents(max_pending_documents)
        .with_budget(Arc::clone(&budget))
        .with_observer(Arc::clone(&observer))
        .with_failure_policies(failure_policies.clone())
        .with_backoff(backoff)
        .with_stop(stop.clone())
        .with_rereads(Arc::clone(&rereads));
        status.attach_index_states(ingest.index_states());

        let live_only: Vec<SinkName> = sinks
            .iter()
            .filter(|parts| !parts.options.backfill)
            .map(|parts| parts.name.clone())
            .collect();
        let positions: Arc<dyn engine::SourcePositions> =
            Arc::new(engine::CapturePositions(Arc::clone(&capture)));
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
                .with_backoff(backoff)
                .with_observer(Arc::clone(&observer))
                .with_failure_policies(failure_policies.clone())
                .skip_backfill(options.skip_backfill)
                .with_stop(stop.clone())
                .with_positions(Arc::clone(&positions))
                .with_rereads(Arc::clone(&rereads))
            })
            .collect();

        let (control, control_receivers) = DaemonControl::new(&sink_names, stop.clone());
        let control = control.with_live_only(live_only);
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
            backoff,
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
    backoff: Backoff,
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

    /// How far each lane has landed what was routed to it, and how many
    /// changes each sink has committed (see [`Progress`]). Cheap to clone.
    pub fn progress(&self) -> Arc<Progress> {
        Arc::clone(&self.progress)
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
            backoff,
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
            backoff,
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

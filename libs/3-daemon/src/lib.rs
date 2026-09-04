#![doc = include_str!("../README.md")]

mod backends;
mod lag;
mod observer;
pub mod status;
mod supervise;

pub use backends::{Backends, SinkParts, SourceParts};
pub use observer::StatusObserver;
pub use status::{IndexState, Phase, SinkPhase, SinkSnapshot, Status, StatusSnapshot};
pub use supervise::{ControlError, DaemonControl};

use supervise::ControlReceivers;

pub use engine::{BuildStats, CommitStats, EngineId, Observer};
pub use kernel::{IndexName, SinkName};

use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use config::Config;
use engine::{FailurePolicies, FanOut, IngestEngine, SinkEngine};
use kernel::IndexMapping;
use source::cdc::{ChangeCapture, Continuity};
use stream::Stream;

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
    /// The longest pause between restarts of a failed engine. Backoff starts
    /// at one second and doubles up to this.
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
        let SourceParts { capture, documents } =
            backends.source(Arc::clone(&config), &options).await?;
        let sinks = backends.sinks(&config).await?;
        let sink_names: Vec<SinkName> = sinks.iter().map(|s| s.name.clone()).collect();
        let stream: Arc<dyn Stream> = backends.stream(&config, &sink_names)?;
        let mappings: Vec<IndexMapping> = documents
            .index_mappings()
            .await
            .context("resolving the index mappings")?;

        let status = Arc::new(Status::new(
            config.indexes.keys().cloned(),
            sink_names.iter().cloned(),
            Instant::now(),
        ));
        let mut observers: Vec<Arc<dyn Observer>> =
            vec![Arc::new(StatusObserver::new(Arc::clone(&status)))];
        observers.extend(extra_observers);
        let observer: Arc<dyn Observer> = Arc::new(FanOut::new(observers));

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

        let ingest = IngestEngine::new(
            Arc::clone(&capture),
            Arc::clone(&documents),
            Arc::clone(&stream),
            sink_names.clone(),
        )
        .with_observer(Arc::clone(&observer));

        let sink_engines: Vec<SinkEngine> = sinks
            .into_iter()
            .map(|parts| {
                SinkEngine::new(
                    parts.name,
                    parts.sink,
                    Arc::clone(&stream),
                    mappings.clone(),
                )
                .with_options(parts.options)
                .with_observer(Arc::clone(&observer))
                .with_failure_policies(failure_policies.clone())
                .skip_backfill(options.skip_backfill)
            })
            .collect();

        let (control, control_receivers) = DaemonControl::new(&sink_names);
        Ok(RunningDaemon {
            status,
            control,
            control_receivers,
            ingest,
            sink_engines,
            continuity,
            source: capture,
            stream,
            observer,
            options,
        })
    }
}

/// A started deployment: engines built, nothing running yet, so a transport
/// the binary owns can take its handles before [`run`](RunningDaemon::run).
#[derive(Debug)]
pub struct RunningDaemon {
    status: Arc<Status>,
    control: DaemonControl,
    control_receivers: ControlReceivers,
    ingest: IngestEngine,
    sink_engines: Vec<SinkEngine>,
    continuity: Continuity,
    source: Arc<dyn ChangeCapture>,
    stream: Arc<dyn Stream>,
    observer: Arc<dyn Observer>,
    options: DaemonOptions,
}

impl RunningDaemon {
    /// A handle to the live operational status, for a transport (HTTP, a TUI, …)
    /// to read while the daemon runs. Cheap to clone.
    pub fn status(&self) -> Arc<Status> {
        Arc::clone(&self.status)
    }

    /// The operations handle: what a transport calls to act on the running
    /// deployment (a reindex). Cheap to clone.
    pub fn control(&self) -> DaemonControl {
        self.control.clone()
    }

    /// Supervise every engine until the live stream ends, or `shutdown`
    /// resolves — typically a signal future the binary owns. A failed engine
    /// restarts with backoff while the others keep running; a pending batch on
    /// shutdown is simply redelivered on the next run (at-least-once), so
    /// dropping the run mid-flight is safe.
    #[tracing::instrument(name = "daemon.run", skip_all)]
    pub async fn run(self, shutdown: impl Future<Output = ()> + Send) -> anyhow::Result<()> {
        let RunningDaemon {
            status,
            control: _,
            control_receivers,
            ingest,
            sink_engines,
            continuity,
            source,
            stream,
            observer,
            options,
        } = self;

        let _lag = LagGuard(tokio::spawn(lag::poll(
            Arc::clone(&source),
            Arc::clone(&observer),
            options.lag_poll_interval,
        )));

        let result = tokio::select! {
            res = supervise::run_all(ingest, sink_engines, control_receivers, continuity, &source, &stream, &options) => res,
            () = shutdown => {
                tracing::info!("shutdown requested; stopping the deployment");
                Ok(())
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

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
    /// Auto-create/extend the publication to cover every table the indexes read
    /// when the source role is privileged enough. When false, a coverage gap is
    /// only reported (the source still streams whatever the publication covers).
    pub manage_publication: bool,
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
            manage_publication: true,
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
    status: Option<Arc<Status>>,
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
            status: None,
        }
    }

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

    /// Provide the [`Status`] handle to update instead of minting a fresh one.
    ///
    /// The binary uses this to keep **one** process-lifetime status across
    /// pipeline restarts (e.g. an on-demand reindex): the long-lived HTTP surface
    /// and metrics keep reading the same handle, and its counters and uptime
    /// survive the restart rather than resetting. Without it, [`start`](Self::start)
    /// creates a new status each time.
    pub fn with_status(mut self, status: Arc<Status>) -> Self {
        self.status = Some(status);
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
            status,
        } = self;

        tracing::info!(
            slot = %options.slot,
            publication = %options.publication,
            indexes = config.indexes.len(),
            "starting sync",
        );

        // Reset the phase to `Starting`: a reused status may
        // have been left `Stopped` by a previous run.
        let status = status.unwrap_or_else(|| {
            Arc::new(Status::new(config.indexes.keys().cloned(), Instant::now()))
        });
        status.set_phase(Phase::Starting);
        let mut observers: Vec<Arc<dyn Observer>> =
            vec![Arc::new(StatusObserver::new(Arc::clone(&status)))];
        observers.extend(extra_observers);
        let observer: Arc<dyn Observer> = Arc::new(FanOut::new(observers));

        let config = Arc::new(config);
        let SourceParts { capture, documents } =
            backends.source(Arc::clone(&config), &options).await?;
        let sink = backends.sink(&config, &options).await?;

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

        // Held in a guard so it's aborted however this returns — a normal stop
        // *or* the future being cancelled (e.g. the binary dropping the run for a
        // reindex restart) — rather than detaching onto the shared status.
        let _lag = LagGuard(tokio::spawn(lag::poll(source, observer, lag_poll_interval)));

        let result = tokio::select! {
            res = engine.run() => res.context("sync engine stopped"),
            () = shutdown => {
                tracing::info!("shutdown requested; stopping pipeline");
                Ok(())
            }
        };

        status.set_phase(Phase::Stopped);
        result
    }
}

/// Aborts the lag poller when dropped — on a normal stop or on cancellation
/// (the run future being dropped for a restart) alike. Its result is discarded,
/// so there's nothing to join.
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

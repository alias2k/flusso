//! `flusso run` — the composition root for a sync run.
//!
//! The daemon owns the pipeline and exposes its live [`Status`](daemon::Status);
//! this command owns the *transport* around it: telemetry export (traces +
//! metrics), the operational HTTP surface, and process signals. It binds the
//! HTTP listener, installs the meter provider, starts the daemon, serves its
//! status/metrics, and runs until the stream ends, an error stops it, or a
//! signal arrives — then drains the server and flushes telemetry.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use clap::Args;
use daemon::{Daemon, DaemonOptions};
use schema::Config;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use crate::DEFAULT_ARTIFACT;
use crate::observer::OtelObserver;
use crate::{http, metrics, telemetry};

#[derive(Debug, Args)]
pub(crate) struct RunArgs {
    /// Source config to compile and run. When omitted, the compiled artifact at
    /// `--artifact` is loaded instead.
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Compiled artifact to run when `--config` is not given.
    #[arg(long, default_value = DEFAULT_ARTIFACT)]
    artifact: PathBuf,

    /// Logical replication slot to consume. Must already exist.
    #[arg(long, default_value = "flusso")]
    slot: String,

    /// Publication to subscribe to. Must already exist and cover the tables.
    #[arg(long, default_value = "flusso")]
    publication: String,

    /// Skip the initial backfill and resume live capture only. Use after the
    /// index has already been seeded, to avoid re-reading every existing row.
    #[arg(long)]
    skip_backfill: bool,

    /// Pretty-print documents instead of compact one-per-line JSON.
    #[arg(long)]
    pretty: bool,

    /// Maximum changes buffered between capture and processing.
    #[arg(long, default_value_t = 1024)]
    queue_capacity: usize,

    /// Serve the operational HTTP surface (`/healthz`, `/readyz`, `/status`,
    /// `/metrics`) on this address. Omit to disable it.
    #[arg(long)]
    http_addr: Option<SocketAddr>,

    /// How often, in seconds, to sample replication slot lag.
    #[arg(long, default_value_t = 15)]
    lag_poll_secs: u64,
}

pub(crate) async fn execute(args: RunArgs) -> anyhow::Result<()> {
    let tracer_provider = telemetry::init_tracing();

    let config = load_run_config(&args)?;

    // Bind the HTTP listener up front: an unusable `--http-addr` should fail fast,
    // before we open database connections or start the pipeline.
    let listener = match args.http_addr {
        Some(addr) => Some(
            TcpListener::bind(addr)
                .await
                .with_context(|| format!("binding HTTP surface to {addr}"))?,
        ),
        None => None,
    };

    // Install the meter provider *before* starting the daemon, so its observer's
    // instruments bind to the readers. Prometheus reader when serving HTTP; OTLP
    // when an endpoint is configured. With neither, the global meter is a no-op.
    let metrics = if args.http_addr.is_some() || metrics::otlp_configured() {
        Some(metrics::init(args.http_addr.is_some())?)
    } else {
        None
    };
    let registry = metrics.as_ref().and_then(|m| m.registry.clone());

    let options = DaemonOptions {
        slot: args.slot,
        publication: args.publication,
        skip_backfill: args.skip_backfill,
        queue_capacity: args.queue_capacity,
        pretty: args.pretty,
        lag_poll_interval: Duration::from_secs(args.lag_poll_secs),
    };

    // Attach the metrics observer (records to the meter installed above; a no-op
    // if none). Metric definitions live in the binary — the daemon is agnostic.
    let otel_observer: Arc<dyn daemon::Observer> = Arc::new(OtelObserver::new());
    let running = Daemon::new(config)
        .with_options(options)
        .with_observer(otel_observer)
        .start()
        .await?;
    let status = running.status();

    // `in_flight` is derived from the status, so it's an observable gauge
    // registered now that the handle exists. Kept alive for the run's duration.
    let _in_flight_gauge = metrics::register_in_flight_gauge(Arc::clone(&status));

    // Serve the status/metrics surface (graceful drain on `http_shutdown`).
    let (http_shutdown, http_rx) = oneshot::channel::<()>();
    let http = listener.map(|listener| {
        let state = http::AppState {
            status: Arc::clone(&status),
            registry: registry.clone(),
        };
        tokio::spawn(http::serve(listener, state, http_rx))
    });

    // Run until the stream ends, an error stops it, or a signal arrives.
    let result = running.run(shutdown_signal()).await;

    // Drain the HTTP server, then flush telemetry — on success or error alike.
    let _ = http_shutdown.send(());
    if let Some(http) = http
        && let Err(error) = http.await
    {
        tracing::warn!(%error, "HTTP server task did not shut down cleanly");
    }
    if let Some(metrics) = &metrics {
        metrics.shutdown();
    }
    if let Some(provider) = tracer_provider
        && let Err(error) = provider.shutdown()
    {
        tracing::warn!(%error, "failed to flush OTLP tracer on shutdown");
    }

    result
}

/// Load the configuration a `run` should use: compiled fresh from `--config`, or
/// read back from the compiled artifact.
fn load_run_config(args: &RunArgs) -> anyhow::Result<Config> {
    match &args.config {
        Some(path) => {
            schema::load(path).with_context(|| format!("loading config from {}", path.display()))
        }
        None => schema::load_compiled(&args.artifact)
            .with_context(|| format!("loading compiled config from {}", args.artifact.display())),
    }
}

/// Resolve once either Ctrl-C (SIGINT) or, on Unix, SIGTERM arrives — the
/// process's signals are the binary's to own, not the daemon library's.
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::warn!(%error, "failed to listen for Ctrl-C");
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => {
                tracing::warn!(%error, "failed to install SIGTERM handler");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}

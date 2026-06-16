//! `flusso run` — the composition root for a sync run.
//!
//! The daemon owns the pipeline and exposes its live [`Status`](daemon::Status);
//! this command owns the *transport* around it: telemetry export (traces +
//! metrics), the two operational HTTP surfaces, and process signals. It binds
//! both listeners, installs the meter provider, starts the daemon, serves its
//! public status/metrics + private control surface, and runs until the stream
//! ends, an error stops it, or a signal arrives — then drains and flushes.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
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
use crate::backends::FlussoBackends;
use crate::http::{self, BasicAuth, DEFAULT_ADMIN_PASSWORD, DEFAULT_ADMIN_USER};
use crate::telemetry::observer::OtelObserver;
use crate::telemetry::{self, metrics};

/// Default bind address for the public surface — localhost, so an unconfigured
/// run is reachable for local ops without being exposed; deployments set
/// `0.0.0.0` explicitly.
const DEFAULT_PUBLIC_ADDRESS: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9464);
/// Default bind address for the private control surface — localhost, which
/// matters because it ships with default Basic-auth credentials.
const DEFAULT_PRIVATE_ADDRESS: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9465);

#[derive(Debug, Args)]
pub(crate) struct RunArgs {
    /// Source config to compile and run. When omitted, the compiled artifact at
    /// `--artifact` is loaded instead.
    #[arg(short, long, env = "FLUSSO_CONFIG")]
    config: Option<PathBuf>,

    /// Compiled artifact to run when `--config` is not given.
    #[arg(long, env = "FLUSSO_ARTIFACT", default_value = DEFAULT_ARTIFACT)]
    artifact: PathBuf,

    /// Logical replication slot to consume. Must already exist.
    #[arg(long, env = "FLUSSO_SLOT", default_value = "flusso")]
    slot: String,

    /// Publication to subscribe to. Must already exist and cover the tables.
    #[arg(long, env = "FLUSSO_PUBLICATION", default_value = "flusso")]
    publication: String,

    /// Skip the initial backfill and resume live capture only. Use after the
    /// index has already been seeded, to avoid re-reading every existing row.
    #[arg(long, env = "FLUSSO_SKIP_BACKFILL")]
    skip_backfill: bool,

    /// Pretty-print documents instead of compact one-per-line JSON.
    #[arg(long, env = "FLUSSO_PRETTY")]
    pretty: bool,

    /// Maximum changes buffered between capture and processing.
    #[arg(long, env = "FLUSSO_QUEUE_CAPACITY", default_value_t = 1024)]
    queue_capacity: usize,

    /// Bind address for the public, read-only HTTP surface (`/healthz`,
    /// `/readyz`, `/status`, `/metrics`). Overrides `[server].public_address`
    /// from config; defaults to `127.0.0.1:9464`.
    #[arg(long, env = "FLUSSO_PUBLIC_ADDRESS")]
    public_address: Option<SocketAddr>,

    /// Bind address for the private, Basic-auth control surface (`/indexes`,
    /// `/reindex`). Overrides `[server].private_address` from config; defaults
    /// to `127.0.0.1:9465`.
    #[arg(long, env = "FLUSSO_PRIVATE_ADDRESS")]
    private_address: Option<SocketAddr>,

    /// Username for the private control surface (HTTP Basic auth).
    #[arg(long, env = "FLUSSO_ADMIN_USER", default_value = DEFAULT_ADMIN_USER)]
    admin_user: String,

    /// Password for the private control surface (HTTP Basic auth). Change it
    /// from the default before exposing the private surface.
    #[arg(long, env = "FLUSSO_ADMIN_PASSWORD", default_value = DEFAULT_ADMIN_PASSWORD)]
    admin_password: String,

    /// How often, in seconds, to sample replication slot lag.
    #[arg(long, env = "FLUSSO_LAG_POLL_SECS", default_value_t = 15)]
    lag_poll_secs: u64,
}

pub(crate) async fn execute(args: RunArgs) -> anyhow::Result<()> {
    let tracer_provider = telemetry::init_tracing();

    let config = load_run_config(&args)?;

    // Resolve each surface's bind address with the precedence CLI flag / env
    // (merged by clap) > `[server]` config > built-in default.
    let public_addr = args
        .public_address
        .or(config.server.public_address)
        .unwrap_or(DEFAULT_PUBLIC_ADDRESS);
    let private_addr = args
        .private_address
        .or(config.server.private_address)
        .unwrap_or(DEFAULT_PRIVATE_ADDRESS);

    // Bind both listeners up front: an unusable address should fail fast, before
    // we open database connections or start the pipeline.
    let public_listener = TcpListener::bind(public_addr)
        .await
        .with_context(|| format!("binding public HTTP surface to {public_addr}"))?;
    let private_listener = TcpListener::bind(private_addr)
        .await
        .with_context(|| format!("binding private HTTP surface to {private_addr}"))?;

    let basic_auth = Arc::new(BasicAuth::new(args.admin_user, args.admin_password));
    if basic_auth.uses_default_password() {
        tracing::warn!(
            "the private control surface is using the DEFAULT admin password ({DEFAULT_ADMIN_PASSWORD:?}); \
             set --admin-password / FLUSSO_ADMIN_PASSWORD before exposing it"
        );
    }

    // The public surface always serves `/metrics`, so install the Prometheus
    // reader (plus an OTLP push reader when the env configures one).
    let metrics = metrics::init(true)?;
    let registry = metrics.registry.clone();

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
    let running = Daemon::new(config, Arc::new(FlussoBackends))
        .with_options(options)
        .with_observer(otel_observer)
        .start()
        .await?;
    let status = running.status();

    // `in_flight` is derived from the status, so it's an observable gauge
    // registered now that the handle exists. Kept alive for the run's duration.
    let _in_flight_gauge = metrics::register_in_flight_gauge(Arc::clone(&status));

    // Serve both surfaces (graceful drain on their shutdown signals).
    let (public_shutdown, public_rx) = oneshot::channel::<()>();
    let (private_shutdown, private_rx) = oneshot::channel::<()>();
    let public = tokio::spawn(http::serve(
        "public",
        public_listener,
        http::public_router(http::PublicState {
            status: Arc::clone(&status),
            registry,
        }),
        public_rx,
    ));
    let private = tokio::spawn(http::serve(
        "private",
        private_listener,
        http::private_router(Arc::clone(&status), Arc::clone(&basic_auth)),
        private_rx,
    ));

    // Run until the stream ends, an error stops it, or a signal arrives.
    let result = running.run(shutdown_signal()).await;

    // Drain both HTTP servers, then flush telemetry — on success or error alike.
    let _ = public_shutdown.send(());
    let _ = private_shutdown.send(());
    for (task, surface) in [(public, "public"), (private, "private")] {
        if let Err(error) = task.await {
            tracing::warn!(%error, surface, "HTTP server task did not shut down cleanly");
        }
    }
    metrics.shutdown();
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

//! `flusso run` — the composition root for a sync run.
//!
//! Like `cargo run`, this command compiles before it runs: when a `flusso.toml`
//! is present (the default path, or `--config`) it compiles the config + schemas,
//! **writes the `flusso.lock`**, and runs that — so a dev who edits the config
//! gets a fresh, committable lock for free. With no config it falls back to the
//! existing lock, and `--locked` runs the lock as-is without recompiling. See
//! [`resolve_config`].
//!
//! The daemon owns the pipeline and exposes its live [`Status`](daemon::Status);
//! this command owns the *transport* around it: telemetry export (traces +
//! metrics), the two operational HTTP surfaces, and process signals. It binds
//! both listeners, installs the meter provider, starts the daemon, serves its
//! public status/metrics + private control surface, and runs until the stream
//! ends, an error stops it, or a signal arrives — then drains and flushes.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use clap::Args;
use daemon::{Daemon, DaemonOptions};
use schema::{Config, IndexName};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};

use crate::DEFAULT_LOCK;
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
    /// Source config to compile and run. Defaults to `flusso.toml` in the working
    /// directory; when that is absent the compiled `--lock` is loaded instead.
    /// An explicitly given path that does not exist is an error.
    #[arg(short, long, env = "FLUSSO_CONFIG")]
    config: Option<PathBuf>,

    /// Compiled lock path: rewritten from the config on every start (cargo-style),
    /// and loaded directly when there is no config to compile.
    #[arg(long, env = "FLUSSO_LOCK", default_value = DEFAULT_LOCK)]
    lock: PathBuf,

    /// Run the existing `--lock` as-is: skip compiling the config and skip
    /// rewriting the lock. Use for deterministic deploys off a committed lock.
    #[arg(long, env = "FLUSSO_LOCKED")]
    locked: bool,

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

    let config = resolve_config(&args)?;

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

    // One status for the whole process, reused by every (re)started daemon run —
    // so the long-lived HTTP surface + metrics keep reading the same handle and
    // its counters/uptime survive a reindex restart. (`config` is moved into the
    // daemon below, so read its index names first.)
    let status = Arc::new(daemon::Status::new(
        config.indexes.keys().cloned(),
        std::time::Instant::now(),
    ));
    // `in_flight` is derived from the status, registered now that it exists.
    let _in_flight_gauge = metrics::register_in_flight_gauge(Arc::clone(&status));

    // Reindex requests from the private surface drive pipeline restarts.
    let (reindex_tx, mut reindex_rx) = mpsc::channel::<IndexName>(8);

    // Serve both surfaces once, for the whole process lifetime (they read the
    // shared status, so restarts don't disturb them). Graceful drain on shutdown.
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
        http::private_router(
            http::PrivateState {
                status: Arc::clone(&status),
                reindex: reindex_tx,
            },
            Arc::clone(&basic_auth),
        ),
        private_rx,
    ));

    let backends: Arc<dyn daemon::Backends> = Arc::new(FlussoBackends);
    let otel_observer: Arc<dyn daemon::Observer> = Arc::new(OtelObserver::new());

    // Run the pipeline, restarting it whenever a reindex is requested. A reindex
    // stages a fresh generation (on a throwaway sink) and the restarted run's
    // backfill seeds it; the alias swaps on completion. Otherwise the run ends
    // only when the stream stops, an error halts it, or a signal arrives.
    let result = loop {
        let running = Daemon::new(config.clone(), Arc::clone(&backends))
            .with_options(options.clone())
            .with_observer(Arc::clone(&otel_observer))
            .with_status(Arc::clone(&status))
            .start()
            .await?;

        let index = tokio::select! {
            outcome = running.run(shutdown_signal()) => break outcome,
            Some(index) = reindex_rx.recv() => index,
        };

        tracing::info!(
            index = index.as_ref(),
            "reindex requested; staging a fresh generation and restarting"
        );
        if let Err(error) = stage_reindex(&config, &options, backends.as_ref(), &index).await {
            tracing::error!(%error, index = index.as_ref(), "failed to stage reindex; restarting without it");
        }
    };

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

/// Stage a reindex of `index`: resolve its mapping from the config, build a sink,
/// and ask it to prepare a fresh generation. The mapping (with its schema hash)
/// is all the sink needs — it doesn't touch the running engine's in-memory state,
/// so a throwaway sink built here is enough.
async fn stage_reindex(
    config: &Config,
    options: &DaemonOptions,
    backends: &dyn daemon::Backends,
    index: &IndexName,
) -> anyhow::Result<()> {
    let mapping = config
        .resolve_mappings()
        .into_iter()
        .find(|mapping| &mapping.index == index)
        .with_context(|| format!("no such index {}", index.as_ref()))?;
    let sink = backends.sink(config, options).await?;
    sink.reindex(&mapping).await?;
    Ok(())
}

/// What `run` does to obtain its [`Config`], decided purely from the flags plus
/// whether the candidate config file exists. Split out from the IO in
/// [`resolve_config`] so the branching is unit-testable without a filesystem.
#[derive(Debug, PartialEq, Eq)]
enum ConfigPlan {
    /// `--locked`: load the existing lock as-is, no compile, no write.
    UseLock,
    /// Compile this config path, (re)write the lock, then run the result.
    Compile(PathBuf),
    /// No config to compile: fall back to loading the existing lock.
    Fallback,
    /// An explicit `--config` was given but does not exist — fatal.
    Missing(PathBuf),
}

/// Decide the [`ConfigPlan`] cargo-style: `--locked` wins; otherwise an explicit
/// `--config` is compiled when it exists and is an error when it doesn't, while a
/// missing default `flusso.toml` is allowed and falls back to the lock.
fn plan_config(
    locked: bool,
    config: Option<&Path>,
    default_config: &Path,
    exists: impl Fn(&Path) -> bool,
) -> ConfigPlan {
    if locked {
        return ConfigPlan::UseLock;
    }
    match config {
        Some(path) if exists(path) => ConfigPlan::Compile(path.to_path_buf()),
        Some(path) => ConfigPlan::Missing(path.to_path_buf()),
        None if exists(default_config) => ConfigPlan::Compile(default_config.to_path_buf()),
        None => ConfigPlan::Fallback,
    }
}

/// Resolve the configuration a `run` should use, cargo-style: compile the config
/// and **(re)write the lock** when one is present, fall back to the existing lock
/// otherwise, or run the lock as-is under `--locked`. A lock write failure is
/// fatal — the committed lock must reflect the config it was compiled from.
fn resolve_config(args: &RunArgs) -> anyhow::Result<Config> {
    let default_config = PathBuf::from(crate::DEFAULT_CONFIG);
    match plan_config(
        args.locked,
        args.config.as_deref(),
        &default_config,
        |path| path.exists(),
    ) {
        ConfigPlan::UseLock => load_lock(&args.lock),
        ConfigPlan::Compile(config_path) => {
            let compiled = schema::compile(&config_path)
                .with_context(|| format!("compiling config from {}", config_path.display()))?;
            schema::write(&compiled, &args.lock)
                .with_context(|| format!("writing compiled lock to {}", args.lock.display()))?;
            tracing::info!(
                indexes = compiled.config.indexes.len(),
                lock = %args.lock.display(),
                "compiled config and wrote lock"
            );
            Ok(compiled.config)
        }
        ConfigPlan::Fallback => load_lock(&args.lock).with_context(|| {
            format!(
                "no {} to compile and no compiled lock at {}; create a config or build a lock first",
                crate::DEFAULT_CONFIG,
                args.lock.display()
            )
        }),
        ConfigPlan::Missing(config_path) => {
            anyhow::bail!("config file {} not found", config_path.display())
        }
    }
}

/// Read back a previously compiled lock from `path`.
fn load_lock(path: &Path) -> anyhow::Result<Config> {
    schema::load_compiled(path)
        .with_context(|| format!("loading compiled lock from {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::{ConfigPlan, plan_config};
    use std::path::{Path, PathBuf};

    const DEFAULT: &str = "flusso.toml";

    /// Helper: an `exists` predicate matching exactly the given paths.
    fn present<'a>(paths: &'a [&'a str]) -> impl Fn(&Path) -> bool + 'a {
        move |p| paths.iter().any(|present| Path::new(present) == p)
    }

    #[test]
    fn locked_uses_the_lock_and_ignores_the_config() {
        // Even with a config present, `--locked` short-circuits to the lock.
        let plan = plan_config(
            true,
            Some(Path::new("flusso.toml")),
            Path::new(DEFAULT),
            present(&["flusso.toml"]),
        );
        assert_eq!(plan, ConfigPlan::UseLock);
    }

    #[test]
    fn explicit_config_present_is_compiled() {
        let plan = plan_config(
            false,
            Some(Path::new("dev/flusso.toml")),
            Path::new(DEFAULT),
            present(&["dev/flusso.toml"]),
        );
        assert_eq!(plan, ConfigPlan::Compile(PathBuf::from("dev/flusso.toml")));
    }

    #[test]
    fn explicit_config_missing_is_fatal() {
        let plan = plan_config(
            false,
            Some(Path::new("nope.toml")),
            Path::new(DEFAULT),
            present(&[]),
        );
        assert_eq!(plan, ConfigPlan::Missing(PathBuf::from("nope.toml")));
    }

    #[test]
    fn default_config_present_is_compiled() {
        let plan = plan_config(false, None, Path::new(DEFAULT), present(&[DEFAULT]));
        assert_eq!(plan, ConfigPlan::Compile(PathBuf::from(DEFAULT)));
    }

    #[test]
    fn default_config_absent_falls_back_to_the_lock() {
        let plan = plan_config(false, None, Path::new(DEFAULT), present(&[]));
        assert_eq!(plan, ConfigPlan::Fallback);
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

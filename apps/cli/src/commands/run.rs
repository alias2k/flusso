//! `flusso run` — the composition root for a sync run.
//!
//! Like `cargo run`, this command compiles before it runs: when a `flusso.toml`
//! is present (the default path, or `--config`) it compiles the config + schemas,
//! **writes the `flusso.lock`**, and runs that — so a dev who edits the config
//! gets a fresh, committable lock for free. With no config it falls back to the
//! existing lock, and `--locked` runs the lock as-is without recompiling. See
//! [`resolve_config`].
//!
//! The daemon owns the deployment and exposes its live [`Status`](daemon::Status)
//! and a [`DaemonControl`](daemon::DaemonControl); this command owns the
//! *transport* around them: telemetry export (traces + metrics), the two
//! operational HTTP surfaces, and process signals. It binds both listeners,
//! installs the meter provider, starts the daemon, serves its public
//! status/metrics + private control surface, and runs until the stream ends or
//! a stop arrives (a signal, or with `--exit-with-parent` the parent exiting).
//! Then it closes the surfaces and flushes telemetry under a deadline; see
//! [`crate::shutdown`] for what a stop does and how it is bounded.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use clap::Args;
use config::Config;
use daemon::{Daemon, DaemonOptions};
use tokio::sync::oneshot;

use crate::DEFAULT_LOCK;
use crate::adapters;
use crate::backends::FlussoBackends;
use crate::http::{self, BasicAuth, DEFAULT_ADMIN_PASSWORD, DEFAULT_ADMIN_USER};
use crate::lock::{ConfigPlan, plan_config};
use crate::shutdown::{self, Cause, Drained, Progress, Signals};
use crate::telemetry::observer::OtelObserver;
use crate::telemetry::script_log::ScriptFailureLog;
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

    /// Compiled lock path: recompiled from the config on every start
    /// (cargo-style) and rewritten only when its content changed, and loaded
    /// directly when there is no config to compile.
    #[arg(long, env = "FLUSSO_LOCK", default_value = DEFAULT_LOCK)]
    lock: PathBuf,

    /// Run the existing `--lock` as-is: skip compiling the config and skip
    /// rewriting the lock. Use for deterministic deploys off a committed lock.
    #[arg(long, env = "FLUSSO_LOCKED")]
    locked: bool,

    /// Logical replication slot to consume. Overrides `[source] slot`; defaults
    /// to it, then to `flusso`.
    #[arg(long, env = "FLUSSO_SLOT")]
    slot: Option<String>,

    /// Publication to subscribe to. Overrides `[source] publication`; defaults
    /// to it, then to `flusso`. flusso creates/extends it to cover every table
    /// the indexes read when the source role is privileged enough.
    #[arg(long, env = "FLUSSO_PUBLICATION")]
    publication: Option<String>,

    /// Whether flusso may auto-create/extend the publication. Overrides the
    /// `[source] manage_publication` config option; defaults to that, then to
    /// enabled. Set `false` to keep flusso from ever issuing publication DDL
    /// (it then only warns about coverage gaps).
    #[arg(long, env = "FLUSSO_MANAGE_PUBLICATION")]
    manage_publication: Option<bool>,

    /// Whether flusso may set `REPLICA IDENTITY FULL` on child tables that
    /// need it. Overrides the `[source] manage_replica_identity` config option;
    /// defaults to that, then to enabled. Set `false` to keep flusso from ever
    /// issuing the `ALTER` (it then only warns about the gap).
    #[arg(long, env = "FLUSSO_MANAGE_REPLICA_IDENTITY")]
    manage_replica_identity: Option<bool>,

    /// Refuse to start on any error `flusso check` reports, printed as
    /// `check` prints it. Without it, only an error no document builds past
    /// stops the start; the others are logged at `error` and counted on
    /// `flusso.index.schema_errors`, and the start goes on.
    #[arg(long, env = "FLUSSO_STRICT")]
    strict: bool,

    /// Skip the initial backfill and resume live capture only. Use after the
    /// index has already been seeded, to avoid re-reading every existing row.
    #[arg(long, env = "FLUSSO_SKIP_BACKFILL")]
    skip_backfill: bool,

    /// Pretty-print documents on every stdout sink instead of compact
    /// one-per-line JSON. Overrides `pretty` on those sinks.
    #[arg(long, env = "FLUSSO_PRETTY")]
    pretty: bool,

    /// Deprecated, and ignored with a warning, as `[stream] capacity` is:
    /// `--max-built-mib` bounds what the lanes hold.
    #[arg(long, env = "FLUSSO_QUEUE_CAPACITY")]
    queue_capacity: Option<usize>,

    /// Bind address for the public, read-only HTTP surface (`/healthz`,
    /// `/readyz`, `/status`, `/metrics`, `/wait`, `/indexes/<name>/status`).
    /// Overrides `[server].public_address`
    /// from config; defaults to `127.0.0.1:9464`.
    #[arg(long, env = "FLUSSO_PUBLIC_ADDRESS")]
    public_address: Option<SocketAddr>,

    /// Bind address for the private, Basic-auth control surface (`/indexes`,
    /// `/indexes/<name>/reindex`). Overrides `[server].private_address` from config; defaults
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

    /// Literal prefix prepended to every index name flusso owns, so several
    /// deployments (dev/staging/nightly) can share one OpenSearch cluster
    /// without colliding. Overrides `prefix` from config; defaults to it, then
    /// to no prefix. The `flusso-query` consumer must be given the same prefix.
    #[arg(long, env = "FLUSSO_INDEX_PREFIX")]
    index_prefix: Option<String>,

    /// Commit a batch once this many live changes have accumulated. Overrides
    /// `[batch].max_changes` from config; defaults to 256. A lone change on a
    /// quiet stream is committed at once regardless.
    #[arg(long, env = "FLUSSO_BATCH_MAX_CHANGES")]
    batch_max_changes: Option<std::num::NonZeroUsize>,

    /// The longest a batch stays open while changes keep arriving, in
    /// milliseconds. Overrides `[batch].max_delay_ms` from config; defaults
    /// to 50.
    #[arg(long, env = "FLUSSO_BATCH_MAX_DELAY_MS")]
    batch_max_delay_ms: Option<u64>,

    /// Bulk writes each sink runs at once. Overrides `[concurrency].writes`
    /// from config; defaults to 4. Two writes never hold the same document,
    /// and live changes go before backfill pages. At most 64. Never written
    /// into the lock.
    #[arg(long, env = "FLUSSO_WRITES", value_parser = config::parse_writes)]
    writes: Option<std::num::NonZeroUsize>,

    /// Document builds the ingest engine runs at once. Overrides
    /// `[concurrency].builds` from config; defaults to 4. A document is never
    /// built twice at once, each lane gets batches in the order they started,
    /// and live batches go before backfill pages. At most 32. Never written
    /// into the lock.
    #[arg(long, env = "FLUSSO_BUILDS", value_parser = config::parse_builds)]
    builds: Option<std::num::NonZeroUsize>,

    /// MiB of built documents held before the ingest engine waits for the
    /// sinks to acknowledge some. Overrides `[concurrency].max_built_mib`
    /// from config; defaults to 512. A backfill holds at most half. Counts
    /// heap, about 8–10× a document's JSON. 16 to 1,048,576. Never written
    /// into the lock.
    #[arg(long, env = "FLUSSO_MAX_BUILT_MIB", value_parser = config::parse_max_built_mib)]
    max_built_mib: Option<std::num::NonZeroUsize>,

    /// The share of `--max-built-mib` one index's live documents may hold
    /// while two or more indexes run, so one index bursting leaves room for
    /// the others to build. Overrides `[concurrency].index_budget_share` from
    /// config; defaults to 0.5. Above 0, at most 1; with one index it does
    /// nothing. Never written into the lock. [env: FLUSSO_INDEX_BUDGET_SHARE]
    #[arg(long, value_parser = config::parse_index_budget_share)]
    index_budget_share: Option<f64>,

    /// Documents one index may have waiting to be built before flusso stops
    /// reading changes: how far reading runs ahead of the builds, per index,
    /// and what a stop may have to build. Overrides
    /// `[concurrency].max_waiting_per_index` from config; defaults to twice
    /// `--batch-max-changes` (512), or `--max-waiting` when that is set lower.
    /// At least 1. Never written into the lock.
    /// [env: FLUSSO_MAX_WAITING_PER_INDEX]
    #[arg(long, value_parser = config::parse_max_waiting)]
    max_waiting_per_index: Option<u64>,

    /// Documents every index together may have waiting to be built, and
    /// changes read ahead of the builds, before flusso stops reading.
    /// Overrides `[concurrency].max_waiting` from config; defaults to
    /// `2 × --builds × --batch-max-changes` (2,048), or
    /// `--max-waiting-per-index` when that is set higher. With both set, at
    /// least `--max-waiting-per-index`. Never written into the lock.
    /// [env: FLUSSO_MAX_WAITING]
    #[arg(long, value_parser = config::parse_max_waiting)]
    max_waiting: Option<u64>,

    /// Documents every held index together may have waiting before flusso
    /// stops reading changes for every index. With two or more indexes, an
    /// index held by the memory budget or a full lane stops no other index's
    /// reading once it has `--max-waiting-per-index` documents waiting; its
    /// changes are kept as documents to build, up to this many over every
    /// such index. Overrides `[concurrency].max_pending_documents` from
    /// config; defaults to 1,000,000. At least 1. Never written into the
    /// lock.
    /// [env: FLUSSO_MAX_PENDING_DOCUMENTS]
    #[arg(long, value_parser = config::parse_max_waiting)]
    max_pending_documents: Option<u64>,

    /// The first wait before trying again, in milliseconds: a failed engine's
    /// restart, a parked lane's probe, a re-staged lane's next staging. Each
    /// wait doubles up to `--backoff-max-ms`. Overrides `[backoff].initial_ms`
    /// from config; defaults to 1000. At least 1. Never written into the lock.
    /// [env: FLUSSO_BACKOFF_INITIAL_MS]
    #[arg(long, value_parser = config::parse_backoff_ms)]
    backoff_initial_ms: Option<std::num::NonZeroU64>,

    /// The longest wait before trying again, in milliseconds. Overrides
    /// `[backoff].max_ms` from config; defaults to 60000, or the first wait
    /// when that is larger. At least the first wait; equal to it, every wait
    /// is the same. Never written into the lock. [env: FLUSSO_BACKOFF_MAX_MS]
    #[arg(long, value_parser = config::parse_backoff_ms)]
    backoff_max_ms: Option<std::num::NonZeroU64>,

    /// How long a stop may take, in seconds, from the first signal to exit.
    /// A stop drains: nothing new is read, what is in flight lands on every
    /// sink and is confirmed to Postgres, then the HTTP surfaces close and
    /// telemetry is flushed. Past the deadline flusso logs the step and the
    /// sinks it gave up on and exits 1. `0` doesn't drain: it stops at once
    /// and exits 0, the one exit 0 that confirmed nothing. The same sender
    /// signalling twice within 500 ms, or any three signals within 1 s, exit
    /// at once. Whatever wasn't confirmed is read again on the next start.
    #[arg(long, env = "FLUSSO_SHUTDOWN_TIMEOUT_SECS", default_value_t = shutdown::DEFAULT_TIMEOUT_SECS)]
    shutdown_timeout_secs: u64,

    /// Stop when the process that started flusso exits (checked twice a
    /// second), as if it had sent SIGTERM. For task runners and other parents
    /// that can die without stopping their children; off by default, so a
    /// run started in the background from a shell outlives the shell.
    #[arg(long, env = "FLUSSO_EXIT_WITH_PARENT")]
    exit_with_parent: bool,
}

pub(crate) async fn execute(args: RunArgs) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let tracer_provider = telemetry::init_tracing();
    let parent = if args.exit_with_parent {
        shutdown::parent()
    } else {
        None
    };

    let mut config = resolve_config(&args)?;

    // Runtime override: the flag (or FLUSSO_INDEX_PREFIX) wins over the config
    // value, which wins over no prefix. Validated before anything touches the
    // cluster, so a bad prefix fails fast rather than producing odd index names.
    if let Some(prefix) = args.index_prefix.clone() {
        config.prefix = prefix;
    }
    config::validate_index_prefix(&config.prefix)
        .map_err(|reason| anyhow::anyhow!("invalid index prefix: {reason}"))?;
    if let Some(max_changes) = args.batch_max_changes {
        config.batch.max_changes = Some(max_changes);
    }
    if let Some(max_delay_ms) = args.batch_max_delay_ms {
        config.batch.max_delay_ms = Some(max_delay_ms);
    }
    if let Some(writes) = args.writes {
        config.concurrency.writes = Some(writes);
    }
    if let Some(builds) = args.builds {
        config.concurrency.builds = Some(builds);
    }
    if let Some(max_built_mib) = args.max_built_mib {
        config.concurrency.max_built_mib = Some(max_built_mib);
    }
    lay_backoff(
        &mut config,
        args.backoff_initial_ms,
        args.backoff_max_ms,
        |name| std::env::var(name).ok(),
    )?;
    lay_index_budget_share(&mut config, args.index_budget_share, |name| {
        std::env::var(name).ok()
    })?;
    lay_max_waiting(
        &mut config,
        args.max_waiting_per_index,
        args.max_waiting,
        |name| std::env::var(name).ok(),
    )?;
    lay_max_pending_documents(&mut config, args.max_pending_documents, |name| {
        std::env::var(name).ok()
    })?;

    // Adapter knobs given as flags/env are laid over the file's entries, then
    // every entry is validated against its adapter before anything connects.
    adapters::apply_overrides(
        &mut config,
        &adapters::Overrides {
            slot: args.slot.clone(),
            publication: args.publication.clone(),
            manage_publication: args.manage_publication,
            manage_replica_identity: args.manage_replica_identity,
            pretty: args.pretty,
            queue_capacity: args.queue_capacity,
        },
    );
    adapters::validate(&config)?;
    crate::backends::compile_scripts(&config).context("compiling the scripts")?;
    if let Some(warning) =
        crate::memory::over_budget(daemon::max_built_bytes(&config), crate::memory::ceiling())
    {
        tracing::warn!("{warning}");
    }

    let public_addr = args
        .public_address
        .or(config.server.public_address)
        .unwrap_or(DEFAULT_PUBLIC_ADDRESS);
    let private_addr = args
        .private_address
        .or(config.server.private_address)
        .unwrap_or(DEFAULT_PRIVATE_ADDRESS);

    let (public_listener, private_listener) =
        bind_then_compare_clusters(&config, public_addr, private_addr).await?;

    let basic_auth = Arc::new(BasicAuth::new(args.admin_user, args.admin_password));
    if basic_auth.uses_default_password() {
        tracing::warn!(
            "the private control surface is using the DEFAULT admin password ({DEFAULT_ADMIN_PASSWORD:?}); \
             set --admin-password / FLUSSO_ADMIN_PASSWORD before exposing it"
        );
    }

    let metrics = metrics::init(true)?;
    let registry = metrics.registry.clone();

    let options = DaemonOptions {
        skip_backfill: args.skip_backfill,
        lag_poll_interval: Duration::from_secs(args.lag_poll_secs),
    };

    let backends: Arc<dyn daemon::Backends> = Arc::new(FlussoBackends {
        strict: args.strict,
    });
    let otel_observer: Arc<dyn daemon::Observer> = Arc::new(OtelObserver::new(started));
    let running = Daemon::new(config, Arc::clone(&backends))
        .with_options(options)
        .with_observer(otel_observer)
        .with_observer(Arc::new(ScriptFailureLog::default()))
        .start()
        .await?;
    let status = running.status();
    let control = running.control();
    let fence = running.fence();
    let _in_flight_gauge = metrics::register_in_flight_gauge(Arc::clone(&status));
    let _changes_committed = metrics::register_changes_committed_counter(Arc::clone(&status));
    let _visibility_wait_gauge = metrics::register_visibility_wait_gauge(Arc::clone(&status));
    let _lane_outstanding_gauge = metrics::register_lane_outstanding_gauge(Arc::clone(&status));
    let _lane_parked_gauge = metrics::register_lane_parked_gauge(Arc::clone(&status));
    let _stall_gauges = metrics::register_stall_gauges(Arc::clone(&status));
    let _index_gauges = metrics::register_index_gauges(Arc::clone(&status));
    let _memory_instruments = metrics::register_memory_instruments(running.budget());

    let waits = http::Waits::default();
    let (public_shutdown, public_rx) = oneshot::channel::<()>();
    let (private_shutdown, private_rx) = oneshot::channel::<()>();
    let public = tokio::spawn(http::serve(
        "public",
        public_listener,
        http::public_router(http::PublicState {
            status: Arc::clone(&status),
            registry,
            fence,
            waits: waits.clone(),
        }),
        public_rx,
    ));
    let private = tokio::spawn(http::serve(
        "private",
        private_listener,
        http::private_router(
            http::PrivateState {
                status: Arc::clone(&status),
                control,
                reattach_wait: http::REATTACH_VERDICT,
            },
            Arc::clone(&basic_auth),
        ),
        private_rx,
    ));

    let mut signals = Signals::install()?;
    let (stop, stop_rx) = oneshot::channel::<()>();
    let run = running.run(async {
        let _ = stop_rx.await;
    });
    tokio::pin!(run);
    let mut first_sender = None;
    let ended = tokio::select! {
        result = &mut run => Some(result),
        cause = shutdown::requested(&mut signals, parent) => {
            match cause {
                Cause::Signal(signal, sender) => {
                    first_sender = sender;
                    tracing::info!(
                        signal = signal.name(),
                        "stopping: landing what is in flight, then closing. Send the signal \
                         twice quickly to exit now"
                    );
                }
                Cause::ParentExited { parent } => tracing::warn!(
                    parent,
                    "the parent process {parent} exited; stopping (--exit-with-parent)"
                ),
            }
            None
        }
    };

    let timeout = Duration::from_secs(args.shutdown_timeout_secs);
    if ended.is_none() && timeout.is_zero() {
        tracing::info!(
            "stopped without draining, as asked (--shutdown-timeout-secs 0): whatever was not \
             confirmed is read again on the next start"
        );
        waits.end_before_exit().await;
        return Ok(());
    }
    let began = tokio::time::Instant::now();
    let deadline = began + timeout;
    let progress = Progress::default();
    let stopping = async {
        let result = match ended {
            Some(result) => result,
            None => {
                progress.set(shutdown::DRAINING);
                let _ = stop.send(());
                run.await
            }
        };
        progress.set("closing the HTTP surfaces");
        waits.end();
        let _ = public_shutdown.send(());
        let _ = private_shutdown.send(());
        let closed = shutdown::side_step(deadline, async {
            for (task, surface) in [(public, "public"), (private, "private")] {
                if let Err(error) = task.await {
                    tracing::warn!(%error, surface, "HTTP server task did not shut down cleanly");
                }
            }
        })
        .await;
        if !closed {
            tracing::warn!("an HTTP connection did not close in time; leaving it");
        }
        progress.set("flushing telemetry");
        let flushed = shutdown::side_step(deadline, async {
            let flushed = tokio::task::spawn_blocking(move || {
                metrics.shutdown();
                if let Some(provider) = tracer_provider
                    && let Err(error) = provider.shutdown()
                {
                    tracing::warn!(%error, "failed to flush OTLP tracer on shutdown");
                }
            })
            .await;
            if let Err(error) = flushed {
                tracing::warn!(%error, "flushing telemetry failed");
            }
        })
        .await;
        if !flushed {
            tracing::warn!("telemetry did not flush in time; the last spans and metrics are lost");
        }
        result
    };
    let mut shown = shutdown::Throttle::default();
    let interrupt = shutdown::forced(&mut signals, (began, first_sender), |signal| {
        if !shown.allow(tokio::time::Instant::now()) {
            return;
        }
        let unfinished = shutdown::unfinished_sinks(&status.snapshot());
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        tracing::warn!(
            signal = signal.name(),
            "stop already requested: {}{}, {}s left. Send the signal twice quickly to exit now",
            progress.current(),
            if unfinished.is_empty() {
                String::new()
            } else {
                format!(" ({unfinished})")
            },
            left.as_secs(),
        );
    });
    match shutdown::within(timeout, &progress, interrupt, stopping).await {
        Drained::Done(result) => result,
        Drained::TimedOut { step } => {
            let unfinished = shutdown::unfinished_sinks(&status.snapshot());
            let unbuilt = shutdown::unbuilt_indexes(&status.waiting_by_index());
            if !unbuilt.is_empty() {
                tracing::error!(
                    unbuilt = %unbuilt,
                    "the stop could not build everything it read ({unbuilt}); the next start reads \
                     it again"
                );
            }
            tracing::error!(
                step,
                unfinished = %unfinished,
                "shutdown deadline of {}s passed while {step}{}; exiting without it \
                 (whatever was not acknowledged is read again on the next start)",
                timeout.as_secs(),
                if step == shutdown::DRAINING && !unfinished.is_empty() {
                    format!(" ({unfinished})")
                } else {
                    String::new()
                },
            );
            waits.end_before_exit().await;
            std::process::exit(1);
        }
        Drained::Forced(signal) => {
            let unfinished = shutdown::unfinished_sinks(&status.snapshot());
            tracing::warn!(
                signal = signal.name(),
                "forced: exiting at once while {}{}; whatever was not acknowledged is read again \
                 on the next start",
                progress.current(),
                if unfinished.is_empty() {
                    String::new()
                } else {
                    format!(" ({unfinished})")
                },
            );
            waits.end_before_exit().await;
            std::process::exit(signal.exit_code());
        }
    }
}

/// Resolve the configuration a `run` should use, cargo-style: compile the config
/// and **(re)write the lock** when one is present (skipping the write when the
/// bytes are already identical, so a no-change run leaves a committed lock
/// untouched), fall back to the existing lock otherwise, or run the lock as-is
/// under `--locked`. A lock write failure is fatal — the committed lock must
/// reflect the config it was compiled from.
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
            let compiled = config::compile(&config_path)
                .with_context(|| format!("compiling config from {}", config_path.display()))?;
            let wrote = config::write_if_changed(&compiled, &args.lock)
                .with_context(|| format!("writing compiled lock to {}", args.lock.display()))?;
            tracing::info!(
                indexes = compiled.config.indexes.len(),
                lock = %args.lock.display(),
                "compiled config; {}",
                if wrote { "wrote lock" } else { "lock already current" }
            );
            Ok(compiled.config)
        }
        ConfigPlan::Fallback => load_lock(&args.lock).with_context(|| {
            format!(
                "{}; create a config or build a lock first",
                crate::lock::no_config_headline(&args.lock, " to compile")
            )
        }),
        ConfigPlan::Missing(config_path) => {
            anyhow::bail!("config file {} not found", config_path.display())
        }
    }
}

fn load_lock(path: &Path) -> anyhow::Result<Config> {
    config::load_compiled(path)
        .with_context(|| format!("loading compiled lock from {}", path.display()))
}

/// Lay `--backoff-*` over `FLUSSO_BACKOFF_*` over `[backoff]`, each value
/// keeping where it was set, then resolve and check them with the daemon's
/// one judge, so a bad pair fails before anything connects. The variables
/// are read here, not by clap, so the refusal can tell a flag from its
/// variable.
fn lay_backoff(
    config: &mut config::Config,
    initial_ms: Option<std::num::NonZeroU64>,
    max_ms: Option<std::num::NonZeroU64>,
    env: impl Fn(&str) -> Option<String>,
) -> anyhow::Result<()> {
    let layer = |flag: Option<std::num::NonZeroU64>,
                 name: &str,
                 file: Option<config::BackoffValue>|
     -> anyhow::Result<Option<config::BackoffValue>> {
        if let Some(ms) = flag {
            return Ok(Some(config::BackoffValue {
                ms,
                origin: config::Origin::Flag,
            }));
        }
        match env(name).filter(|text| !text.is_empty()) {
            Some(text) => {
                let ms = config::parse_backoff_ms(&text)
                    .map_err(|why| anyhow::anyhow!("{name}: {why}"))?;
                Ok(Some(config::BackoffValue {
                    ms,
                    origin: config::Origin::Env,
                }))
            }
            None => Ok(file),
        }
    };
    config.backoff.initial_ms = layer(
        initial_ms,
        "FLUSSO_BACKOFF_INITIAL_MS",
        config.backoff.initial_ms,
    )?;
    config.backoff.max_ms = layer(max_ms, "FLUSSO_BACKOFF_MAX_MS", config.backoff.max_ms)?;
    daemon::backoff(config)?;
    Ok(())
}

/// Lay `--index-budget-share` over `FLUSSO_INDEX_BUDGET_SHARE` over
/// `[concurrency] index_budget_share`, keeping where it was set, then check
/// it with the daemon's judge, so a bad share fails before anything
/// connects. The variable is read here, not by clap, so the refusal can tell
/// a flag from its variable.
fn lay_index_budget_share(
    config: &mut config::Config,
    flag: Option<f64>,
    env: impl Fn(&str) -> Option<String>,
) -> anyhow::Result<()> {
    const VARIABLE: &str = "FLUSSO_INDEX_BUDGET_SHARE";
    let laid = match flag {
        Some(share) => Some(config::ShareValue {
            share,
            origin: config::Origin::Flag,
        }),
        None => match env(VARIABLE).filter(|text| !text.is_empty()) {
            Some(text) => Some(config::ShareValue {
                share: config::parse_index_budget_share(&text)
                    .map_err(|why| anyhow::anyhow!("{VARIABLE}: {why}"))?,
                origin: config::Origin::Env,
            }),
            None => config.concurrency.index_budget_share,
        },
    };
    config.concurrency.index_budget_share = laid;
    daemon::index_budget_share(config)?;
    Ok(())
}

/// Lay `--max-waiting-per-index` and `--max-waiting` over their variables
/// over `[concurrency]`, keeping where each was set, then check the pair
/// with the daemon's judge, so a bad bound fails before anything connects.
/// The variables are read here, not by clap, so a refusal can tell a flag
/// from its variable.
fn lay_max_waiting(
    config: &mut config::Config,
    per_index: Option<u64>,
    total: Option<u64>,
    env: impl Fn(&str) -> Option<String>,
) -> anyhow::Result<()> {
    let layer = |flag: Option<u64>,
                 name: &str,
                 file: Option<config::CountValue>|
     -> anyhow::Result<Option<config::CountValue>> {
        if let Some(count) = flag {
            return Ok(Some(config::CountValue {
                count,
                origin: config::Origin::Flag,
            }));
        }
        match env(name).filter(|text| !text.is_empty()) {
            Some(text) => Ok(Some(config::CountValue {
                count: config::parse_max_waiting(&text)
                    .map_err(|why| anyhow::anyhow!("{name}: {why}"))?,
                origin: config::Origin::Env,
            })),
            None => Ok(file),
        }
    };
    config.concurrency.max_waiting_per_index = layer(
        per_index,
        "FLUSSO_MAX_WAITING_PER_INDEX",
        config.concurrency.max_waiting_per_index,
    )?;
    config.concurrency.max_waiting =
        layer(total, "FLUSSO_MAX_WAITING", config.concurrency.max_waiting)?;
    daemon::max_waiting(config)?;
    Ok(())
}

/// Lay `--max-pending-documents`, else `FLUSSO_MAX_PENDING_DOCUMENTS`, over
/// `[concurrency]`, keeping where it was set, then check it with the
/// daemon's judge, so 0 fails before anything connects.
fn lay_max_pending_documents(
    config: &mut config::Config,
    flag: Option<u64>,
    env: impl Fn(&str) -> Option<String>,
) -> anyhow::Result<()> {
    const NAME: &str = "FLUSSO_MAX_PENDING_DOCUMENTS";
    if let Some(count) = flag {
        config.concurrency.max_pending_documents = Some(config::CountValue {
            count,
            origin: config::Origin::Flag,
        });
    } else if let Some(text) = env(NAME).filter(|text| !text.is_empty()) {
        config.concurrency.max_pending_documents = Some(config::CountValue {
            count: config::parse_max_waiting(&text)
                .map_err(|why| anyhow::anyhow!("{NAME}: {why}"))?,
            origin: config::Origin::Env,
        });
    }
    daemon::max_pending_documents(config)?;
    Ok(())
}

/// Bind both listeners, then refuse two OpenSearch sinks on one cluster.
/// Binding first, before any connection: a taken or bad address fails at
/// once, never after the up to 10 s the clusters' answers may take (the
/// review of the closing round, C; of round 13, m-5). The listeners are
/// only bound here; nothing is served until the daemon has started.
async fn bind_then_compare_clusters(
    config: &Config,
    public_addr: SocketAddr,
    private_addr: SocketAddr,
) -> anyhow::Result<(tokio::net::TcpListener, tokio::net::TcpListener)> {
    let public_listener = http::bind("public", public_addr, Some(public_addr)).await?;
    let private_listener = http::bind("private", private_addr, None).await?;
    adapters::distinct_clusters_online(config).await?;
    Ok((public_listener, private_listener))
}

#[cfg(test)]
mod tests;

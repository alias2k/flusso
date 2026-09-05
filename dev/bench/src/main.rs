//! `flusso-bench`: run one scenario against the real `flusso` binary and
//! report the headline metrics.
//!
//! A run is four phases, each timed and capped:
//!
//! 1. **Seed.** Start Postgres and OpenSearch (or take them from
//!    `BENCH_PG_URL` / `BENCH_OS_URL`), create the scenario's store, seed it
//!    deterministically at the chosen scale.
//! 2. **Backfill.** Spawn `flusso run` and wait until `/status` reports `live`
//!    with every index seeded. Reports `backfill_docs_per_s`.
//! 3. **Latency.** A paced trickle of changes; each stamped root-row update is
//!    timed until its document is searchable. Reports `visible_latency_p50_ms`
//!    and `_p99_ms` — refresh included, as a user sees it.
//! 4. **Drain.** A burst of changes from several writers as fast as Postgres
//!    accepts; time until every sink has nothing in flight. Reports
//!    `drain_changes_per_s`.
//!
//! Throughout, the child's peak resident set and CPU time are sampled
//! (`peak_rss_mib`, `cpu_seconds`), and the flush-duration histogram from
//! `/metrics` gives the attribution figures (`flush_p50_ms`, `flush_p99_ms`).
//!
//! ```text
//! flusso-bench --scenario reference --scale ci --out target/bench/reference-ci
//! ```
//!
//! Needs Docker (or the two env URLs) and the `flusso` binary: `--flusso-bin`
//! or `FLUSSO_BIN`, else a release build of `flusso-cli --no-default-features`
//! is made first.

mod flusso;
mod probe;
mod report;
mod scale;
mod scenario;
mod services;
mod writer;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::Parser;
use sqlx::postgres::PgPoolOptions;

use crate::report::{Failure, Images, Report};
use crate::scale::{Scale, ScaleName};
use crate::scenario::{Scenario, ScenarioName};

#[derive(Debug, Parser)]
#[command(
    name = "flusso-bench",
    about = "Run a flusso benchmark scenario against the real binary"
)]
struct Args {
    /// The scenario to run.
    #[arg(long, value_enum, default_value_t = ScenarioName::Reference)]
    scenario: ScenarioName,
    /// The scale preset.
    #[arg(long, value_enum, default_value_t = ScaleName::Ci)]
    scale: ScaleName,
    /// Directory for `smaller.json`, `bigger.json`, `summary.json` (or
    /// `failure.json`).
    #[arg(long, default_value = "target/bench")]
    out: PathBuf,
    /// The `flusso` binary to drive; built from the workspace when omitted.
    #[arg(long, env = "FLUSSO_BIN")]
    flusso_bin: Option<PathBuf>,
    /// Seed for the change generator.
    #[arg(long, default_value_t = 42)]
    seed: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sqlx=warn,testcontainers=warn,bollard=warn".into()),
        )
        .with_writer(std::io::stderr)
        .init();
    let args = Args::parse();
    let scale = Scale::for_scenario(args.scenario, args.scale);
    let scenario = Arc::new(Scenario::new(args.scenario));
    let out = args.out.join(format!(
        "{}-{}",
        args.scenario.as_str(),
        args.scale.as_str()
    ));
    let bin = match args.flusso_bin {
        Some(bin) => bin,
        None => build_flusso().await?,
    };

    let mut phase = "setup".to_owned();
    match run(&scenario, &scale, &bin, args.seed, &mut phase).await {
        Ok(report) => {
            report.write(&out)?;
            report.log();
            tracing::info!(dir = %out.display(), "report written");
            Ok(())
        }
        Err(error) => {
            let failure = Failure {
                scenario: args.scenario.as_str().to_owned(),
                scale: args.scale.as_str().to_owned(),
                phase: phase.clone(),
                reason: format!("{error:#}"),
            };
            failure.write(&out)?;
            tracing::error!(phase, "run failed: {error:#}");
            Err(error)
        }
    }
}

/// `cargo build --release -p flusso-cli --no-default-features`, the server
/// image's configuration, and return the binary path.
async fn build_flusso() -> Result<PathBuf> {
    let root = workspace_root();
    tracing::info!("building flusso (release, no designer)");
    let status = tokio::process::Command::new("cargo")
        .args([
            "build",
            "--release",
            "-p",
            "flusso-cli",
            "--no-default-features",
        ])
        .current_dir(&root)
        .status()
        .await
        .context("running cargo build")?;
    if !status.success() {
        bail!("cargo build failed with {status}");
    }
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target"));
    let bin = target.join("release").join("flusso");
    if !bin.is_file() {
        bail!("built binary not found at {}", bin.display());
    }
    Ok(bin)
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("."))
}

async fn run(
    scenario: &Arc<Scenario>,
    scale: &Scale,
    bin: &Path,
    seed: u64,
    phase: &mut String,
) -> Result<Report> {
    let run_started = Instant::now();
    let wall_cap = Duration::from_secs(scale.wall_cap_secs);
    let remaining = |phase: &str| -> Result<Duration> {
        wall_cap
            .checked_sub(run_started.elapsed())
            .with_context(|| format!("the wall-clock cap {wall_cap:?} ran out before {phase}"))
    };

    // ── 1. seed ──────────────────────────────────────────────────────────────
    let services = services::start().await?;
    let pool = PgPoolOptions::new()
        .max_connections((scale.writers + 4) as u32)
        .connect(&services.pg_url)
        .await
        .context("connecting to Postgres")?;
    let seed_started = Instant::now();
    sqlx::raw_sql(scenario.schema_sql())
        .execute(&pool)
        .await
        .context("creating the scenario's tables")?;
    sqlx::raw_sql(sqlx::AssertSqlSafe(scenario.seed_sql(scale)))
        .execute(&pool)
        .await
        .context("seeding the scenario")?;
    let seed_seconds = seed_started.elapsed().as_secs_f64();
    let documents = scenario.root_documents(scale);
    tracing::info!(documents, seconds = %format!("{seed_seconds:.1}"), "seeded");

    // ── 2. backfill ──────────────────────────────────────────────────────────
    *phase = "backfill".to_owned();
    let workdir = tempfile::tempdir()?;
    let spawned = Instant::now();
    let mut child = flusso::Flusso::spawn(
        bin,
        &scenario.config_path(),
        workdir.path(),
        &services.pg_url,
        &services.os_url,
        scale.rss_cap_mib,
    )
    .await?;
    let backfill = child.wait_seeded(spawned, remaining("backfill")?).await?;
    let backfill_docs_per_s = documents as f64 / backfill.as_secs_f64();
    tracing::info!(
        seconds = %format!("{:.1}", backfill.as_secs_f64()),
        docs_per_s = %format!("{backfill_docs_per_s:.0}"),
        "backfilled"
    );

    // ── 3. latency ───────────────────────────────────────────────────────────
    *phase = "latency".to_owned();
    let ids = Arc::new(scenario.id_counters(scale));
    let probe_cap = Duration::from_secs(60).min(remaining("latency")?);
    let latencies = writer::probed_trickle(
        &pool,
        scenario,
        scale,
        &ids,
        &services.os_url,
        seed,
        probe_cap,
    )
    .await?;
    let p50 = latencies
        .percentile(0.5)
        .context("no latency samples")?
        .as_secs_f64()
        * 1_000.0;
    let p99 = latencies
        .percentile(0.99)
        .context("no latency samples")?
        .as_secs_f64()
        * 1_000.0;
    tracing::info!(
        probes = latencies.samples.len(),
        p50_ms = %format!("{p50:.1}"),
        p99_ms = %format!("{p99:.1}"),
        "latency measured"
    );

    // ── 4. drain ─────────────────────────────────────────────────────────────
    *phase = "drain".to_owned();
    child
        .wait_drained(0, 0, remaining("drain")?)
        .await
        .context("settling before the burst")?;
    let baseline = child.captured().await?;
    let burst = writer::burst(
        &pool,
        Arc::clone(scenario),
        Arc::new(scale.clone()),
        Arc::clone(&ids),
        seed,
    )
    .await?;
    let drained = child
        .wait_drained(baseline, burst.rows, remaining("drain")?)
        .await?;
    let drain_seconds = drained.duration_since(burst.started).as_secs_f64();
    let write_seconds = burst
        .writers_done
        .duration_since(burst.started)
        .as_secs_f64();
    let drain_changes_per_s = burst.rows as f64 / drain_seconds;
    tracing::info!(
        rows = burst.rows,
        statements = burst.statements,
        write_seconds = %format!("{write_seconds:.1}"),
        drain_seconds = %format!("{drain_seconds:.1}"),
        changes_per_s = %format!("{drain_changes_per_s:.0}"),
        "burst drained"
    );

    // ── attribution + resources ──────────────────────────────────────────────
    let metrics = child.metrics().await?;
    let flush_p50 =
        report::histogram_quantile(&metrics, "flusso_flush_duration_seconds", "primary", 0.5);
    let flush_p99 =
        report::histogram_quantile(&metrics, "flusso_flush_duration_seconds", "primary", 0.99);
    let resources = child.stop().await?;

    let mut report = Report {
        scenario: scenario.name.as_str().to_owned(),
        scale: serde_json::to_value(scale)?,
        images: Images {
            postgres: services.pg_image.clone(),
            opensearch: services.os_image.clone(),
        },
        phases: serde_json::json!({
            "seed_seconds": seed_seconds,
            "backfill_seconds": backfill.as_secs_f64(),
            "backfill_documents": documents,
            "latency_probes": latencies.samples.len(),
            "latency_changes": latencies.changes,
            "burst_rows": burst.rows,
            "burst_statements": burst.statements,
            "burst_write_seconds": write_seconds,
            "drain_seconds": drain_seconds,
            "total_seconds": run_started.elapsed().as_secs_f64(),
        }),
        smaller: Vec::new(),
        bigger: Vec::new(),
    };
    report.smaller("visible_latency_p50_ms", "ms", p50);
    report.smaller("visible_latency_p99_ms", "ms", p99);
    report.smaller("peak_rss_mib", "MiB", resources.peak_rss_mib);
    report.smaller("cpu_seconds", "s", resources.cpu_seconds);
    if let Some(p) = flush_p50 {
        report.smaller("flush_p50_ms", "ms", p * 1_000.0);
    }
    if let Some(p) = flush_p99 {
        report.smaller("flush_p99_ms", "ms", p * 1_000.0);
    }
    report.bigger("backfill_docs_per_s", "docs/s", backfill_docs_per_s);
    report.bigger("drain_changes_per_s", "changes/s", drain_changes_per_s);
    Ok(report)
}

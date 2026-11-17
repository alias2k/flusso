//! `--startup <COPIES>`: how long `flusso run` takes to reach `live` with many
//! indexes, the backfill skipped (issue #398).
//!
//! The scenario's indexes are each declared `COPIES` times under their own
//! names (`users_1`, `users_2`, …), so the deployment stages
//! `COPIES × indexes` indexes on one sink. Each of `--runs` rounds starts
//! twice:
//!
//! - **fresh**: no index, slot or publication, so every index is created;
//! - **restart**: the same deployment started again, every index already there.
//!
//! Each start is timed from spawning the binary to `/status` reporting `live`,
//! with `--skip-backfill`, and stopped with SIGTERM before the next. Beside
//! it, each start keeps the steps flusso timed itself
//! (`flusso.startup.duration`: `source`, `stage` per sink, `prepare`,
//! `open_live`, and `live` from `flusso run` starting), read from `/metrics`
//! once it is live; the outside figure also covers the process starting. The load
//! average at each start is recorded beside it, since the figures are only
//! comparable on a quiet machine. The store holds the scenario's tables and no
//! rows: nothing here reads them. Tables an earlier `--startup` run left are
//! reused, so the measurement can be repeated against one pair of services.
//!
//! ```text
//! flusso-bench --scenario reference --startup 8 --runs 3 --out target/bench
//! ```
//!
//! Writes `startup.json` (every start, and the min/median/max per kind). Not a
//! headline metric: `bench.yml` does not run it.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use sqlx::postgres::PgPoolOptions;

use crate::flusso::Flusso;
use crate::scenario::Scenario;
use crate::services;

/// The longest one start may take.
const START_CAP: Duration = Duration::from_secs(120);

/// No document is built, so the resident set is not what this measures.
const RSS_CAP_MIB: u64 = 4096;

/// One timed start.
#[derive(Debug, Clone, Serialize)]
struct Start {
    kind: &'static str,
    round: usize,
    seconds: f64,
    load_average: Option<f64>,
    /// The start-up steps flusso reported, in seconds.
    steps: std::collections::BTreeMap<String, f64>,
}

/// Min, median and max of one kind of start.
#[derive(Debug, Serialize)]
struct Spread {
    kind: &'static str,
    min: f64,
    median: f64,
    max: f64,
}

#[derive(Debug, Serialize)]
struct StartupReport {
    scenario: String,
    indexes: usize,
    images: crate::report::Images,
    starts: Vec<Start>,
    spread: Vec<Spread>,
}

/// Run the start-up measurement and write `out/startup.json`.
pub(crate) async fn run(
    scenario: &Scenario,
    copies: usize,
    runs: usize,
    bin: &Path,
    out: &Path,
) -> Result<()> {
    if copies == 0 || runs == 0 {
        bail!("--startup and --runs take at least 1");
    }
    let services = services::start().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&services.pg_url)
        .await
        .context("connecting to Postgres")?;
    // An earlier `--startup` run against the same store left the tables.
    if let Err(error) = sqlx::raw_sql(scenario.schema_sql()).execute(&pool).await {
        let existing =
            matches!(&error, sqlx::Error::Database(db) if db.code().as_deref() == Some("42P07"));
        if !existing {
            return Err(error).context("creating the scenario's tables");
        }
        tracing::info!("the scenario's tables exist; reusing them");
    }

    let workdir = tempfile::tempdir()?;
    let (config, indexes) = copied_config(&scenario.config_path(), copies, workdir.path())?;
    tracing::info!(indexes, "start-up: one sink, the backfill skipped");
    let client = reqwest::Client::new();

    let mut starts = Vec::new();
    for round in 1..=runs {
        reset(&pool, &client, &services.os_url).await?;
        for kind in ["fresh", "restart"] {
            let load_average = load_average();
            let (seconds, steps) = time_to_live(bin, &config, workdir.path(), &services).await?;
            let shown: Vec<String> = steps
                .iter()
                .map(|(step, seconds)| format!("{step} {seconds:.3}"))
                .collect();
            tracing::info!(
                kind,
                round,
                seconds = %format!("{seconds:.3}"),
                load_average = ?load_average,
                steps = %shown.join(", "),
                "reached live"
            );
            starts.push(Start {
                kind,
                round,
                seconds,
                load_average,
                steps,
            });
        }
    }

    let spread = ["fresh", "restart"]
        .into_iter()
        .filter_map(|kind| spread(kind, &starts))
        .collect();
    let report = StartupReport {
        scenario: scenario.name.as_str().to_owned(),
        indexes,
        images: crate::report::Images {
            postgres: services.pg_image.clone(),
            opensearch: services.os_image.clone(),
        },
        starts,
        spread,
    };
    for spread in &report.spread {
        tracing::info!(
            kind = spread.kind,
            min = %format!("{:.3}", spread.min),
            median = %format!("{:.3}", spread.median),
            max = %format!("{:.3}", spread.max),
            "time to live, seconds"
        );
    }
    std::fs::create_dir_all(out)?;
    let path = out.join("startup.json");
    std::fs::write(&path, serde_json::to_string_pretty(&report)?)?;
    tracing::info!(path = %path.display(), "report written");
    Ok(())
}

/// Spawn the binary, wait for `live`, stop it. Returns the seconds to `live`
/// and the start-up steps flusso reported.
async fn time_to_live(
    bin: &Path,
    config: &Path,
    workdir: &Path,
    services: &services::Services,
) -> Result<(f64, std::collections::BTreeMap<String, f64>)> {
    let spawned = Instant::now();
    let mut child = Flusso::spawn(
        bin,
        config,
        workdir,
        &services.pg_url,
        &services.os_url,
        RSS_CAP_MIB,
        &["--skip-backfill"],
    )
    .await?;
    let seconds = child.wait_live(spawned, START_CAP).await?.as_secs_f64();
    // `/status` turns live before the metrics observer records `live`: the
    // status observer hears the event first.
    let mut steps = std::collections::BTreeMap::new();
    for _ in 0..50 {
        steps = crate::attribution::Scrape::parse(&child.metrics().await?).startup_steps();
        if steps.contains_key("live") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    child.stop().await?;
    Ok((seconds, steps))
}

/// No index, slot or publication: what a first start on a fresh cluster finds.
async fn reset(pool: &sqlx::PgPool, client: &reqwest::Client, os_url: &str) -> Result<()> {
    sqlx::raw_sql(
        "SELECT pg_drop_replication_slot(slot_name) FROM pg_replication_slots \
         WHERE slot_name = 'flusso'; DROP PUBLICATION IF EXISTS flusso",
    )
    .execute(pool)
    .await
    .context("dropping the slot and the publication")?;
    let rows: serde_json::Value = client
        .get(format!("{os_url}/_cat/indices?format=json&h=index"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let names: Vec<&str> = rows
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row.get("index")?.as_str())
                .filter(|name| !name.starts_with('.'))
                .collect()
        })
        .unwrap_or_default();
    for chunk in names.chunks(50) {
        client
            .delete(format!("{os_url}/{}", chunk.join(",")))
            .send()
            .await?
            .error_for_status()
            .context("deleting the indexes of the last start")?;
    }
    Ok(())
}

/// The scenario's config with every `[[index]]` declared `copies` times, named
/// `<name>_<n>`, its schema path made absolute. Returns the written path and
/// how many indexes it declares.
fn copied_config(config: &Path, copies: usize, workdir: &Path) -> Result<(PathBuf, usize)> {
    let text =
        std::fs::read_to_string(config).with_context(|| format!("reading {}", config.display()))?;
    let base = config.parent().context("the config has no folder")?;
    let mut blocks = text.split("[[index]]");
    let mut copied = blocks.next().unwrap_or_default().to_owned();
    let mut indexes = 0;
    for block in blocks {
        let name = value_of(block, "name").context("an [[index]] without a name")?;
        let schema = value_of(block, "schema").context("an [[index]] without a schema")?;
        let schema = base
            .join(&schema)
            .canonicalize()
            .with_context(|| format!("finding the schema {schema}"))?;
        for copy in 1..=copies {
            copied.push_str("[[index]]");
            for line in block.lines() {
                let key = line.split('=').next().unwrap_or_default().trim();
                match key {
                    "name" => writeln!(copied, "name = \"{name}_{copy}\"")?,
                    "schema" => writeln!(copied, "schema = {:?}", schema.display().to_string())?,
                    _ => writeln!(copied, "{line}")?,
                }
            }
            indexes += 1;
        }
    }
    let path = workdir.join("flusso.toml");
    std::fs::write(&path, copied)?;
    Ok((path, indexes))
}

/// The string value of `key = "…"` in an `[[index]]` block.
fn value_of(block: &str, key: &str) -> Option<String> {
    block.lines().find_map(|line| {
        let (k, v) = line.split_once('=')?;
        (k.trim() == key).then(|| v.trim().trim_matches('"').to_owned())
    })
}

/// The one-minute load average, where `/proc/loadavg` exists.
fn load_average() -> Option<f64> {
    std::fs::read_to_string("/proc/loadavg")
        .ok()?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

fn spread(kind: &'static str, starts: &[Start]) -> Option<Spread> {
    let mut seconds: Vec<f64> = starts
        .iter()
        .filter(|start| start.kind == kind)
        .map(|start| start.seconds)
        .collect();
    seconds.sort_by(f64::total_cmp);
    Some(Spread {
        kind,
        min: *seconds.first()?,
        median: *seconds.get(seconds.len() / 2)?,
        max: *seconds.last()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_index_is_declared_once_per_copy_with_an_absolute_schema() {
        let dir = tempfile::tempdir().unwrap();
        let config = Path::new(env!("CARGO_MANIFEST_DIR")).join("scenarios/reference/flusso.toml");
        let (path, indexes) = copied_config(&config, 2, dir.path()).unwrap();
        let text = std::fs::read_to_string(path).unwrap();

        assert_eq!(indexes, 6);
        for name in ["users_1", "users_2", "products_1", "orders_2"] {
            assert!(text.contains(&format!("name = \"{name}\"")), "{text}");
        }
        assert!(!text.contains("../"), "{text}");
        assert!(text.contains("[sinks.primary]"), "{text}");
    }
}

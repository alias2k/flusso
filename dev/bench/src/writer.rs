//! The change writer: a paced, probed trickle for the latency phase, and a
//! concurrent burst for the drain phase. Every writer derives from one seed so
//! the mix is identical run to run.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use rand::SeedableRng;
use rand::rngs::StdRng;
use sqlx::PgPool;
use tokio::task::JoinSet;

use crate::probe;
use crate::scale::Scale;
use crate::scenario::{IdCounters, Scenario};

/// Visible latencies from the probed trickle.
#[derive(Debug)]
pub(crate) struct Latencies {
    pub(crate) samples: Vec<Duration>,
    pub(crate) changes: u64,
}

impl Latencies {
    pub(crate) fn percentile(&self, p: f64) -> Option<Duration> {
        if self.samples.is_empty() {
            return None;
        }
        let mut sorted = self.samples.clone();
        sorted.sort();
        let rank = ((sorted.len() as f64 - 1.0) * p).round() as usize;
        sorted.get(rank).copied()
    }
}

async fn execute(pool: &PgPool, sql: String) -> Result<u64> {
    let result = sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
        .execute(pool)
        .await
        .context("committing a change")?;
    Ok(result.rows_affected())
}

/// Commit changes at `probe_rate_per_s` until `probes` of them were root-row
/// updates timed to visibility.
pub(crate) async fn probed_trickle(
    pool: &PgPool,
    scenario: &Scenario,
    scale: &Scale,
    ids: &IdCounters,
    os_url: &str,
    seed: u64,
    probe_cap: Duration,
) -> Result<Latencies> {
    let client = reqwest::Client::new();
    let mut rng = StdRng::seed_from_u64(seed);
    let mut tasks: JoinSet<Result<Duration>> = JoinSet::new();
    let mut issued = 0u64;
    let mut changes = 0u64;
    let period = Duration::from_micros(1_000_000 / scale.probe_rate_per_s.max(1));
    let mut ticker = tokio::time::interval(period);
    while issued < scale.probes {
        ticker.tick().await;
        let change = scenario.change(&mut rng, scale, ids);
        execute(pool, change.sql).await?;
        let committed = Instant::now();
        changes += 1;
        if let Some(probe) = change.probe {
            issued += 1;
            let client = client.clone();
            let os_url = os_url.to_owned();
            tasks.spawn(async move {
                probe::wait_visible(&client, &os_url, &probe, committed, probe_cap).await
            });
        }
    }
    let mut samples = Vec::with_capacity(issued as usize);
    while let Some(joined) = tasks.join_next().await {
        samples.push(joined.context("a probe task panicked")??);
    }
    Ok(Latencies { samples, changes })
}

/// What the burst wrote.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Burst {
    pub(crate) started: Instant,
    pub(crate) writers_done: Instant,
    /// Rows the statements touched — one captured change each.
    pub(crate) rows: u64,
    pub(crate) statements: u64,
}

/// `scale.writers` sessions commit `scale.burst` changes between them as fast
/// as Postgres accepts.
pub(crate) async fn burst(
    pool: &PgPool,
    scenario: Arc<Scenario>,
    scale: Arc<Scale>,
    ids: Arc<IdCounters>,
    seed: u64,
) -> Result<Burst> {
    let writers = scale.writers.max(1);
    let per_writer = scale.burst / writers;
    if per_writer == 0 {
        bail!("burst {} is smaller than the writer count", scale.burst);
    }
    let started = Instant::now();
    let mut tasks: JoinSet<Result<u64>> = JoinSet::new();
    for w in 0..writers {
        let pool = pool.clone();
        let scenario = Arc::clone(&scenario);
        let scale = Arc::clone(&scale);
        let ids = Arc::clone(&ids);
        tasks.spawn(async move {
            let mut rng = StdRng::seed_from_u64(seed ^ (w + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let mut rows = 0u64;
            for _ in 0..per_writer {
                let change = scenario.change(&mut rng, &scale, &ids);
                rows += execute(&pool, change.sql).await?;
            }
            Ok(rows)
        });
    }
    let mut rows = 0u64;
    while let Some(joined) = tasks.join_next().await {
        rows += joined.context("a writer task panicked")??;
    }
    Ok(Burst {
        started,
        writers_done: Instant::now(),
        rows,
        statements: per_writer * writers,
    })
}

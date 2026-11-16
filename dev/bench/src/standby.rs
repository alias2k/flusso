//! The `sync-standby` scenario's emulated synchronous standby (issue #343).
//!
//! Behind a synchronous standby every commit is flushed to the WAL, then waits
//! for the standby's answer before new snapshots see it, and flusso holds the
//! change until then. Instead of a second server this names a standby that
//! never answers and cancels each waiting commit once it has waited
//! [`ROUND_TRIP`]: the commit is in the WAL that long before it is visible,
//! as behind a standby with that round trip. What it does not model is a real
//! standby's own throughput or jitter.
//!
//! Only the scenario's writers wait (their sessions set `synchronous_commit =
//! on`); every other session commits locally.

use std::time::Duration;

use anyhow::{Context, Result};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

/// How long each commit waits before the canceller releases it.
pub(crate) const ROUND_TRIP: Duration = Duration::from_millis(2);

/// The running emulation; dropping it stops releasing commits.
#[derive(Debug)]
pub(crate) struct Standby {
    canceller: tokio::task::JoinHandle<()>,
}

impl Drop for Standby {
    fn drop(&mut self) {
        self.canceller.abort();
    }
}

/// Name the standby, start releasing commits after [`ROUND_TRIP`], and return
/// a pool of writer sessions that wait for it.
pub(crate) async fn start(admin: &PgPool, url: &str, writers: u32) -> Result<(Standby, PgPool)> {
    for statement in [
        "ALTER SYSTEM SET synchronous_commit = 'local'",
        "ALTER SYSTEM SET synchronous_standby_names = 'bench_standby_never_answers'",
        "SELECT pg_reload_conf()",
    ] {
        sqlx::query(statement)
            .execute(admin)
            .await
            .with_context(|| format!("emulating a synchronous standby: {statement}"))?;
    }
    let pool = PgPoolOptions::new()
        .max_connections(writers)
        .after_connect(|connection, _| {
            Box::pin(async move {
                sqlx::raw_sql("SET synchronous_commit = on")
                    .execute(connection)
                    .await
                    .map(|_| ())
            })
        })
        .connect(url)
        .await
        .context("connecting the synchronous writers")?;
    let admin = admin.clone();
    let canceller = tokio::spawn(async move {
        loop {
            let released = sqlx::query(
                "SELECT pg_cancel_backend(pid) FROM pg_stat_activity \
                 WHERE wait_event = 'SyncRep' \
                 AND clock_timestamp() - query_start > make_interval(secs => $1)",
            )
            .bind(ROUND_TRIP.as_secs_f64())
            .execute(&admin)
            .await;
            if let Err(error) = released {
                tracing::warn!(%error, "releasing synchronous commits");
            }
            tokio::time::sleep(Duration::from_micros(250)).await;
        }
    });
    Ok((Standby { canceller }, pool))
}

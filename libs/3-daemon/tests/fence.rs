//! Fence e2e: a whole deployment (the real daemon, engines, and channel
//! stream) over a **real Postgres** and a **real OpenSearch**, proving the one
//! promise a fence makes: a read made right after it passes sees the commit.
//!
//! Every scenario follows what a client does. It commits, reads
//! `pg_current_wal_lsn()` on another connection, waits on the fence, then reads
//! OpenSearch **once, with no polling**. A stale read fails the test.
//!
//! - `applied`: a `GET _doc` right after the fence returns the new value.
//! - `searchable`: a `_search` right after the fence matches the new value.
//! - **Trailing WAL**: the position lies past the watched commit because an
//!   unwatched table was written in between; and a position with *no* watched
//!   commit behind it at all. Both rely on the source following idle WAL.
//! - **A split transaction**: with four changes per batch, a ten-row
//!   transaction spans three batches; the fence passes only once all ten rows
//!   are in.
//! - **Concurrent writers**: each fences on its own commit and never reads a
//!   stale document.
//! - **A stalled sibling sink**: a fence on the healthy sink passes, a fence on
//!   all sinks times out, and passes once the sibling recovers.
//! - **A commit visible later than its WAL** (issue #343): a fence never
//!   reports applied for a commit flusso read before it was visible.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo nextest run -p flusso-daemon --run-ignored all -E 'binary(fence)'
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use config::Config;
use daemon::{
    Backends, Daemon, DaemonOptions, Fence, FenceLevel, FenceOutcome, FenceRequest, Phase,
    SinkParts, SourceParts, Status, Unserved,
};
use kernel::{
    Column, ColumnName, DatabaseSchema, Envelope, Field, FieldName, FieldSource, FlussoType,
    IndexName, IndexSchema, PortEntry, Secret, SinkName, TableName,
};
use sink::{Sink, SinkOptions, WriteReport};
use sink_opensearch::OpensearchSink;
use source::SourceSpec;
use source_postgres::{PgDocumentBuilder, ReplicationConfig, WalChangeCapture};
use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::{AssertSqlSafe, Row};
use stream::Stream;
use stream_channel::ChannelStream;
use testcontainers_modules::testcontainers::core::wait::HttpWaitStrategy;
use testcontainers_modules::testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{GenericImage, ImageExt};
use tokio::sync::watch;

const WAIT: Duration = Duration::from_secs(10);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs Docker (starts Postgres and OpenSearch containers)"]
async fn a_read_after_a_fence_sees_the_commit() {
    let postgres = test_postgres::image()
        .with_cmd([
            "postgres",
            "-c",
            "wal_level=logical",
            "-c",
            "max_wal_senders=10",
            "-c",
            "max_replication_slots=10",
        ])
        .start()
        .await
        .unwrap();
    let pg_port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let pg_url = format!("postgres://postgres:postgres@127.0.0.1:{pg_port}/postgres");
    let pool = PgPoolOptions::new()
        .max_connections(16)
        .connect(&pg_url)
        .await
        .unwrap();

    let opensearch = GenericImage::new("opensearchproject/opensearch", "2")
        .with_exposed_port(9200.tcp())
        .with_wait_for(WaitFor::http(
            HttpWaitStrategy::new("/_cluster/health")
                .with_port(9200.tcp())
                .with_expected_status_code(200u16)
                .with_poll_interval(Duration::from_secs(1)),
        ))
        .with_env_var("discovery.type", "single-node")
        .with_env_var("DISABLE_SECURITY_PLUGIN", "true")
        .with_env_var("DISABLE_INSTALL_DEMO_CONFIG", "true")
        .with_env_var("OPENSEARCH_JAVA_OPTS", "-Xms512m -Xmx512m")
        .with_startup_timeout(Duration::from_secs(180))
        .start()
        .await
        .expect("opensearch container should start");
    let os_url = format!(
        "http://127.0.0.1:{}",
        opensearch.get_host_port_ipv4(9200).await.unwrap()
    );

    exec(&pool, "CREATE TABLE things (id int PRIMARY KEY, name text)").await;
    exec(&pool, "CREATE TABLE others (id int PRIMARY KEY, name text)").await;
    exec(&pool, "CREATE TABLE noise (id serial PRIMARY KEY, v text)").await;
    exec(
        &pool,
        "INSERT INTO things SELECT n, 'seed' FROM generate_series(1, 40) n",
    )
    .await;
    exec(&pool, "INSERT INTO others VALUES (1, 'seed')").await;
    exec(&pool, "CREATE PUBLICATION flusso FOR TABLE things, others").await;

    let (gate, gated) = watch::channel(true);
    let (others_gate, others_gated) = watch::channel(true);
    let backends = Arc::new(RealBackends {
        pg_url: pg_url.clone(),
        pg_port,
        os_url: os_url.clone(),
        audit: Arc::new(GatedSink {
            open: gated,
            others_open: others_gated,
        }),
        seeds: None,
    });
    let mut config = config();
    config.indexes.insert(
        others(),
        config::Index {
            enabled: true,
            schema: IndexSchema {
                table: TableName::try_new("others").unwrap(),
                ..schema()
            },
            on_error: None,
            sinks: BTreeMap::new(),
        },
    );
    let running = Daemon::new(config, backends)
        .with_options(DaemonOptions::default())
        .start()
        .await
        .unwrap();
    let fence = running.fence();
    let status = running.status();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let deployment = tokio::spawn(running.run(async {
        let _ = stopped.await;
    }));
    await_live(&status).await;

    let client = Client {
        pool: pool.clone(),
        fence: fence.clone(),
        http: reqwest::Client::new(),
        os_url,
    };

    // ── applied: a get right after the fence sees the write ──────────────────
    for round in 0..10 {
        let name = format!("applied-{round}");
        let position = client.rename(1, &name).await;
        client.fence(&position, FenceLevel::Applied, PRIMARY).await;
        assert_eq!(client.get(1).await.as_deref(), Some(name.as_str()));
    }

    // ── searchable: a search right after the fence matches the write ─────────
    for round in 0..3 {
        let name = format!("searchable-{round}");
        let position = client.rename(2, &name).await;
        client
            .fence(&position, FenceLevel::Searchable, PRIMARY)
            .await;
        assert_eq!(client.search(&name).await, vec![2], "{name}");
    }

    // ── a position past the watched commit: unwatched WAL followed it ────────
    exec(&pool, "UPDATE things SET name = 'trailing' WHERE id = 3").await;
    exec(&pool, "INSERT INTO noise (v) VALUES ('after the commit')").await;
    let position = client.position().await;
    let waited = client.fence(&position, FenceLevel::Applied, PRIMARY).await;
    assert_eq!(client.get(3).await.as_deref(), Some("trailing"));
    assert_prompt("a position behind unwatched WAL", waited);

    // ── a position with no watched commit behind it at all ───────────────────
    exec(&pool, "INSERT INTO noise (v) VALUES ('nothing watched')").await;
    let position = client.position().await;
    let waited = client
        .fence(&position, FenceLevel::Searchable, PRIMARY)
        .await;
    assert_prompt("a position with only unwatched WAL before it", waited);

    // ── a transaction split across batches is all in, or the fence holds ─────
    exec(
        &pool,
        "UPDATE things SET name = 'split' WHERE id BETWEEN 11 AND 20",
    )
    .await;
    let position = client.position().await;
    client.fence(&position, FenceLevel::Applied, PRIMARY).await;
    for id in 11..=20 {
        assert_eq!(client.get(id).await.as_deref(), Some("split"), "row {id}");
    }

    // ── concurrent writers each read their own write ─────────────────────────
    let writers: Vec<_> = (21..=28)
        .map(|id| {
            let client = client.clone();
            tokio::spawn(async move {
                for round in 0..15 {
                    let name = format!("writer-{id}-{round}");
                    let position = client.rename(id, &name).await;
                    client.fence(&position, FenceLevel::Applied, PRIMARY).await;
                    assert_eq!(client.get(id).await.as_deref(), Some(name.as_str()));
                }
            })
        })
        .collect();
    for writer in writers {
        writer.await.unwrap();
    }

    // ── every sink: passes while the sibling keeps up ────────────────────────
    let position = client.rename(4, "both").await;
    client.fence(&position, FenceLevel::Searchable, None).await;

    // ── a stalled sibling blocks only the fences that include it ─────────────
    gate.send(false).unwrap();
    let position = client.rename(5, "stalled").await;
    client.fence(&position, FenceLevel::Applied, PRIMARY).await;
    assert_eq!(client.get(5).await.as_deref(), Some("stalled"));
    let all = fence
        .wait(&FenceRequest {
            position: position.clone(),
            level: FenceLevel::Applied,
            sink: None,
            indexes: Vec::new(),
            timeout: Duration::from_millis(500),
        })
        .await
        .unwrap();
    assert!(
        matches!(all, FenceOutcome::TimedOut { .. }),
        "the audit sink has not flushed: {all:?}"
    );
    gate.send(true).unwrap();
    client.fence(&position, FenceLevel::Applied, None).await;

    // ── a stalled index on a sink blocks only the fences that include it ─────
    others_gate.send(false).unwrap();
    exec(&pool, "UPDATE others SET name = 'stalled' WHERE id = 1").await;
    let position = client.rename(6, "beside a stalled index").await;
    let on = |indexes: Vec<IndexName>| FenceRequest {
        position: position.clone(),
        level: FenceLevel::Applied,
        sink: Some(SinkName::try_new("audit").unwrap()),
        indexes,
        timeout: Duration::from_millis(500),
    };
    let healthy = fence.wait(&on(vec![things()])).await.unwrap();
    assert!(
        matches!(healthy, FenceOutcome::Reached { .. }),
        "the things lane of audit landed: {healthy:?}"
    );
    for indexes in [vec![others()], vec![things(), others()], Vec::new()] {
        let outcome = fence.wait(&on(indexes.clone())).await.unwrap();
        assert!(
            matches!(outcome, FenceOutcome::TimedOut { .. }),
            "{indexes:?} includes the stalled others lane: {outcome:?}"
        );
    }
    let snapshot = serde_json::to_value(status.snapshot()).unwrap();
    assert_eq!(
        snapshot["sinks"]["audit"]["lanes"]["others"]["outstanding"], 1,
        "{snapshot}"
    );
    // The stalled lane ages, pins the slot and holds its WAL; its sibling on
    // the same sink keeps writing and doesn't age (ADR 0012 stage 8c).
    tokio::time::sleep(Duration::from_millis(3_500)).await;
    let snapshot = serde_json::to_value(status.snapshot()).unwrap();
    let lanes = &snapshot["sinks"]["audit"]["lanes"];
    assert!(
        lanes["others"]["stall_seconds"].as_f64().unwrap() >= 2.0,
        "the stalled lane ages: {snapshot}"
    );
    assert_eq!(
        lanes["things"]["stall_seconds"].as_f64(),
        Some(0.0),
        "the sibling lane writes: {snapshot}"
    );
    assert_eq!(
        snapshot["pinned_by"],
        serde_json::json!({ "index": "others", "sink": "audit" }),
        "{snapshot}"
    );
    others_gate.send(true).unwrap();
    let reached = fence
        .wait(&FenceRequest {
            timeout: WAIT,
            ..on(Vec::new())
        })
        .await
        .unwrap();
    assert!(
        matches!(reached, FenceOutcome::Reached { .. }),
        "{reached:?}"
    );
    eventually("the lane to recover", async || {
        let snapshot = serde_json::to_value(status.snapshot()).unwrap();
        snapshot["sinks"]["audit"]["lanes"]["others"]["stall_seconds"].as_f64() == Some(0.0)
            && snapshot["pinned_by"].is_null()
    })
    .await;

    let snapshot = serde_json::to_value(status.snapshot()).unwrap();
    assert!(snapshot["applied_position"].is_string(), "{snapshot}");
    assert!(
        snapshot["sinks"]["primary"]["searchable_position"].is_string(),
        "{snapshot}"
    );

    let _ = stop.send(());
    deployment.await.unwrap().unwrap();
}

/// Issue #343: a fence never covers a commit flusso read before it was visible.
///
/// A commit is held between its WAL flush and its visibility (a synchronous
/// standby that never answers). What a client can know, honestly:
///
/// - The **committing** session cannot read `pg_current_wal_lsn()` while its
///   commit waits. ADR 0009's client reads the position after its commit
///   returns, and by then the transaction is visible. Once that fence reports
///   `applied`, a read by id must return the committed value.
/// - **Any other** session can read a position past the commit record while
///   the commit waits (the record is already flushed). A fence on it must not
///   report `applied` before the transaction is visible: flusso cannot have
///   indexed state no snapshot shows yet, so `applied` would be a claim about
///   the state before the commit. The wait is bounded at three seconds, ample
///   for a pipeline that reads early to build and apply (milliseconds).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs Docker (starts Postgres and OpenSearch containers)"]
async fn a_fence_never_covers_a_commit_read_before_it_was_visible() {
    let (_postgres, pg_port, pg_url) = start_postgres().await;
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(&pg_url)
        .await
        .unwrap();
    let (_opensearch, os_url) = start_opensearch().await;
    exec(&pool, "CREATE TABLE things (id int PRIMARY KEY, name text)").await;
    exec(&pool, "INSERT INTO things VALUES (1, 'seed')").await;
    exec(&pool, "CREATE PUBLICATION flusso FOR TABLE things").await;

    let (_gate, gated) = watch::channel(true);
    let running = Daemon::new(
        config(),
        Arc::new(RealBackends {
            pg_url: pg_url.clone(),
            pg_port,
            os_url: os_url.clone(),
            audit: Arc::new(GatedSink {
                open: gated.clone(),
                others_open: gated,
            }),
            seeds: None,
        }),
    )
    .start()
    .await
    .unwrap();
    let fence = running.fence();
    let status = running.status();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let deployment = tokio::spawn(running.run(async {
        let _ = stopped.await;
    }));
    await_live(&status).await;
    let client = Client {
        pool: pool.clone(),
        fence: fence.clone(),
        http: reqwest::Client::new(),
        os_url,
    };
    let position = client.rename(1, "before").await;
    client.fence(&position, FenceLevel::Applied, PRIMARY).await;

    let sync = SyncRep::configure(&pool).await;
    let held = sync
        .hold("UPDATE things SET name = 'held' WHERE id = 1")
        .await;
    sent_to(&pool, "flusso").await;
    let past_the_commit = client.position().await;
    let while_held = fence
        .wait(&FenceRequest {
            position: past_the_commit.clone(),
            level: FenceLevel::Applied,
            sink: PRIMARY.map(|name| SinkName::try_new(name).unwrap()),
            indexes: Vec::new(),
            timeout: Duration::from_secs(3),
        })
        .await
        .unwrap();
    let read_while_held = client.get(1).await;
    held.release().await;

    let after_commit = client.position().await;
    client
        .fence(&after_commit, FenceLevel::Applied, PRIMARY)
        .await;
    let read_after = client.get(1).await;

    assert!(
        matches!(while_held, FenceOutcome::TimedOut { .. }),
        "a fence on {past_the_commit}, past a commit not yet visible, answered \
         {while_held:?} while the index held {read_while_held:?}; after the commit \
         returned, a fence on {after_commit} then read {read_after:?}"
    );
    assert_eq!(
        read_after.as_deref(),
        Some("held"),
        "a read after an applied fence on {after_commit} sees the commit"
    );

    let _ = stop.send(());
    deployment.await.unwrap().unwrap();
}

async fn start_postgres() -> (test_postgres::Container, u16, String) {
    let postgres = test_postgres::image()
        .with_cmd([
            "postgres",
            "-c",
            "wal_level=logical",
            "-c",
            "max_wal_senders=10",
            "-c",
            "max_replication_slots=10",
        ])
        .start()
        .await
        .unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    (postgres, port, url)
}

async fn start_opensearch() -> (
    testcontainers_modules::testcontainers::ContainerAsync<GenericImage>,
    String,
) {
    let opensearch = GenericImage::new("opensearchproject/opensearch", "2")
        .with_exposed_port(9200.tcp())
        .with_wait_for(WaitFor::http(
            HttpWaitStrategy::new("/_cluster/health")
                .with_port(9200.tcp())
                .with_expected_status_code(200u16)
                .with_poll_interval(Duration::from_secs(1)),
        ))
        .with_env_var("discovery.type", "single-node")
        .with_env_var("DISABLE_SECURITY_PLUGIN", "true")
        .with_env_var("DISABLE_INSTALL_DEMO_CONFIG", "true")
        .with_env_var("OPENSEARCH_JAVA_OPTS", "-Xms512m -Xmx512m")
        .with_startup_timeout(Duration::from_secs(180))
        .start()
        .await
        .expect("opensearch container should start");
    let url = format!(
        "http://127.0.0.1:{}",
        opensearch.get_host_port_ipv4(9200).await.unwrap()
    );
    (opensearch, url)
}

const PRIMARY: Option<&str> = Some("primary");

/// Commits held between their WAL flush and their visibility (issue #343).
///
/// Postgres flushes a commit record, then waits for synchronous standbys, and
/// only then takes the transaction out of the proc array, which is when new
/// snapshots see it. A logical walsender decodes up to the flushed WAL, so the
/// change reaches a replication client while every other session still reads
/// the state before it. [`configure`](Self::configure) names a synchronous
/// standby that never answers; a commit made with `synchronous_commit = on`
/// then stays in that window until [`Held::release`] cancels its wait.
/// Every other session commits locally and is never held.
///
/// The same helper as `flusso-engine`'s `tests/common`; keep the two alike.
///
/// It changes server settings with `ALTER SYSTEM`. Each test starts its own
/// container, so they never reach another test.
struct SyncRep {
    pool: sqlx::PgPool,
}

impl SyncRep {
    /// Name the standby that never answers.
    async fn configure(pool: &sqlx::PgPool) -> Self {
        configure(
            pool,
            &[
                "ALTER SYSTEM SET synchronous_commit = 'local'",
                "ALTER SYSTEM SET synchronous_standby_names = 'nobody'",
            ],
        )
        .await;
        standbys_in_force(pool).await;
        Self { pool: pool.clone() }
    }

    /// Run `sql` as one transaction and return once its commit is flushed and
    /// waiting on the standby: in the WAL, not visible.
    async fn hold(&self, sql: &str) -> Held {
        self.hold_statement(format!("BEGIN; {sql}; COMMIT;")).await
    }

    /// Run `statement` as written (a `COMMIT PREPARED`, say) and return once it
    /// waits on the standby.
    async fn hold_statement(&self, statement: String) -> Held {
        let mut connection = self.pool.acquire().await.unwrap().detach();
        let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut connection)
            .await
            .unwrap();
        eventually("the reloaded standby names", async || {
            let names: String = sqlx::query_scalar("SHOW synchronous_standby_names")
                .fetch_one(&mut connection)
                .await
                .unwrap();
            names == "nobody"
        })
        .await;
        sqlx::raw_sql("SET synchronous_commit = on")
            .execute(&mut connection)
            .await
            .unwrap();
        let commit = tokio::spawn(async move {
            sqlx::raw_sql(sqlx::AssertSqlSafe(statement))
                .execute(&mut connection)
                .await
                .unwrap();
        });
        eventually("the commit waiting on its standby", async || {
            sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS (SELECT 1 FROM pg_stat_activity \
                 WHERE pid = $1 AND wait_event = 'SyncRep')",
            )
            .bind(pid)
            .fetch_one(&self.pool)
            .await
            .unwrap()
        })
        .await;
        Held {
            pool: self.pool.clone(),
            pid,
            commit,
        }
    }
}

/// One held commit.
struct Held {
    pool: sqlx::PgPool,
    pid: i32,
    commit: tokio::task::JoinHandle<()>,
}

impl Held {
    /// Cancel this commit's standby wait alone: its transaction leaves the proc
    /// array and becomes visible, and its session returns.
    async fn release(self) {
        let cancelled: bool = sqlx::query_scalar("SELECT pg_cancel_backend($1)")
            .bind(self.pid)
            .fetch_one(&self.pool)
            .await
            .unwrap();
        assert!(cancelled, "the held session {} is gone", self.pid);
        tokio::time::timeout(std::time::Duration::from_secs(10), self.commit)
            .await
            .expect("the held commit returns once its wait is cancelled")
            .unwrap();
    }
}

/// Wait until the replication client named `application_name` was sent every
/// WAL byte written so far.
async fn sent_to(pool: &sqlx::PgPool, application_name: &str) {
    let flushed: String = sqlx::query_scalar("SELECT pg_current_wal_flush_lsn()::text")
        .fetch_one(pool)
        .await
        .unwrap();
    eventually("the WAL sent to the replication client", async || {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM pg_stat_replication \
             WHERE application_name = $1 AND sent_lsn >= $2::pg_lsn)",
        )
        .bind(application_name)
        .bind(&flushed)
        .fetch_one(pool)
        .await
        .unwrap()
    })
    .await;
}

/// Wait until Postgres makes commits wait for synchronous standbys, as
/// `synchronous_standby_names` now asks.
///
/// A session reads the reloaded setting at once, but whether a commit waits
/// is decided by a shared flag the checkpointer sets when it processes the
/// reload, later: a commit in between doesn't wait (11 of 30 commits made just
/// after the reload, measured on Postgres 16). So commit a write to an
/// unpublished probe table until one is seen waiting, then cancel its wait.
/// The reload signal also reaches each session a little later, so every write
/// the helper doesn't mean to hold sets its commit level itself.
async fn standbys_in_force(pool: &sqlx::PgPool) {
    // Locally: a session the reload signal hasn't reached yet still has
    // `synchronous_commit = on`, and its commit would wait forever.
    sqlx::raw_sql(
        "BEGIN; SET LOCAL synchronous_commit = local; \
         CREATE TABLE IF NOT EXISTS flusso_syncrep_probe (x int); COMMIT;",
    )
    .execute(pool)
    .await
    .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let mut connection = pool.acquire().await.unwrap().detach();
        let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut connection)
            .await
            .unwrap();
        sqlx::raw_sql("SET synchronous_commit = on")
            .execute(&mut connection)
            .await
            .unwrap();
        let probe = tokio::spawn(async move {
            let _ = sqlx::raw_sql("INSERT INTO flusso_syncrep_probe VALUES (1)")
                .execute(&mut connection)
                .await;
        });
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM pg_stat_activity \
                 WHERE pid = $1 AND wait_event = 'SyncRep')",
            )
            .bind(pid)
            .fetch_one(pool)
            .await
            .unwrap();
            if waiting {
                sqlx::query("SELECT pg_cancel_backend($1)")
                    .bind(pid)
                    .execute(pool)
                    .await
                    .unwrap();
                let _ = probe.await;
                return;
            }
            if probe.is_finished() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "commits never started waiting for the synchronous standby"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

/// `ALTER SYSTEM` each statement (one per call: it refuses a transaction
/// block), then reload the configuration.
async fn configure(pool: &sqlx::PgPool, statements: &[&str]) {
    for statement in statements.iter().chain(&["SELECT pg_reload_conf()"]) {
        sqlx::query(sqlx::AssertSqlSafe(*statement))
            .execute(pool)
            .await
            .unwrap();
    }
}

/// Poll `check` until it holds, failing after ten seconds with `what`.
async fn eventually(what: &str, mut check: impl AsyncFnMut() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !check().await {
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

/// How long a fence may take when nothing stands in its way. Generous: the
/// point is that it is not the timeout, or a keepalive interval away.
fn assert_prompt(what: &str, waited: Duration) {
    eprintln!("{what}: reached after {waited:?}");
    assert!(
        waited < Duration::from_secs(3),
        "{what} took {waited:?} to be reached"
    );
}

/// What an application does: write, read the position, fence, read once.
#[derive(Clone)]
struct Client {
    pool: PgPool,
    fence: Fence,
    http: reqwest::Client,
    os_url: String,
}

impl Client {
    /// Commit a new name for `id`, then read the position on another
    /// connection: after the commit, outside the transaction.
    async fn rename(&self, id: i32, name: &str) -> String {
        sqlx::query("UPDATE things SET name = $1 WHERE id = $2")
            .bind(name)
            .bind(id)
            .execute(&self.pool)
            .await
            .unwrap();
        self.position().await
    }

    async fn position(&self) -> String {
        sqlx::query("SELECT pg_current_wal_lsn()::text AS lsn")
            .fetch_one(&self.pool)
            .await
            .unwrap()
            .get("lsn")
    }

    /// Wait on the fence; panics unless it is reached. Returns how long it took.
    async fn fence(&self, position: &str, level: FenceLevel, sink: Option<&str>) -> Duration {
        let started = Instant::now();
        let outcome = self
            .fence
            .wait(&FenceRequest {
                position: position.to_owned(),
                level,
                sink: sink.map(|name| SinkName::try_new(name).unwrap()),
                indexes: Vec::new(),
                timeout: WAIT,
            })
            .await
            .unwrap();
        assert!(
            matches!(outcome, FenceOutcome::Reached { .. }),
            "fence on {position} ({level}, {sink:?}): {outcome:?}"
        );
        started.elapsed()
    }

    /// One realtime get, no retry.
    async fn get(&self, id: i32) -> Option<String> {
        let body: serde_json::Value = self
            .http
            .get(format!("{}/things/_doc/{id}", self.os_url))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        body["_source"]["name"].as_str().map(str::to_owned)
    }

    /// One search for the documents named `name`, no retry: their ids.
    async fn search(&self, name: &str) -> Vec<i64> {
        let body: serde_json::Value = self
            .http
            .post(format!("{}/things/_search", self.os_url))
            .json(&serde_json::json!({ "query": { "term": { "name": name } } }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        body["hits"]["hits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|hit| hit["_id"].as_str().unwrap().parse().unwrap())
            .collect()
    }
}

async fn await_live(status: &Status) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while status.snapshot().phase != Phase::Live {
        assert!(
            Instant::now() < deadline,
            "the deployment never went live: {:?}",
            status.snapshot()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn exec(pool: &PgPool, sql: &str) {
    sqlx::query(AssertSqlSafe(sql.to_owned()))
        .execute(pool)
        .await
        .unwrap();
}

fn things() -> IndexName {
    IndexName::try_new("things").unwrap()
}

fn others() -> IndexName {
    IndexName::try_new("others").unwrap()
}

fn schema() -> IndexSchema {
    let keyword = |name: &str| Field {
        field: FieldName::try_new(name).unwrap(),
        options: Default::default(),
        emit: true,
        source: FieldSource::Column(Column {
            column: ColumnName::try_new(name).unwrap(),
            ty: FlussoType::Keyword,
            nullable: true,
            transforms: Vec::new(),
            default: None,
            enum_order: Vec::new(),
            subfields: true,
            indexed: true,
            array: false,
        }),
    };
    IndexSchema {
        version: 1,
        table: TableName::try_new("things").unwrap(),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(ColumnName::try_new("id").unwrap()),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![keyword("id"), keyword("name")],
    }
}

/// Four changes per batch, so the ten-row transaction spans three batches.
fn config() -> Config {
    let mut config = Config {
        source: PortEntry::new("postgres"),
        stream: PortEntry::new(config::DEFAULT_STREAM_KIND),
        sinks: BTreeMap::new(),
        indexes: BTreeMap::from([(
            things(),
            config::Index {
                enabled: true,
                schema: schema(),
                on_error: None,
                sinks: BTreeMap::new(),
            },
        )]),
        on_error: Default::default(),
        batch: Default::default(),
        concurrency: Default::default(),
        server: Default::default(),
        prefix: String::new(),
    };
    config.batch.max_changes = NonZeroUsize::new(4);
    config
}

/// The real adapters, assembled the way the CLI's composition root does: the
/// Postgres source, the channel stream, an OpenSearch sink named `primary`,
/// and a second sink, `audit`, that a test can stall.
#[derive(Debug)]
struct RealBackends {
    pg_url: String,
    pg_port: u16,
    os_url: String,
    audit: Arc<GatedSink>,
    /// When set, `primary`'s seeds wait while it is closed.
    seeds: Option<watch::Receiver<bool>>,
}

#[async_trait]
impl Backends for RealBackends {
    fn validate(&self, _: &Config) -> anyhow::Result<()> {
        Ok(())
    }

    async fn source(&self, config: Arc<Config>, _: &DaemonOptions) -> anyhow::Result<SourceParts> {
        let spec = Arc::new(SourceSpec::new(
            config
                .indexes
                .iter()
                .map(|(name, index)| (name.clone(), index.schema.clone()))
                .collect(),
        ));
        let replication = ReplicationConfig::new(
            "127.0.0.1",
            "postgres",
            "postgres",
            "postgres",
            "flusso",
            "flusso",
        )
        .with_port(self.pg_port);
        Ok(SourceParts {
            capture: Arc::new(WalChangeCapture::new(replication, self.pg_url.clone())),
            documents: Arc::new(PgDocumentBuilder::connect(&self.pg_url, spec).await?),
            resolver: None,
        })
    }

    fn stream(&self, _: &Config, lanes: &[stream::LaneKey]) -> anyhow::Result<Arc<dyn Stream>> {
        Ok(Arc::new(ChannelStream::new(1024, lanes.iter().cloned())))
    }

    async fn sinks(&self, _: &Config) -> anyhow::Result<Vec<SinkParts>> {
        let primary = SinkName::try_new("primary").unwrap();
        let opensearch = OpensearchSink::from_config(
            &primary,
            &sink_opensearch::OpensearchConfig {
                url: Secret::Value(self.os_url.clone()),
                username: None,
                password: None,
                tls_verify: false,
                ca_file: None,
                batch_size: 1000,
                max_bytes: 10 * 1024 * 1024,
                timeout_secs: 30,
                max_retries: 3,
                pipeline: None,
                number_of_shards: 1,
                number_of_replicas: 0,
                refresh_interval: "1s".to_owned(),
                text_analysis: sink_opensearch::TextAnalysis::Builtin,
                auto_subfields: true,
                index_settings: Default::default(),
            },
        )?;
        let opensearch: Arc<dyn Sink> = match &self.seeds {
            Some(open) => Arc::new(SeedGated {
                inner: opensearch,
                open: open.clone(),
            }),
            None => Arc::new(opensearch),
        };
        Ok(vec![
            SinkParts {
                name: primary,
                sink: opensearch,
                options: SinkOptions::default(),
            },
            SinkParts {
                name: SinkName::try_new("audit").unwrap(),
                sink: Arc::clone(&self.audit) as Arc<dyn Sink>,
                options: SinkOptions { backfill: false },
            },
        ])
    }
}

/// A sink whose writes wait while its gate is closed: a destination that
/// stopped answering. Writes of the `others` index also wait on their own
/// gate: one index stuck while the sink's others go on.
#[derive(Debug)]
struct GatedSink {
    open: watch::Receiver<bool>,
    others_open: watch::Receiver<bool>,
}

#[async_trait]
impl Sink for GatedSink {
    async fn write(&self, envelopes: &[Envelope], _caught_up: bool) -> sink::Result<WriteReport> {
        let mut open = self.open.clone();
        let _ = open.wait_for(|open| *open).await;
        if envelopes.first().is_some_and(|e| e.index == others()) {
            let mut open = self.others_open.clone();
            let _ = open.wait_for(|open| *open).await;
        }
        Ok(WriteReport::clean())
    }
}

/// The OpenSearch sink, its seeds (the refresh and alias move that end a
/// backfill) waiting while a gate is closed: a backfill that takes as long as
/// a test needs. Every other call goes straight through.
#[derive(Debug)]
struct SeedGated {
    inner: OpensearchSink,
    open: watch::Receiver<bool>,
}

#[async_trait]
impl Sink for SeedGated {
    async fn ensure_index(&self, mapping: &kernel::IndexMapping) -> sink::Result<()> {
        self.inner.ensure_index(mapping).await
    }

    async fn write(&self, envelopes: &[Envelope], caught_up: bool) -> sink::Result<WriteReport> {
        self.inner.write(envelopes, caught_up).await
    }

    async fn write_parts(
        &self,
        parts: &[&[Envelope]],
        caught_up: sink::CaughtUp<'_>,
    ) -> sink::Result<WriteReport> {
        self.inner.write_parts(parts, caught_up).await
    }

    fn write_limit(&self) -> Option<usize> {
        self.inner.write_limit()
    }

    async fn is_seeded(&self, index: &IndexName) -> sink::Result<bool> {
        self.inner.is_seeded(index).await
    }

    async fn seeding(&self, index: &IndexName) -> sink::Result<sink::Seeding> {
        self.inner.seeding(index).await
    }

    async fn record_walked(&self, index: &IndexName, mark: &str) -> sink::Result<()> {
        self.inner.record_walked(index, mark).await
    }

    async fn forget_walked(&self, index: &IndexName) -> sink::Result<()> {
        self.inner.forget_walked(index).await
    }

    async fn mark_seeded(&self, index: &IndexName) -> sink::Result<()> {
        let mut open = self.open.clone();
        let _ = open.wait_for(|open| *open).await;
        self.inner.mark_seeded(index).await
    }

    async fn reindex(&self, mapping: &kernel::IndexMapping) -> sink::Result<()> {
        self.inner.reindex(mapping).await
    }

    fn ordered(&self) -> bool {
        self.inner.ordered()
    }
}

/// Wait until the source follows live changes: fences can be asked.
async fn await_fences(fence: &Fence) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let asked = fence
            .wait(&FenceRequest {
                position: "0/0".to_owned(),
                level: FenceLevel::Applied,
                sink: None,
                indexes: Vec::new(),
                timeout: Duration::ZERO,
            })
            .await;
        if asked.is_ok() {
            return;
        }
        assert!(Instant::now() < deadline, "fences never became available");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// A fence on `things` at `primary`, applied, bounded at 1.5 s.
async fn held(fence: &Fence, position: &str) -> FenceOutcome {
    fence
        .wait(&FenceRequest {
            position: position.to_owned(),
            level: FenceLevel::Applied,
            sink: PRIMARY.map(|name| SinkName::try_new(name).unwrap()),
            indexes: vec![things()],
            timeout: Duration::from_millis(1_500),
        })
        .await
        .unwrap()
}

/// Whether `outcome` says the `things` lane can't vouch, for `reason`.
fn unserved_as(outcome: &FenceOutcome, reason: Unserved) -> bool {
    matches!(outcome, FenceOutcome::Unserved { lanes, .. }
        if lanes.iter().any(|(lane, why)| lane.index == things() && *why == reason))
}

/// A running deployment, as a test drives it.
struct Running {
    fence: Fence,
    status: Arc<Status>,
    control: daemon::DaemonControl,
    stop: tokio::sync::oneshot::Sender<()>,
    deployment: tokio::task::JoinHandle<anyhow::Result<()>>,
}

impl Running {
    async fn start(backends: &Arc<RealBackends>) -> Self {
        let running = Daemon::new(config(), Arc::clone(backends) as Arc<dyn Backends>)
            .start()
            .await
            .unwrap();
        let fence = running.fence();
        let status = running.status();
        let control = running.control();
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let deployment = tokio::spawn(running.run(async {
            let _ = stopped.await;
        }));
        await_fences(&fence).await;
        Self {
            fence,
            status,
            control,
            stop,
            deployment,
        }
    }

    async fn stop(self) {
        let _ = self.stop.send(());
        self.deployment.await.unwrap().unwrap();
    }
}

/// Issue #379: a fence never answers reached for a position reads don't see
/// while the sink seeds or rebuilds the index: a first backfill, an on-demand
/// reindex, and a rebuild after the slot was dropped while flusso was
/// stopped. Each seed is held back by a gate, so the window is as long as
/// the test needs; a read right after the fence is reached sees the commit.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs Docker (starts Postgres and OpenSearch containers)"]
async fn a_fence_holds_while_the_index_is_seeded_or_rebuilt() {
    let (_postgres, pg_port, pg_url) = start_postgres().await;
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(&pg_url)
        .await
        .unwrap();
    let (_opensearch, os_url) = start_opensearch().await;
    exec(&pool, "CREATE TABLE things (id int PRIMARY KEY, name text)").await;
    exec(
        &pool,
        "INSERT INTO things SELECT n, 'seed' FROM generate_series(1, 10) n",
    )
    .await;
    exec(&pool, "CREATE PUBLICATION flusso FOR TABLE things").await;

    let (_audit_gate, audit_open) = watch::channel(true);
    let (seeds, seeds_open) = watch::channel(false);
    let backends = Arc::new(RealBackends {
        pg_url: pg_url.clone(),
        pg_port,
        os_url: os_url.clone(),
        audit: Arc::new(GatedSink {
            open: audit_open.clone(),
            others_open: audit_open,
        }),
        seeds: Some(seeds_open),
    });

    // ── a first backfill: nothing is vouched for until the seed ──────────────
    let running = Running::start(&backends).await;
    let client = Client {
        pool: pool.clone(),
        fence: running.fence.clone(),
        http: reqwest::Client::new(),
        os_url: os_url.clone(),
    };
    let position = client.rename(1, "first").await;
    let outcome = held(&running.fence, &position).await;
    assert!(
        unserved_as(&outcome, Unserved::Backfilling),
        "while the first seed waits: {outcome:?}"
    );
    assert_eq!(client.get(1).await, None, "nothing is readable yet");
    seeds.send(true).unwrap();
    client
        .fence(&position, FenceLevel::Searchable, PRIMARY)
        .await;
    assert_eq!(client.get(1).await.as_deref(), Some("first"));
    await_live(&running.status).await;

    // ── a reindex: the old generation vouches for what it holds, no more ─────
    let before = client.rename(2, "before").await;
    client.fence(&before, FenceLevel::Searchable, PRIMARY).await;
    seeds.send(false).unwrap();
    running
        .control
        .reindex(things(), Some(&SinkName::try_new("primary").unwrap()))
        .unwrap();
    eventually("the reindex to start", async || {
        let probe = client.rename(9, "probe").await;
        unserved_as(&held(&running.fence, &probe).await, Unserved::Rebuilding)
    })
    .await;
    let after = client.rename(2, "after").await;
    let outcome = held(&running.fence, &after).await;
    assert!(
        unserved_as(&outcome, Unserved::Rebuilding),
        "a change after the reindex began: {outcome:?}"
    );
    assert_eq!(
        client.get(2).await.as_deref(),
        Some("before"),
        "reads still see the old generation"
    );
    let outcome = held(&running.fence, &before).await;
    assert!(
        matches!(outcome, FenceOutcome::Reached { .. }),
        "a change the old generation holds is vouched for throughout: {outcome:?}"
    );
    seeds.send(true).unwrap();
    client.fence(&after, FenceLevel::Searchable, PRIMARY).await;
    assert_eq!(client.get(2).await.as_deref(), Some("after"));
    assert_eq!(client.search("after").await, vec![2]);

    // ── a dropped slot: a commit made while flusso was away (the issue) ──────
    running.stop().await;
    exec(&pool, "SELECT pg_drop_replication_slot('flusso')").await;
    let gap = client.rename(3, "gap").await;
    seeds.send(false).unwrap();
    let running = Running::start(&backends).await;
    let client = Client {
        fence: running.fence.clone(),
        ..client
    };
    let outcome = held(&running.fence, &gap).await;
    assert!(
        unserved_as(&outcome, Unserved::Rebuilding),
        "a commit made while flusso was away: {outcome:?}"
    );
    assert_eq!(
        client.get(3).await.as_deref(),
        Some("seed"),
        "reads still see the generation that missed it"
    );
    let since = client.rename(4, "since").await;
    let outcome = held(&running.fence, &since).await;
    assert!(
        unserved_as(&outcome, Unserved::Rebuilding),
        "a commit made after the restart: {outcome:?}"
    );
    seeds.send(true).unwrap();
    client.fence(&since, FenceLevel::Searchable, PRIMARY).await;
    assert_eq!(client.get(3).await.as_deref(), Some("gap"));
    assert_eq!(client.get(4).await.as_deref(), Some("since"));

    running.stop().await;
}

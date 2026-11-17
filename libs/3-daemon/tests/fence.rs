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
    SinkParts, SourceParts, Status,
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
    exec(&pool, "CREATE TABLE noise (id serial PRIMARY KEY, v text)").await;
    exec(
        &pool,
        "INSERT INTO things SELECT n, 'seed' FROM generate_series(1, 40) n",
    )
    .await;
    exec(&pool, "CREATE PUBLICATION flusso FOR TABLE things").await;

    let (gate, gated) = watch::channel(true);
    let backends = Arc::new(RealBackends {
        pg_url: pg_url.clone(),
        pg_port,
        os_url: os_url.clone(),
        audit: Arc::new(GatedSink { open: gated }),
    });
    let running = Daemon::new(config(), backends)
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
            audit: Arc::new(GatedSink { open: gated }),
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
        })
    }

    fn stream(&self, _: &Config, sinks: &[SinkName]) -> anyhow::Result<Arc<dyn Stream>> {
        Ok(Arc::new(ChannelStream::new(1024, sinks.iter().cloned())))
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
        Ok(vec![
            SinkParts {
                name: primary,
                sink: Arc::new(opensearch),
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

/// A sink whose flush waits while its gate is closed: a destination that
/// stopped answering.
#[derive(Debug)]
struct GatedSink {
    open: watch::Receiver<bool>,
}

#[async_trait]
impl Sink for GatedSink {
    async fn write(&self, _envelopes: &[Envelope], _caught_up: bool) -> sink::Result<WriteReport> {
        let mut open = self.open.clone();
        let _ = open.wait_for(|open| *open).await;
        Ok(WriteReport::clean())
    }
}

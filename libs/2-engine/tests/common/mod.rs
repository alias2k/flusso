//! Helpers shared by the Postgres-backed e2e binaries (`wal`, `wal_release`,
//! `visibility`, …): a logical-replication Postgres container, the small schema
//! builders the index specs are written with, and commits held before they
//! are visible.
//!
//! Each binary compiles this module on its own and uses a subset of it, hence
//! the `dead_code` allowance.

#![allow(dead_code)]

use std::collections::BTreeMap;

use kernel::{
    Column, ColumnName, DatabaseSchema, Field, FieldName, FieldSource, FlussoType, IndexName,
    IndexSchema, TableName,
};
use source::SourceSpec;
use testcontainers_modules::testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

/// The builds an e2e ingest engine runs at once: `FLUSSO_E2E_BUILDS`, else
/// 4, the daemon's `DEFAULT_BUILDS`: what a deployment runs.
pub(crate) fn e2e_builds() -> usize {
    std::env::var("FLUSSO_E2E_BUILDS")
        .ok()
        .and_then(|builds| builds.parse().ok())
        .unwrap_or(4)
}

/// A memory budget for an e2e ingest engine: the daemon's default 512 MiB,
/// so the e2e count what they build. Dropped with bytes still counted, it
/// fails the test (in debug builds): the standing leak guard.
pub(crate) fn e2e_budget() -> std::sync::Arc<engine::Budget> {
    std::sync::Arc::new(engine::Budget::new(512 * 1024 * 1024))
}

/// A PG 16 with logical replication, and its connection URL. PG 14+ is
/// required: the replication client requests the pgoutput `messages` option.
pub(crate) async fn start_postgres() -> (test_postgres::Container, String) {
    let container = test_postgres::image()
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
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    (container, url)
}

/// A Postgres with logical replication and `extra` server settings (`-c`
/// pairs), its port and its connection URL.
pub(crate) async fn start_postgres_with(extra: &[&str]) -> (test_postgres::Container, u16, String) {
    let mut command = vec![
        "postgres",
        "-c",
        "wal_level=logical",
        "-c",
        "max_wal_senders=10",
        "-c",
        "max_replication_slots=10",
    ];
    command.extend_from_slice(extra);
    let container = test_postgres::image()
        .with_cmd(command)
        .start()
        .await
        .unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    (container, port, url)
}

/// The replication settings of the `flusso` slot and publication on `port`.
pub(crate) fn replication(port: u16) -> source_postgres::ReplicationConfig {
    source_postgres::ReplicationConfig::new(
        "127.0.0.1",
        "postgres",
        "postgres",
        "postgres",
        "flusso",
        "flusso",
    )
    .with_port(port)
}

/// `users` indexed as `{ id, email }`.
pub(crate) fn users_spec() -> SourceSpec {
    let schema = IndexSchema {
        version: 1,
        table: table("users"),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![column_field("id", "id"), column_field("email", "email")],
    };
    SourceSpec::new(BTreeMap::from([(index_name("users"), schema)]))
}

pub(crate) fn column_field(name: &str, col: &str) -> Field {
    Field {
        field: field(name),
        options: Default::default(),
        emit: true,
        source: FieldSource::Column(Column {
            column: column(col),
            ty: FlussoType::Keyword,
            nullable: true,
            transforms: Vec::new(),
            default: None,
            enum_order: Vec::new(),
            subfields: true,
            indexed: true,
            array: false,
        }),
    }
}

pub(crate) fn field(name: &str) -> FieldName {
    FieldName::try_new(name).unwrap()
}

pub(crate) fn column(name: &str) -> ColumnName {
    ColumnName::try_new(name).unwrap()
}

pub(crate) fn table(name: &str) -> TableName {
    TableName::try_new(name).unwrap()
}

pub(crate) fn index_name(name: &str) -> IndexName {
    IndexName::try_new(name).unwrap()
}

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
/// It changes server settings with `ALTER SYSTEM`. Each test starts its own
/// container, so they never reach another test.
pub(crate) struct SyncRep {
    pool: sqlx::PgPool,
}

impl SyncRep {
    /// Name the standby that never answers.
    pub(crate) async fn configure(pool: &sqlx::PgPool) -> Self {
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
    pub(crate) async fn hold(&self, sql: &str) -> Held {
        self.hold_statement(format!("BEGIN; {sql}; COMMIT;")).await
    }

    /// Run `statement` as written (a `COMMIT PREPARED`, say) and return once it
    /// waits on the standby.
    pub(crate) async fn hold_statement(&self, statement: String) -> Held {
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
pub(crate) struct Held {
    pool: sqlx::PgPool,
    pid: i32,
    commit: tokio::task::JoinHandle<()>,
}

impl Held {
    /// Cancel this commit's standby wait alone: its transaction leaves the proc
    /// array and becomes visible, and its session returns.
    pub(crate) async fn release(self) {
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
pub(crate) async fn sent_to(pool: &sqlx::PgPool, application_name: &str) {
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
pub(crate) async fn standbys_in_force(pool: &sqlx::PgPool) {
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
pub(crate) async fn configure(pool: &sqlx::PgPool, statements: &[&str]) {
    for statement in statements.iter().chain(&["SELECT pg_reload_conf()"]) {
        sqlx::query(sqlx::AssertSqlSafe(*statement))
            .execute(pool)
            .await
            .unwrap();
    }
}

/// Poll `check` until it holds, failing after ten seconds with `what`.
pub(crate) async fn eventually(what: &str, mut check: impl AsyncFnMut() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !check().await {
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

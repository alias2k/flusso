//! Helpers shared by the Postgres-backed e2e binaries (`wal`, `wal_release`):
//! a logical-replication Postgres container and the small schema builders the
//! index specs are written with.
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

/// One commit held between its WAL flush and its visibility (issue #343).
///
/// Postgres flushes a commit record, then waits for synchronous standbys, and
/// only then takes the transaction out of the proc array, which is when new
/// snapshots see it. A logical walsender decodes up to the flushed WAL, so the
/// change reaches a replication client while every other session still reads
/// the state before it. Naming a standby that does not exist stretches that
/// window until [`release`](Self::release) cancels the wait.
///
/// It changes server settings with `ALTER SYSTEM`. Each test starts its own
/// container, so they never reach another test; `release` resets them anyway.
pub(crate) struct HeldCommit {
    pool: sqlx::PgPool,
    pid: i32,
    commit: tokio::task::JoinHandle<()>,
}

impl HeldCommit {
    /// Run `sql` as one transaction and return once its commit is flushed and
    /// waiting on the standby: in the WAL, not visible. Every other session
    /// keeps committing locally.
    pub(crate) async fn hold(pool: &sqlx::PgPool, sql: &str) -> Self {
        configure(
            pool,
            &[
                "ALTER SYSTEM SET synchronous_commit = 'local'",
                "ALTER SYSTEM SET synchronous_standby_names = 'nobody'",
            ],
        )
        .await;
        let mut connection = pool.acquire().await.unwrap().detach();
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
        let statement = format!("BEGIN; {sql}; COMMIT;");
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
            .fetch_one(pool)
            .await
            .unwrap()
        })
        .await;
        Self {
            pool: pool.clone(),
            pid,
            commit,
        }
    }

    /// Wait until the replication client named `application_name` was sent
    /// every WAL byte written so far, the held commit included.
    pub(crate) async fn sent_to(&self, application_name: &str) {
        let flushed: String = sqlx::query_scalar("SELECT pg_current_wal_lsn()::text")
            .fetch_one(&self.pool)
            .await
            .unwrap();
        eventually(
            "the held commit sent to the replication client",
            async || {
                sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS (SELECT 1 FROM pg_stat_replication \
                 WHERE application_name = $1 AND sent_lsn >= $2::pg_lsn)",
                )
                .bind(application_name)
                .bind(&flushed)
                .fetch_one(&self.pool)
                .await
                .unwrap()
            },
        )
        .await;
    }

    /// Cancel the standby wait: the transaction leaves the proc array and
    /// becomes visible, and its session returns. Resets the settings.
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
        configure(
            &self.pool,
            &[
                "ALTER SYSTEM RESET synchronous_standby_names",
                "ALTER SYSTEM RESET synchronous_commit",
            ],
        )
        .await;
    }
}

/// `ALTER SYSTEM` each statement (one per call: it refuses a transaction
/// block), then reload the configuration.
async fn configure(pool: &sqlx::PgPool, statements: &[&'static str]) {
    for statement in statements.iter().chain(&["SELECT pg_reload_conf()"]) {
        sqlx::query(*statement).execute(pool).await.unwrap();
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

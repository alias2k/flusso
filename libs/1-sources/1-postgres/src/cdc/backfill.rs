//! The snapshot capability: reads the *current* rows of a set of tables, the
//! data an initial backfill seeds an index with.
//!
//! `pgwire-replication` is a pure replication consumer — it neither creates the
//! slot nor exports its snapshot — so there is no slot-tied snapshot to read
//! from. Instead this reads existing rows over an ordinary SQL connection inside
//! a `REPEATABLE READ` transaction (a single, self-consistent view) and emits an
//! [`ChangeEvent::Upsert`] per row. The stream simply ends when every table is
//! drained; the engine knows it is seeding from *which stream* it is draining,
//! so there is no in-band boundary marker.
//!
//! That this is not fused to the slot's live position is fine: events are *thin*
//! (table + key), the engine re-reads each row at assembly time, and delivery is
//! at-least-once and idempotent. A row touched between the snapshot and where
//! live capture resumes is simply re-applied as a live change — same row, same
//! document, no harm. Snapshot changes therefore carry a no-op ack: a backfill
//! that crashes part-way is just re-run, it does not advance any cursor.
//!
//! ## Scope
//!
//! The engine passes exactly the tables to read — the **root table** of each
//! index it is seeding (a document is identified by its root row, so the nested
//! joins and aggregates are pulled in by `build`, not snapshotted here). Tables
//! without a single usable primary key are skipped with a warning rather than
//! aborting; logical replication can't address keyless rows anyway.
//!
//! ## Cost
//!
//! The `REPEATABLE READ` transaction is held open for the whole snapshot, which
//! holds back the source's vacuum horizon until it finishes — the usual,
//! expected cost of a consistent read. Rows stream through a server-side cursor,
//! so memory stays bounded regardless of table size.

use std::collections::VecDeque;
use std::sync::Arc;

use futures::stream::{self, BoxStream};
use schema_core::{ColumnName, TableName};
use sources_core::cdc::{Ack, AckSink, Change, ChangeEvent};
use sources_core::{Result, RowKey, SnapshotTable, SourceError};
use sqlx::pool::PoolConnection;
use sqlx::postgres::{PgPoolOptions, PgRow};
use sqlx::{PgPool, Postgres, Row};

use crate::document::value;

/// Name of the single server-side cursor reused across tables. The snapshot owns
/// its own connection for its whole lifetime, so a fixed name never collides.
const CURSOR: &str = "flusso_backfill_cursor";

/// How many keys to pull from the cursor per round-trip.
const FETCH_SQL: &str = "FETCH FORWARD 1024 FROM flusso_backfill_cursor";

/// Connect a query connection and stream the current rows of `tables` as
/// `Upsert` changes. Returns an empty stream when nothing is in scope.
pub(crate) async fn snapshot(
    connection_url: &str,
    tables: &[SnapshotTable],
) -> Result<BoxStream<'static, Result<Change>>> {
    // A tiny pool: the snapshot checks out exactly one connection and holds it
    // (its snapshot transaction) for the duration.
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(connection_url)
        .await
        .map_err(|e| SourceError::Connection(e.to_string()))?;
    let tables = resolve_tables(&pool, tables).await?;
    Ok(build_stream(pool, tables))
}

/// One table to snapshot, with its identifiers pre-quoted for the cursor's
/// `SELECT` and the validated names used to build [`Change`]s.
struct BackfillTable {
    /// `"schema"."table"`, quoted and ready to interpolate.
    qualified: String,
    /// `"pk"`, quoted and ready to interpolate.
    pk_quoted: String,
    /// The table name as it appears in change events.
    table: TableName,
    /// The primary-key column name carried in each [`RowKey`].
    pk: ColumnName,
}

/// Pair each requested table with its single-column primary key, skipping (with
/// a warning) any table that lacks one — neither it nor logical replication can
/// address keyless rows.
async fn resolve_tables(pool: &PgPool, tables: &[SnapshotTable]) -> Result<Vec<BackfillTable>> {
    let mut out = Vec::with_capacity(tables.len());
    for table in tables {
        let schema = table.db_schema.as_ref();
        let name = table.table.as_ref();
        let Some(pk) = primary_key(pool, schema, name).await? else {
            continue; // reason already logged
        };
        out.push(BackfillTable {
            qualified: format!("{}.{}", quote_ident(schema), quote_ident(name)),
            pk_quoted: quote_ident(pk.as_ref()),
            table: table.table.clone(),
            pk,
        });
    }
    Ok(out)
}

/// The single-column primary key of a table from the catalog, or `None` (with a
/// warning) if it has no primary key or a composite one.
async fn primary_key(pool: &PgPool, schema: &str, table: &str) -> Result<Option<ColumnName>> {
    let sql = "SELECT a.attname AS name \
               FROM pg_index i \
               JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey) \
               WHERE i.indrelid = $1::regclass AND i.indisprimary";
    let rows = sqlx::query(sql)
        .bind(format!("{}.{}", quote_ident(schema), quote_ident(table)))
        .fetch_all(pool)
        .await
        .map_err(query_err)?;

    let mut columns = Vec::with_capacity(rows.len());
    for row in &rows {
        let name: String = row.try_get("name").map_err(query_err)?;
        match ColumnName::try_new(&name) {
            Ok(column) => columns.push(column),
            Err(e) => {
                tracing::warn!(%table, column = %name, error = %e, "skipping backfill: invalid primary key column");
                return Ok(None);
            }
        }
    }
    match columns.as_slice() {
        [single] => Ok(Some(single.clone())),
        [] => {
            tracing::warn!(%table, "skipping backfill: table has no primary key");
            Ok(None)
        }
        _ => {
            tracing::warn!(%table, "skipping backfill: composite primary key is unsupported");
            Ok(None)
        }
    }
}

/// Build the snapshot stream: an `Upsert` per existing row across `tables`, then
/// end. With no tables it ends immediately.
fn build_stream(pool: PgPool, tables: Vec<BackfillTable>) -> BoxStream<'static, Result<Change>> {
    let phase = if tables.is_empty() {
        Phase::Done
    } else {
        Phase::Pending {
            tables: tables.into(),
        }
    };
    let state = Backfill { pool, phase };
    Box::pin(stream::unfold(state, |mut state| async move {
        state.step().await.map(|item| (item, state))
    }))
}

/// Carries the snapshot across `unfold` polls.
struct Backfill {
    /// Kept alive for the whole snapshot so the checked-out connection stays
    /// valid; the connection itself lives in [`Phase::Reading`].
    pool: PgPool,
    phase: Phase,
}

enum Phase {
    /// Tables resolved, snapshot transaction not yet opened.
    Pending { tables: VecDeque<BackfillTable> },
    /// A cursor is open on `current`; drain `buf`, then fetch the next batch.
    Reading {
        conn: PoolConnection<Postgres>,
        current: BackfillTable,
        remaining: VecDeque<BackfillTable>,
        buf: VecDeque<RowKey>,
    },
    /// Stream exhausted.
    Done,
}

impl Backfill {
    /// Produce the next item, or `None` once the snapshot is exhausted. Drives
    /// the phase machine, doing one unit of DB work per loop turn between yields.
    async fn step(&mut self) -> Option<Result<Change>> {
        loop {
            // Take the phase by value so async work and the reassignment below
            // don't fight over a borrow of `self.phase`.
            match std::mem::replace(&mut self.phase, Phase::Done) {
                Phase::Pending { mut tables } => {
                    let mut conn = match self.pool.acquire().await {
                        Ok(conn) => conn,
                        Err(e) => return Some(Err(SourceError::Connection(e.to_string()))),
                    };
                    if let Err(e) = begin_snapshot(&mut conn).await {
                        return Some(Err(e));
                    }
                    // `tables` is non-empty here (the empty case starts `Done`);
                    // `?` is a total fallback that just ends the stream otherwise.
                    let current = tables.pop_front()?;
                    if let Err(e) = declare_cursor(&mut conn, &current).await {
                        return Some(Err(e));
                    }
                    self.phase = Phase::Reading {
                        conn,
                        current,
                        remaining: tables,
                        buf: VecDeque::new(),
                    };
                }
                Phase::Reading {
                    mut conn,
                    current,
                    mut remaining,
                    mut buf,
                } => {
                    if let Some(key) = buf.pop_front() {
                        let change = upsert_change(current.table.clone(), key);
                        self.phase = Phase::Reading {
                            conn,
                            current,
                            remaining,
                            buf,
                        };
                        return Some(Ok(change));
                    }

                    let rows = match fetch_batch(&mut conn).await {
                        Ok(rows) => rows,
                        Err(e) => return Some(Err(e)),
                    };

                    if rows.is_empty() {
                        if let Err(e) = close_cursor(&mut conn).await {
                            return Some(Err(e));
                        }
                        match remaining.pop_front() {
                            None => {
                                if let Err(e) = commit(&mut conn).await {
                                    return Some(Err(e));
                                }
                                return None;
                            }
                            Some(next) => {
                                if let Err(e) = declare_cursor(&mut conn, &next).await {
                                    return Some(Err(e));
                                }
                                self.phase = Phase::Reading {
                                    conn,
                                    current: next,
                                    remaining,
                                    buf,
                                };
                            }
                        }
                    } else {
                        for row in &rows {
                            let value = value::first_column_to_generic(row);
                            buf.push_back(RowKey(vec![(current.pk.clone(), value)]));
                        }
                        self.phase = Phase::Reading {
                            conn,
                            current,
                            remaining,
                            buf,
                        };
                    }
                }
                Phase::Done => return None,
            }
        }
    }
}

/// A snapshot row as an [`Upsert`](ChangeEvent::Upsert) change. Its ack is a
/// no-op: the snapshot is not resumable, so confirming a row moves no cursor.
fn upsert_change(table: TableName, key: RowKey) -> Change {
    Change {
        event: ChangeEvent::Upsert { table, key },
        ack: Ack::new(0, Arc::new(NoopAck)),
    }
}

/// An [`AckSink`] that discards confirmations — for snapshot changes, which have
/// no durable cursor to advance.
#[derive(Debug)]
struct NoopAck;

impl AckSink for NoopAck {
    fn confirm(&self, _seq: u64) {}
}

/// Open the read-only, single-snapshot transaction the cursors read within.
async fn begin_snapshot(conn: &mut PoolConnection<Postgres>) -> Result<()> {
    sqlx::query("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut **conn)
        .await
        .map_err(query_err)?;
    Ok(())
}

/// Declare the cursor over a table's primary-key column.
///
/// The SQL is dynamic only in its identifiers, which come from the catalog and
/// are quoted (so injection-safe); it binds no data values — hence
/// [`sqlx::AssertSqlSafe`].
async fn declare_cursor(conn: &mut PoolConnection<Postgres>, table: &BackfillTable) -> Result<()> {
    let sql = format!(
        "DECLARE {CURSOR} NO SCROLL CURSOR FOR SELECT {} FROM {}",
        table.pk_quoted, table.qualified,
    );
    sqlx::query(sqlx::AssertSqlSafe(sql))
        .execute(&mut **conn)
        .await
        .map_err(query_err)?;
    Ok(())
}

async fn fetch_batch(conn: &mut PoolConnection<Postgres>) -> Result<Vec<PgRow>> {
    sqlx::query(FETCH_SQL)
        .fetch_all(&mut **conn)
        .await
        .map_err(query_err)
}

async fn close_cursor(conn: &mut PoolConnection<Postgres>) -> Result<()> {
    sqlx::query("CLOSE flusso_backfill_cursor")
        .execute(&mut **conn)
        .await
        .map_err(query_err)?;
    Ok(())
}

async fn commit(conn: &mut PoolConnection<Postgres>) -> Result<()> {
    sqlx::query("COMMIT")
        .execute(&mut **conn)
        .await
        .map_err(query_err)?;
    Ok(())
}

/// Double-quote an SQL identifier, escaping embedded quotes.
fn quote_ident(ident: &str) -> String {
    format!("\"{}\"", ident.replace('"', "\"\""))
}

fn query_err(error: sqlx::Error) -> SourceError {
    SourceError::Query(error.to_string())
}

//! The initial backfill: a one-time seed of the rows that already exist in the
//! source before live capture takes over.
//!
//! `pgwire-replication` is a pure replication consumer — it neither creates the
//! slot nor exports its snapshot — so there is no slot-tied snapshot to read
//! from. Instead this reads existing rows over an ordinary SQL connection inside
//! a `REPEATABLE READ` transaction (a single, self-consistent view) and emits a
//! [`ChangeEvent::Snapshot`] per row, then a [`ChangeEvent::SnapshotComplete`].
//!
//! That this is not perfectly fused to the slot's start position is fine: events
//! are *thin* (table + key), the engine re-reads each row at assembly time, and
//! delivery is at-least-once and idempotent. A row touched between the slot's
//! confirmed position and the snapshot is simply re-applied as a live change —
//! same row, same document, no harm. Backfill rows therefore register against
//! the shared ack watermark at `start_lsn`, so confirming them never advances
//! the slot past where live replay must begin.
//!
//! ## What gets backfilled
//!
//! Every table in the configured **publication** — exactly the set of tables the
//! live stream reports changes for — keeping this layer free of any index
//! knowledge (mapping rows to documents is the engine's job). A `FOR ALL TABLES`
//! publication therefore backfills everything it covers. Tables without a single
//! usable primary key are skipped with a warning rather than aborting the run;
//! logical replication can't address keyless rows anyway.
//!
//! ## Cost
//!
//! The `REPEATABLE READ` transaction is held open for the whole backfill, which
//! holds back the source's vacuum horizon until it finishes — the usual,
//! expected cost of a consistent initial load. Rows stream through a server-side
//! cursor, so memory stays bounded regardless of table size.

use std::collections::VecDeque;
use std::sync::Arc;

use futures::stream::{self, BoxStream};
use schema_core::{ColumnName, TableName};
use sources_core::cdc::{Ack, AckSink, Change, ChangeEvent};
use sources_core::{Result, RowKey, SourceError};
use sqlx::pool::PoolConnection;
use sqlx::postgres::{PgPoolOptions, PgRow};
use sqlx::{PgPool, Postgres, Row};

use super::ack::AckShared;
use crate::document::value;

/// Name of the single server-side cursor reused across tables. The backfill owns
/// its own connection for its whole lifetime, so a fixed name never collides.
const CURSOR: &str = "storno_backfill_cursor";

/// How many keys to pull from the cursor per round-trip.
const FETCH_SQL: &str = "FETCH FORWARD 1024 FROM storno_backfill_cursor";

/// One publication table to backfill, with its identifiers pre-quoted for the
/// cursor's `SELECT` and the validated names used to build [`Change`]s.
pub(crate) struct BackfillTable {
    /// `"schema"."table"`, quoted and ready to interpolate.
    qualified: String,
    /// `"pk"`, quoted and ready to interpolate.
    pk_quoted: String,
    /// The table name as it appears in change events.
    table: TableName,
    /// The primary-key column name carried in each [`RowKey`].
    pk: ColumnName,
}

/// Connect a query pool and resolve the publication's backfillable tables.
///
/// Done eagerly (before streaming) so a misconfigured publication or connection
/// surfaces at startup rather than mid-stream.
pub(crate) async fn prepare(
    connection_url: &str,
    publication: &str,
) -> Result<(PgPool, Vec<BackfillTable>)> {
    // A tiny pool: the backfill checks out exactly one connection and holds it
    // (its snapshot transaction) for the duration; `plan` returns its borrow.
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(connection_url)
        .await
        .map_err(|e| SourceError::Connection(e.to_string()))?;
    let tables = plan(&pool, publication).await?;
    Ok((pool, tables))
}

/// The publication's tables, each paired with its single-column primary key.
async fn plan(pool: &PgPool, publication: &str) -> Result<Vec<BackfillTable>> {
    let rows = sqlx::query(
        "SELECT schemaname, tablename FROM pg_publication_tables WHERE pubname = $1 \
         ORDER BY schemaname, tablename",
    )
    .bind(publication)
    .fetch_all(pool)
    .await
    .map_err(query_err)?;

    if rows.is_empty() {
        tracing::warn!(
            %publication,
            "backfill found no tables for the publication; seeding nothing"
        );
    }

    let mut tables = Vec::with_capacity(rows.len());
    for row in &rows {
        let schema: String = row.try_get("schemaname").map_err(query_err)?;
        let name: String = row.try_get("tablename").map_err(query_err)?;
        let Ok(table) = TableName::try_new(&name) else {
            tracing::warn!(table = %name, "skipping backfill: not a valid table identifier");
            continue;
        };
        let Some(pk) = primary_key(pool, &schema, &name).await? else {
            continue; // reason already logged
        };
        tables.push(BackfillTable {
            qualified: format!("{}.{}", quote_ident(&schema), quote_ident(&name)),
            pk_quoted: quote_ident(pk.as_ref()),
            table,
            pk,
        });
    }
    Ok(tables)
}

/// The single-column primary key of a table from the catalog, or `None` (with a
/// warning) if it has no primary key or a composite one — neither of which the
/// rest of the system can address.
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

/// Build the backfill [`Change`] stream: a `Snapshot` per existing row across
/// `tables`, followed by `SnapshotComplete`. With no tables it is just the
/// `SnapshotComplete` boundary — the correct shape when there is nothing to seed
/// (or backfill is disabled), so the live phase still gets its clean start.
pub(crate) fn stream(
    pool: Option<PgPool>,
    tables: Vec<BackfillTable>,
    ack: Arc<AckShared>,
    sink: Arc<dyn AckSink>,
    start_lsn: u64,
) -> BoxStream<'static, Result<Change>> {
    let phase = if tables.is_empty() {
        Phase::Completing
    } else {
        Phase::Pending {
            tables: tables.into(),
        }
    };
    let state = Backfill {
        ack,
        sink,
        start_lsn,
        pool,
        phase,
    };
    Box::pin(stream::unfold(state, |mut state| async move {
        state.step().await.map(|item| (item, state))
    }))
}

/// Carries the backfill across `unfold` polls.
struct Backfill {
    ack: Arc<AckShared>,
    sink: Arc<dyn AckSink>,
    start_lsn: u64,
    /// Kept alive for the whole backfill so the checked-out connection stays
    /// valid; the connection itself lives in [`Phase::Reading`].
    pool: Option<PgPool>,
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
    /// All rows emitted (or none to emit): emit the `SnapshotComplete` boundary.
    Completing,
    /// Stream exhausted.
    Done,
}

impl Backfill {
    /// Produce the next item, or `None` once the backfill is exhausted. Drives
    /// the phase machine, doing one unit of DB work per loop turn between yields.
    async fn step(&mut self) -> Option<Result<Change>> {
        loop {
            // Take the phase by value so async work and the reassignment below
            // don't fight over a borrow of `self.phase`.
            match std::mem::replace(&mut self.phase, Phase::Done) {
                Phase::Pending { mut tables } => {
                    let Some(pool) = &self.pool else {
                        return Some(Err(SourceError::Setup(
                            "backfill: connection pool missing".into(),
                        )));
                    };
                    let mut conn = match pool.acquire().await {
                        Ok(conn) => conn,
                        Err(e) => return Some(Err(SourceError::Connection(e.to_string()))),
                    };
                    if let Err(e) = begin_snapshot(&mut conn).await {
                        return Some(Err(e));
                    }
                    // `tables` is non-empty here (the empty case starts in
                    // `Completing`), but stay total rather than unwrap.
                    let Some(current) = tables.pop_front() else {
                        self.phase = Phase::Completing;
                        continue;
                    };
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
                        let change = self.make_change(ChangeEvent::Snapshot {
                            table: current.table.clone(),
                            key,
                        });
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
                                self.phase = Phase::Completing;
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
                Phase::Completing => {
                    self.phase = Phase::Done;
                    return Some(Ok(self.make_change(ChangeEvent::SnapshotComplete)));
                }
                Phase::Done => return None,
            }
        }
    }

    /// Wrap an event in a [`Change`] whose ack registers at `start_lsn`, so
    /// confirming backfill rows holds the slot at its resume point.
    fn make_change(&self, event: ChangeEvent) -> Change {
        let seq = self.ack.register(self.start_lsn);
        Change {
            event,
            ack: Ack::new(seq, Arc::clone(&self.sink)),
        }
    }
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
    sqlx::query("CLOSE storno_backfill_cursor")
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

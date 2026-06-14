use std::sync::Arc;

use async_trait::async_trait;
use futures::stream::BoxStream;
use pgwire_replication::{ReplicationClient, ReplicationConfig};
use sources_core::cdc::{AckSink, Change, ChangeCapture};
use sources_core::{Result, SnapshotTable, SourceError};
use sqlx::{PgPool, Row};
use tokio::sync::OnceCell;

use super::ack::{AckShared, WalAckSink};
use super::{backfill, stream};

/// Postgres change capture over logical replication (pgoutput).
///
/// Exposes the two [`ChangeCapture`] capabilities the engine orchestrates:
///
/// - [`live`](ChangeCapture::live) connects to a replication slot and streams
///   committed row changes as thin [`Change`]s. Resume is the slot's: its
///   `confirmed_flush_lsn` is the durable cursor on the server, advanced as the
///   engine confirms changes (see [`Ack`](sources_core::cdc::Ack)).
/// - [`snapshot`](ChangeCapture::snapshot) reads current rows over an ordinary
///   SQL connection for an initial backfill (see the crate-private `backfill`). The engine calls
///   it only for tables backing an index the sink reports as unseeded.
///
/// # Prerequisites
///
/// The server must have `wal_level = logical` and the configured **publication**
/// must already exist and cover every table any index reads from. The replication
/// **slot** is created automatically on first connect if it does not exist yet.
#[derive(Debug, Clone)]
pub struct WalChangeCapture {
    config: ReplicationConfig,
    /// Ordinary SQL connection URL, used by [`snapshot`](Self::snapshot) and
    /// for the automatic slot creation check.
    connection_url: String,
    /// A small, lazily-opened SQL pool shared by the slot check and the
    /// out-of-band [`lag`](Self::lag) polling, so periodic status probes reuse
    /// connections instead of opening and tearing one down each time. Shared
    /// across clones (an `Arc`), opened on first use. The bulk snapshot read
    /// stays on its own connection (see [`snapshot`](Self::snapshot)).
    admin_pool: Arc<OnceCell<PgPool>>,
}

impl WalChangeCapture {
    /// Create a capture from a `pgwire-replication` configuration and the
    /// ordinary SQL connection URL the snapshot reads through (the same URL the
    /// document builder connects with).
    ///
    /// Leave `config.start_lsn` at [`Lsn::ZERO`](pgwire_replication::Lsn::ZERO)
    /// to resume from the slot's `confirmed_flush_lsn` — the usual choice.
    pub fn new(config: ReplicationConfig, connection_url: impl Into<String>) -> Self {
        Self {
            config,
            connection_url: connection_url.into(),
            admin_pool: Arc::new(OnceCell::new()),
        }
    }

    /// The shared admin pool, opened on first call and reused thereafter. Kept
    /// deliberately small — it serves only the slot check and lag probes, not
    /// the change or snapshot paths.
    async fn admin_pool(&self) -> Result<&PgPool> {
        self.admin_pool
            .get_or_try_init(|| async {
                sqlx::postgres::PgPoolOptions::new()
                    .max_connections(2)
                    .connect(&self.connection_url)
                    .await
                    .map_err(|e| SourceError::Connection(e.to_string()))
            })
            .await
    }

    /// Ensure the replication slot exists, creating it if it does not.
    ///
    /// Runs over the shared admin pool so the check can run before the
    /// replication connection is opened. If the slot already exists its plugin
    /// is validated; a slot with the wrong plugin name is an error (it was
    /// created for a different consumer and we should not clobber it).
    async fn ensure_slot(&self) -> Result<()> {
        let pool = self.admin_pool().await?;

        let row = sqlx::query("SELECT plugin FROM pg_replication_slots WHERE slot_name = $1")
            .bind(&self.config.slot)
            .fetch_optional(pool)
            .await
            .map_err(|e| SourceError::Query(e.to_string()))?;

        match row {
            Some(row) => {
                let plugin: String = row
                    .try_get("plugin")
                    .map_err(|e| SourceError::Query(e.to_string()))?;
                if plugin != "pgoutput" {
                    return Err(SourceError::Connection(format!(
                        "replication slot '{}' exists but uses plugin '{}', expected 'pgoutput'",
                        self.config.slot, plugin,
                    )));
                }
                tracing::debug!(slot = %self.config.slot, "replication slot already exists");
            }
            None => {
                sqlx::query("SELECT pg_create_logical_replication_slot($1, 'pgoutput')")
                    .bind(&self.config.slot)
                    .execute(pool)
                    .await
                    .map_err(|e| {
                        SourceError::Connection(format!(
                            "failed to create replication slot '{}': {e}",
                            self.config.slot,
                        ))
                    })?;
                tracing::info!(slot = %self.config.slot, "created replication slot");
            }
        }

        Ok(())
    }
}

#[async_trait]
impl ChangeCapture for WalChangeCapture {
    #[tracing::instrument(name = "wal.live", skip_all, err)]
    async fn live(&self) -> Result<BoxStream<'static, Result<Change>>> {
        self.ensure_slot().await?;

        let client = ReplicationClient::connect(self.config.clone())
            .await
            .map_err(|e| SourceError::Connection(e.to_string()))?;

        let ack = Arc::new(AckShared::new(self.config.start_lsn.as_u64()));
        let sink: Arc<dyn AckSink> = Arc::new(WalAckSink::new(Arc::clone(&ack)));
        tracing::info!(
            start_lsn = self.config.start_lsn.as_u64(),
            "opened replication stream"
        );
        Ok(stream::build(client, ack, sink))
    }

    #[tracing::instrument(name = "wal.snapshot", skip_all, fields(tables = tables.len()), err)]
    async fn snapshot(
        &self,
        tables: &[SnapshotTable],
    ) -> Result<BoxStream<'static, Result<Change>>> {
        tracing::info!(tables = tables.len(), "starting snapshot");
        backfill::snapshot(&self.connection_url, tables).await
    }

    /// Bytes between the slot's `confirmed_flush_lsn` and the server's current
    /// WAL LSN — how far behind the destination is. Returns `None` until the
    /// slot exists (it is created on the first [`live`](Self::live) connect).
    #[tracing::instrument(name = "wal.lag", skip_all, err)]
    async fn lag(&self) -> Result<Option<u64>> {
        let pool = self.admin_pool().await?;

        // `pg_wal_lsn_diff` yields a numeric byte distance; cast to bigint so it
        // decodes as an integer. A slot whose consumer is fully caught up reads
        // zero; a never-connected slot has no row, hence `Option`.
        let row = sqlx::query(
            "SELECT pg_wal_lsn_diff(pg_current_wal_lsn(), confirmed_flush_lsn)::bigint AS lag \
             FROM pg_replication_slots WHERE slot_name = $1",
        )
        .bind(&self.config.slot)
        .fetch_optional(pool)
        .await
        .map_err(|e| SourceError::Query(e.to_string()))?;

        let lag = match row {
            Some(row) => {
                let bytes: i64 = row
                    .try_get("lag")
                    .map_err(|e| SourceError::Query(e.to_string()))?;
                // A negative diff (slot momentarily ahead of the read LSN) clamps
                // to zero — there is no meaningful "negative lag".
                Some(bytes.max(0) as u64)
            }
            None => None,
        };
        Ok(lag)
    }
}

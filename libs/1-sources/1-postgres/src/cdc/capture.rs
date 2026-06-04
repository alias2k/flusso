use std::sync::Arc;

use async_trait::async_trait;
use futures::stream::BoxStream;
use pgwire_replication::{ReplicationClient, ReplicationConfig};
use sources_core::cdc::{AckSink, Change, ChangeCapture};
use sources_core::{Result, SnapshotTable, SourceError};
use sqlx::Row;

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
///   SQL connection for an initial backfill (see [`backfill`]). The engine calls
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
        }
    }

    /// Ensure the replication slot exists, creating it if it does not.
    ///
    /// Connects via the ordinary SQL URL so the check can run before the
    /// replication connection is opened. If the slot already exists its plugin
    /// is validated; a slot with the wrong plugin name is an error (it was
    /// created for a different consumer and we should not clobber it).
    async fn ensure_slot(&self) -> Result<()> {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(&self.connection_url)
            .await
            .map_err(|e| SourceError::Connection(e.to_string()))?;

        let row = sqlx::query("SELECT plugin FROM pg_replication_slots WHERE slot_name = $1")
            .bind(&self.config.slot)
            .fetch_optional(&pool)
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
                    .execute(&pool)
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

        pool.close().await;
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
}

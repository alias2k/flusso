use std::sync::Arc;

use async_trait::async_trait;
use futures::stream::BoxStream;
use pgwire_replication::{ReplicationClient, ReplicationConfig};
use sources_core::cdc::{AckSink, Change, ChangeCapture};
use sources_core::{Result, SnapshotTable, SourceError};

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
/// The server must have `wal_level = logical`, and the configured **slot** and
/// **publication** must already exist (this is a pure consumer — it creates
/// neither). The publication must include every table any index reads from
/// (roots, joined tables, and junction tables).
#[derive(Debug, Clone)]
pub struct WalChangeCapture {
    config: ReplicationConfig,
    /// Ordinary SQL connection URL, used by [`snapshot`](Self::snapshot).
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
}

#[async_trait]
impl ChangeCapture for WalChangeCapture {
    async fn live(&self) -> Result<BoxStream<'static, Result<Change>>> {
        let client = ReplicationClient::connect(self.config.clone())
            .await
            .map_err(|e| SourceError::Connection(e.to_string()))?;

        let ack = Arc::new(AckShared::new(self.config.start_lsn.as_u64()));
        let sink: Arc<dyn AckSink> = Arc::new(WalAckSink::new(Arc::clone(&ack)));
        Ok(stream::build(client, ack, sink))
    }

    async fn snapshot(
        &self,
        tables: &[SnapshotTable],
    ) -> Result<BoxStream<'static, Result<Change>>> {
        backfill::snapshot(&self.connection_url, tables).await
    }
}

use async_trait::async_trait;
use futures::stream::BoxStream;
use pgwire_replication::{ReplicationClient, ReplicationConfig};
use sources_core::{Change, ChangeCapture, Result, SourceError};

use crate::stream;

/// Postgres change capture over logical replication (pgoutput).
///
/// Connects to a replication slot and streams committed row changes as thin
/// [`Change`]s. Resume is handled by the slot itself: its `confirmed_flush_lsn`
/// is the durable cursor on the server, advanced as the engine confirms changes
/// (see [`Ack`](sources_core::Ack)).
///
/// # Prerequisites
///
/// The server must have `wal_level = logical`, and the configured **slot** and
/// **publication** must already exist (this is a pure consumer — it creates
/// neither). The publication must include every table any index reads from
/// (roots, joined tables, and junction tables).
///
/// # Not yet implemented
///
/// Initial backfill. A consistent snapshot of existing rows requires a normal
/// SQL query connection tied to the slot's exported snapshot, which is a
/// separate piece. Until then [`start`](Self::start) emits
/// [`ChangeEvent::SnapshotComplete`](sources_core::ChangeEvent::SnapshotComplete)
/// immediately and streams only live changes — correct when resuming an
/// existing slot, but it will not seed a brand-new index.
#[derive(Debug, Clone)]
pub struct WalChangeCapture {
    config: ReplicationConfig,
}

impl WalChangeCapture {
    /// Create a capture from a `pgwire-replication` configuration.
    ///
    /// Leave `config.start_lsn` at [`Lsn::ZERO`](pgwire_replication::Lsn::ZERO)
    /// to resume from the slot's `confirmed_flush_lsn` — the usual choice.
    pub fn new(config: ReplicationConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl ChangeCapture for WalChangeCapture {
    async fn start(&self) -> Result<BoxStream<'static, Result<Change>>> {
        tracing::warn!(
            "WAL initial backfill is not yet implemented; resuming live capture from the \
             slot's confirmed position. Existing rows are not (re)synced by this source."
        );

        let client = ReplicationClient::connect(self.config.clone())
            .await
            .map_err(|e| SourceError::Connection(e.to_string()))?;

        Ok(stream::build(client, self.config.start_lsn))
    }
}

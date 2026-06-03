use std::sync::Arc;

use async_trait::async_trait;
use futures::StreamExt;
use futures::stream::BoxStream;
use pgwire_replication::{ReplicationClient, ReplicationConfig};
use sources_core::cdc::{AckSink, Change, ChangeCapture};
use sources_core::{Result, SourceError};

use super::ack::{AckShared, WalAckSink};
use super::{backfill, stream};

/// Postgres change capture over logical replication (pgoutput).
///
/// Connects to a replication slot and streams committed row changes as thin
/// [`Change`]s. Resume is handled by the slot itself: its `confirmed_flush_lsn`
/// is the durable cursor on the server, advanced as the engine confirms changes
/// (see [`Ack`](sources_core::cdc::Ack)).
///
/// # Prerequisites
///
/// The server must have `wal_level = logical`, and the configured **slot** and
/// **publication** must already exist (this is a pure consumer — it creates
/// neither). The publication must include every table any index reads from
/// (roots, joined tables, and junction tables).
///
/// # Backfill
///
/// On [`start`](Self::start) the existing rows of the publication's tables are
/// seeded first (a `Snapshot` per row, then `SnapshotComplete`), over an
/// ordinary SQL connection — see [`backfill`]. Only then is the replication
/// client connected and live changes streamed. The backfill runs on every start
/// (the source keeps no durable "already seeded" state beyond the slot); it is
/// idempotent, but for an already-seeded index that just wants to resume, build
/// with [`with_backfill(false)`](Self::with_backfill) to skip it.
#[derive(Debug, Clone)]
pub struct WalChangeCapture {
    config: ReplicationConfig,
    /// Ordinary SQL connection URL, used for the initial backfill query.
    connection_url: String,
    /// Whether to run the initial backfill before live capture.
    backfill: bool,
}

impl WalChangeCapture {
    /// Create a capture from a `pgwire-replication` configuration and the
    /// ordinary SQL connection URL the backfill reads through (the same URL the
    /// document builder connects with).
    ///
    /// Leave `config.start_lsn` at [`Lsn::ZERO`](pgwire_replication::Lsn::ZERO)
    /// to resume from the slot's `confirmed_flush_lsn` — the usual choice.
    pub fn new(config: ReplicationConfig, connection_url: impl Into<String>) -> Self {
        Self {
            config,
            connection_url: connection_url.into(),
            backfill: true,
        }
    }

    /// Enable or disable the initial backfill (enabled by default). Disable it
    /// to resume an already-seeded index without re-reading every existing row.
    pub fn with_backfill(mut self, enabled: bool) -> Self {
        self.backfill = enabled;
        self
    }
}

#[async_trait]
impl ChangeCapture for WalChangeCapture {
    async fn start(&self) -> Result<BoxStream<'static, Result<Change>>> {
        let start_lsn = self.config.start_lsn.as_u64();
        let ack = Arc::new(AckShared::new(start_lsn));
        let sink: Arc<dyn AckSink> = Arc::new(WalAckSink::new(Arc::clone(&ack)));

        // Resolve what to backfill up front so a bad publication/connection fails
        // at startup, not mid-stream. Disabled or empty → just the boundary.
        let (pool, tables) = if self.backfill {
            let (pool, tables) =
                backfill::prepare(&self.connection_url, &self.config.publication).await?;
            tracing::info!(tables = tables.len(), "starting initial backfill");
            (Some(pool), tables)
        } else {
            tracing::info!("skipping initial backfill; resuming live capture only");
            (None, Vec::new())
        };

        let backfill = backfill::stream(
            pool,
            tables,
            Arc::clone(&ack),
            Arc::clone(&sink),
            start_lsn,
        );
        // Live capture is built lazily so the replication connection only opens
        // once the backfill has fully drained — an idle slot consumer left open
        // through a long backfill risks stalling.
        let live = live_stream(self.config.clone(), ack, sink);
        Ok(backfill.chain(live).boxed())
    }
}

/// Connect the replication client and stream live changes — lazily, evaluated
/// only when the chained stream first polls past the backfill.
fn live_stream(
    config: ReplicationConfig,
    ack: Arc<AckShared>,
    sink: Arc<dyn AckSink>,
) -> BoxStream<'static, Result<Change>> {
    let connect = async move {
        match ReplicationClient::connect(config).await {
            Ok(client) => stream::build(client, ack, sink),
            Err(e) => {
                let err: Result<Change> = Err(SourceError::Connection(e.to_string()));
                futures::stream::once(async move { err }).boxed()
            }
        }
    };
    futures::stream::once(connect).flatten().boxed()
}

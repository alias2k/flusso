use async_trait::async_trait;
use futures::stream::{self, BoxStream};

use crate::{Result, SnapshotTable};

use super::Change;

/// A pluggable change-capture mechanism — logical replication (WAL) today,
/// polling or trigger-based capture later.
///
/// The mechanism exposes two independent capabilities; the engine decides when
/// to use each:
///
/// - [`live`](Self::live) streams ongoing changes, resuming from the
///   mechanism's own durable position (a replication slot's
///   `confirmed_flush_lsn`, a poll cursor, …). No position is threaded through
///   this API — resume state is the mechanism's to own.
/// - [`snapshot`](Self::snapshot) reads the *current* rows of a set of tables as
///   a finite stream — the data an initial backfill needs. Whether a backfill
///   is *needed* is not the mechanism's call: the engine asks the **sink**
///   whether a target is already seeded and only then requests a snapshot. A
///   mechanism that cannot snapshot keeps the default (an empty stream).
///
/// Each emitted [`Change`] carries an [`Ack`](super::Ack); for `live`, the
/// mechanism only advances its durable resume point once changes are confirmed,
/// which makes delivery at-least-once across restarts. Snapshot changes are not
/// resumable (a crashed backfill simply re-runs, idempotently), so their acks
/// need not move any cursor.
///
/// Returned streams are `'static` and `Send`: an implementation moves whatever
/// it needs (its connection, its [`AckSink`](super::AckSink)) into the stream
/// rather than borrowing from `self`.
#[async_trait]
pub trait ChangeCapture: std::fmt::Debug + Send + Sync {
    /// Connect, ensure setup, resume from the last confirmed point, and stream
    /// live changes.
    async fn live(&self) -> Result<BoxStream<'static, Result<Change>>>;

    /// Snapshot the current rows of `tables` as a finite stream of
    /// [`Upsert`](super::ChangeEvent::Upsert) changes — the rows to seed an
    /// index with. The stream ends when the snapshot is complete; there is no
    /// in-band boundary marker.
    ///
    /// The default is an empty stream, for mechanisms that cannot snapshot.
    async fn snapshot(
        &self,
        tables: &[SnapshotTable],
    ) -> Result<BoxStream<'static, Result<Change>>> {
        let _ = tables;
        Ok(Box::pin(stream::empty()))
    }

    /// How far the mechanism's durable resume point trails the source's latest
    /// position, in bytes — e.g. a replication slot's distance from the server's
    /// current WAL LSN. A growing value means the consumer is falling behind the
    /// source; it is the single best signal of pipeline health.
    ///
    /// This is sampled out-of-band (by a supervisor, on a timer), not on the
    /// change path, so it opens its own short-lived connection rather than
    /// borrowing the live stream's. The default is `Ok(None)` — for mechanisms
    /// that have no notion of lag (e.g. a finite snapshot-only source).
    async fn lag(&self) -> Result<Option<u64>> {
        Ok(None)
    }
}

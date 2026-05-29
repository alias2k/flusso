use async_trait::async_trait;
use futures::stream::BoxStream;

use crate::{Change, Result};

/// A pluggable Postgres change-capture mechanism — logical replication (WAL)
/// today, polling or trigger-based capture later.
///
/// Implementations own their own resume state: no position is threaded through
/// this API. [`start`](Self::start) reconnects, ensures whatever setup the
/// mechanism needs (a replication slot and publication, a poll cursor, …), and
/// resumes from where it last left off.
///
/// The returned stream begins with the initial backfill — a run of
/// [`ChangeEvent::Snapshot`](crate::ChangeEvent::Snapshot) events terminated by
/// a single [`ChangeEvent::SnapshotComplete`](crate::ChangeEvent::SnapshotComplete)
/// — and then continues with live changes, all as one continuous stream. The
/// snapshot is taken at a point consistent with where live capture begins, so
/// no change is missed or duplicated across the boundary.
///
/// Each emitted [`Change`] carries an [`Ack`](crate::Ack); the mechanism only
/// advances its durable resume point once changes are confirmed, which makes
/// delivery at-least-once across restarts.
///
/// The stream is `'static` and `Send`: an implementation moves whatever it
/// needs (its connection, its [`AckSink`](crate::AckSink)) into the stream
/// rather than borrowing from `self`.
#[async_trait]
pub trait ChangeCapture: std::fmt::Debug + Send + Sync {
    /// Connect, ensure setup, resume from the last confirmed point, and begin
    /// emitting changes — backfill first, then live changes.
    async fn start(&self) -> Result<BoxStream<'static, Result<Change>>>;
}

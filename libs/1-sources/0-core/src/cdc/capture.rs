use async_trait::async_trait;
use futures::stream::{self, BoxStream};

use crate::{Result, SnapshotTable};

use super::Change;

/// Whether a [`ChangeCapture`]'s durable resume point survived from the
/// previous run — the answer to [`ChangeCapture::continuity`].
///
/// A seed recorded by an earlier run is only trustworthy if every change since
/// then is still observable. That holds exactly when the resume point that fed
/// it is intact; a point that had to be (re)created starts from *now* and has
/// no memory of what happened before.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Continuity {
    /// The resume point already existed: the stream continues where the last
    /// run confirmed, so existing seeds stay valid.
    Resumed,
    /// The resume point does not exist (a first run, a replaced database, a
    /// dropped replication slot): [`prepare`](ChangeCapture::prepare) will
    /// create it from *now*, so anything earlier is unobservable and every
    /// earlier seed is stale.
    Fresh,
}

/// A pluggable change-capture mechanism — logical replication (WAL) today,
/// polling or trigger-based capture later.
///
/// The mechanism exposes three capabilities; the engine decides when to use
/// each:
///
/// - [`continuity`](Self::continuity) reports, read-only, whether the
///   mechanism's durable resume point survived from the last run
///   ([`Continuity`]). The engine asks this first: a resume point that is
///   *missing* invalidates every seed recorded before it.
/// - [`prepare`](Self::prepare) establishes that resume point. The engine calls
///   it after the sinks have been reconciled and before any snapshot, so a seed
///   can never outrun the position that will follow it.
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
    /// Whether the durable resume point survived from the previous run.
    /// **Read-only**: this must not create anything.
    ///
    /// The engine asks this **first**. [`Continuity::Fresh`] means no earlier
    /// seed can be trusted — whatever changed before the resume point existed is
    /// gone for good — so the engine stages a rebuild of every index the sink
    /// still reports as seeded *before* calling [`prepare`](Self::prepare). That
    /// order is what makes the answer crash-safe: the rebuilds are durably
    /// staged at the sink before the resume point comes into existence, so a
    /// crash in between comes back as `Fresh` again and stages them again,
    /// rather than as `Resumed` with the stale seeds now trusted.
    ///
    /// Required (no default) because it is a statement about correctness, not
    /// an optional capability — a mechanism with no durable position of its own
    /// must still say so explicitly.
    async fn continuity(&self) -> Result<Continuity>;

    /// Establish the durable resume point (idempotent: a point that already
    /// exists is left alone).
    ///
    /// The engine calls this after [`continuity`](Self::continuity) and the
    /// sink reconciliation it drives, and **before any snapshot**: once it
    /// returns, every change from this instant on is observable through
    /// [`live`](Self::live), so a backfill that snapshots afterwards can never
    /// miss a write that landed between the snapshot and the first live read.
    async fn prepare(&self) -> Result<()>;

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

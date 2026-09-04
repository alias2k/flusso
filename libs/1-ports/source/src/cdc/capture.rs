use async_trait::async_trait;
use futures::stream::{self, BoxStream};
use kernel::Position;

use crate::{Result, SnapshotTable};

use super::ChangeEvent;

/// A live change: the position the source assigned it, and what changed.
pub type LiveChange = (Position, ChangeEvent);

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
/// The mechanism exposes these capabilities; the ingest engine decides when to
/// use each:
///
/// - [`continuity`](Self::continuity) reports, read-only, whether the
///   mechanism's durable resume point survived from the last run
///   ([`Continuity`]). The daemon asks this first: a resume point that is
///   *missing* invalidates every seed recorded before it.
/// - [`prepare`](Self::prepare) establishes that resume point. It runs after
///   the sinks have staged their rebuilds and before any snapshot, so a seed
///   can never outrun the position that will follow it.
/// - [`live`](Self::live) streams ongoing changes, each with the
///   [`Position`] the mechanism assigned it, resuming from the mechanism's own
///   durable position (a replication slot's `confirmed_flush_lsn`, a poll
///   cursor, …). Positions are monotonic within one run.
/// - [`confirm`](Self::confirm) tells the mechanism that every change up to and
///   including a position has been made durable downstream, so it may advance
///   its resume point. Confirmation is cumulative: confirming `P` covers every
///   position before `P`. The stream's watermark is what the ingest engine
///   confirms, so delivery stays at-least-once across restarts.
/// - [`snapshot`](Self::snapshot) reads the *current* rows of a set of tables as
///   a finite stream — the data a backfill needs. Whether a backfill is
///   *needed* is not the mechanism's call: each sink engine asks its **sink**
///   whether an index is seeded and requests a snapshot only then. A mechanism
///   that cannot snapshot keeps the default (an empty stream).
///
/// Returned streams are `'static` and `Send`: an implementation moves whatever
/// it needs (its connection, its position bookkeeping) into the stream rather
/// than borrowing from `self`.
#[async_trait]
pub trait ChangeCapture: std::fmt::Debug + Send + Sync {
    /// Whether the durable resume point survived from the previous run.
    /// **Read-only**: this must not create anything.
    ///
    /// [`Continuity::Fresh`] means no earlier seed can be trusted — whatever
    /// changed before the resume point existed is gone for good — so every sink
    /// engine stages a rebuild of the indexes its sink still reports as seeded
    /// *before* [`prepare`](Self::prepare) runs. That order is what makes the
    /// answer crash-safe: the rebuilds are durably staged at the sink before
    /// the resume point comes into existence, so a crash in between comes back
    /// as `Fresh` again and stages them again, rather than as `Resumed` with the
    /// stale seeds now trusted.
    ///
    /// Required (no default) because it is a statement about correctness, not
    /// an optional capability — a mechanism with no durable position of its own
    /// must still say so explicitly.
    async fn continuity(&self) -> Result<Continuity>;

    /// Establish the durable resume point (idempotent: a point that already
    /// exists is left alone).
    ///
    /// Runs after [`continuity`](Self::continuity) and the sink staging it
    /// drives, and **before any snapshot**: once it returns, every change from
    /// this instant on is observable through [`live`](Self::live), so a backfill
    /// that snapshots afterwards can never miss a write that landed between the
    /// snapshot and the first live read.
    async fn prepare(&self) -> Result<()>;

    /// Connect, ensure setup, resume from the last confirmed point, and stream
    /// live changes with their positions.
    async fn live(&self) -> Result<BoxStream<'static, Result<LiveChange>>>;

    /// Every change up to and including `position` is durable downstream; the
    /// mechanism may advance its resume point that far. Cheap and non-blocking:
    /// the ingest engine calls it after every commit.
    fn confirm(&self, position: Position);

    /// Snapshot the current rows of `tables` as a finite stream of
    /// [`Upsert`](super::ChangeEvent::Upsert) events — the rows to seed an
    /// index with. The stream ends when the snapshot is complete; there is no
    /// in-band boundary marker.
    ///
    /// The default is an empty stream, for mechanisms that cannot snapshot.
    async fn snapshot(
        &self,
        tables: &[SnapshotTable],
    ) -> Result<BoxStream<'static, Result<ChangeEvent>>> {
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

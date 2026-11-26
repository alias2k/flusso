use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::stream::BoxStream;
use kernel::Position;

use crate::{Result, SourceError};

use super::ChangeEvent;

/// A live change: the position the source assigned it, and what changed.
pub type LiveChange = (Position, ChangeEvent);

/// A point in the source's own change order, as a client can read it from the
/// source itself (Postgres: a WAL LSN).
///
/// Unlike a [`Position`], which the mechanism assigns and nobody outside it
/// can interpret, this is what a client holds after its own commit and hands
/// back to wait on. Ordered: a later commit never has a lower one. Only the
/// mechanism parses and prints it ([`ChangeCapture::parse_position`],
/// [`ChangeCapture::format_position`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourcePosition(pub u64);

/// How far every lane that needs a change has landed it, in positions: what
/// the engine hands [`ChangeCapture::confirm`] and [`ChangeCapture::reached`].
///
/// The mechanism turns it into its own terms with the stamps the positions
/// carry, so it keeps no map of the changes it emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Landed {
    /// Every change before this one has landed; this one is the oldest still
    /// outstanding on some lane.
    Before(Position),
    /// Every change through this one has landed, and it is the newest the
    /// engine has routed. A change emitted after it may not be routed yet:
    /// only the mechanism, which knows what it emitted, can tell.
    Through(Position),
    /// The engine has routed nothing yet. [`ChangeCapture::reached`] answers
    /// it at or past where the current stream started, so at or past
    /// anything an earlier holder of the slot dropped: the engine settles a
    /// reattach's gap end where a lane that routed nothing stands, and a gap
    /// must hold what was dropped (ADR 0012 stage 10, the fifth re-check).
    Idle,
    /// No lane needs anything: every lane is detached (ADR 0012 stage 10).
    /// Everything the mechanism has emitted may be confirmed, and the resume
    /// point follows the source's own horizon, which never passes a change it
    /// hasn't emitted.
    Free,
}

impl Landed {
    /// For a position-only view (a mock, a log line): the `seq` up to which
    /// nothing routed is outstanding. For `Before(o)` that is `o.seq - 1`,
    /// which says nothing of a position there that was emitted but never
    /// routed: only the source knows of those.
    pub fn covered_seq(self) -> Option<u64> {
        match self {
            Landed::Before(oldest) => oldest.seq.checked_sub(1),
            Landed::Through(last) => Some(last.seq),
            Landed::Idle => None,
            Landed::Free => Some(u64::MAX),
        }
    }
}

/// Called by a mechanism when [`ChangeCapture::reached`] may answer further.
pub type AdvanceListener = Arc<dyn Fn() + Send + Sync>;

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
///   cursor, …). Positions strictly increase within one stream, and over
///   the capture's life a position never names a different change than it
///   named before: a reopened stream either keeps numbering (as the
///   Postgres capture does) or gives a replayed change the position it had.
///   Numbering anew with new meanings is unsafe: an answer kept about an
///   earlier position would cover a change it never named.
/// - [`confirm`](Self::confirm) tells the mechanism how far every lane has
///   landed the changes it emitted ([`Landed`]), so it may advance its resume
///   point that far and no further. The mechanism reads the answer off the
///   positions' own stamps and what it knows it emitted, so delivery stays
///   at-least-once across restarts with no map of every change.
///
/// A backfill is not the mechanism's: it reads root keys through the document
/// builder ([`DocumentBuilder::keys`](crate::document::DocumentBuilder::keys)).
///
/// - [`reached`](Self::reached) translates progress back into the source's own
///   terms: given how far a sink's lanes have landed, the [`SourcePosition`]
///   that is fully covered. It is what a fence compares a client's position
///   with. A mechanism that cannot answer keeps the default (`None`) and
///   fences are unavailable.
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
    /// live changes with their positions, in source commit order.
    ///
    /// **A change is yielded only once it can be read back**: every read the
    /// [`DocumentBuilder`](crate::document::DocumentBuilder) makes after
    /// receiving it sees its transaction committed. A source whose change feed
    /// can run ahead of what its readers see (Postgres decodes a commit before
    /// new snapshots include it) holds the change, and every later one, until
    /// then; [`readable_wait`](Self::readable_wait) reports how long.
    async fn live(&self) -> Result<BoxStream<'static, Result<LiveChange>>>;

    /// The lanes have landed `landed`; the mechanism may advance its resume
    /// point as far as that covers, and never into a change emitted but not
    /// landed: one in no lane yet is the mechanism's to know (`Through(p)`
    /// says only that `p` is the newest routed). Monotonic: an answer behind
    /// the resume point moves nothing. Cheap and non-blocking: the engine
    /// calls it after every commit and once a second, whether or not `landed`
    /// changed, since the answer can move with it unchanged (idle WAL).
    fn confirm(&self, landed: Landed);

    /// Ask the open live stream to end gracefully: hand the position last
    /// [`confirm`](Self::confirm)ed to the source, then end the stream
    /// without yielding another change. The caller polls the stream to its
    /// end. `false` (the default) means this mechanism can't, and the caller
    /// drops the stream instead, so the last confirm may not reach the source:
    /// the next start then reads those changes again, which at-least-once
    /// delivery allows.
    fn close_live(&self) -> bool {
        false
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

    /// How long the live stream has been holding a committed change that can't
    /// be read back yet (see [`live`](Self::live)); `None` while nothing waits.
    /// Cheap and non-blocking: sampled out of band, like [`lag`](Self::lag).
    /// How many live streams this source has begun: each call of
    /// [`live`](Self::live) counts, before it does anything. The engine opens
    /// every stream through one place that counts its openings first (ADR
    /// 0012 stage 10, O29), so a count ahead of those openings is a stream
    /// opened past them: a fence then vouches for no lane. `None` when the
    /// source doesn't count, the default.
    fn streams_opened(&self) -> Option<u64> {
        None
    }

    fn readable_wait(&self) -> Option<Duration> {
        None
    }

    /// Parse a position a client read from the source (Postgres: the text of
    /// `pg_current_wal_lsn()`). The default refuses: the mechanism has no
    /// position a client could read.
    fn parse_position(&self, text: &str) -> Result<SourcePosition> {
        let _ = text;
        Err(SourceError::Unsupported(
            "this source has no position a client can wait on".to_owned(),
        ))
    }

    /// Print `position` the way the source itself does, so it round-trips
    /// through [`parse_position`](Self::parse_position).
    fn format_position(&self, position: SourcePosition) -> String {
        position.0.to_string()
    }

    /// How far the source's own order is covered once a sink's lanes have
    /// landed `landed`: every source commit at or before the returned
    /// position is among what landed. A fence compares a client's position
    /// with it.
    ///
    /// The answer keeps moving while the watched tables are idle: with nothing
    /// emitted after what landed, it follows what the mechanism has read.
    /// [`Landed::Idle`] is never answered below where the current stream
    /// started (the slot's confirmed position at the opening).
    /// Returns `None` while the mechanism cannot tell (no live stream yet, or
    /// no notion of a source position at all — the default).
    fn reached(&self, landed: Landed) -> Option<SourcePosition> {
        let _ = landed;
        None
    }

    /// Register `listener`, called whenever [`reached`](Self::reached) may
    /// answer further for an unchanged `position` (the mechanism read on with
    /// nothing new to deliver). Cheap and non-blocking for the mechanism: the
    /// listener must only wake something. The default never calls it.
    fn on_advance(&self, listener: AdvanceListener) {
        let _ = listener;
    }
}

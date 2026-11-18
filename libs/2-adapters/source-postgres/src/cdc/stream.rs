//! The state machine that turns a `pgwire-replication` event stream into a
//! stream of positioned, thin [`ChangeEvent`]s.
//!
//! Row changes are buffered per transaction and only emitted once the `Commit`
//! arrives, tagged with the commit LSN. That gives every change a clean,
//! commit-aligned position to acknowledge against, and matches logical
//! decoding's "whole transactions only" model. Because events are thin (a table
//! name, primary key, and at most a pre- and post-image), buffering even a large
//! transaction is cheap. An update's post-image is the one wider payload, and
//! it is kept only beside a pre-image.
//!
//! Each `Relation` message is followed by a catalog lookup of the table's
//! primary key (and whether it is partitioned, which withholds the post-image),
//! before the next replication event is read: a change is keyed by
//! its primary key, not by the replica-identity columns pgoutput flags (all of
//! them, under `REPLICA IDENTITY FULL`). See [`pgoutput`].
//!
//! A committed transaction is emitted only once it can be read back (issue
//! #343): Postgres flushes a commit record before new snapshots see the
//! transaction, so the stream holds it in `pending` until a snapshot includes
//! its xid ([`super::visibility`]). Holding it there is what keeps it out of
//! every confirmation and fence: nothing in `pending` is registered, and the
//! horizon never moves past buffered changes. While the oldest transaction is
//! held the stream reads on, up to its read-ahead bound, and checks every
//! transaction read so far with one snapshot query; a transaction that touches
//! no table an index reads needs no check. Commit order is kept: one held
//! transaction holds back every later one.
//!
//! The loop also hands the confirmed position to the replication client: on
//! every turn, during a hold too, and the moment a confirmation moves it,
//! without waiting for the next replication event. The client reports it to
//! the server within its status interval, so the slot releases WAL seconds
//! after the sinks land it.

use std::collections::{HashMap, HashSet, VecDeque};
use std::future::Future;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use futures::stream::{self, BoxStream};
use kernel::{ColumnName, Position, TableName};
use pgwire_replication::{Lsn, PgWireError, ReplicationClient, ReplicationEvent};
use source::cdc::{ChangeEvent, LiveChange};
use source::{PreImageColumns, QualifiedTable, Result, RowKey, SourceError};
use sqlx::PgPool;
use tokio::time::Instant;

use super::ack::Positions;
use super::pgoutput::{self, Decoded, Relation};
use super::replica_identity;
use super::visibility::{Visibility, poll_delay};
use crate::ColumnTypes;

/// How long a transaction may stay held before the stream warns about it, and
/// checks whether the server made flusso a synchronous standby.
const STALL_WARNING: Duration = Duration::from_secs(5);

/// How often the warning repeats while the hold lasts.
const STALL_REPEAT: Duration = Duration::from_secs(60);

/// Which transaction the stream holds and since when, shared with the capture,
/// which reports it. It outlives a stream: a reopened stream that holds the
/// same transaction again goes on from the same instant, so the wait and its
/// warning count from when the transaction was first found invisible, not
/// from the last restart. Cleared once that transaction is emitted, and when
/// the capture refuses the server.
pub(crate) type HoldClock = Arc<Mutex<Option<(u32, Instant)>>>;

/// How long the hold `clock` records has lasted; `None` when nothing waits.
pub(crate) fn held_for(clock: &HoldClock) -> Option<Duration> {
    clock
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .map(|(_, since)| since.elapsed())
}

/// Forget the hold `clock` records.
pub(crate) fn clear_hold(clock: &HoldClock) {
    *clock.lock().unwrap_or_else(PoisonError::into_inner) = None;
}

/// What the stream needs besides the connections.
pub(crate) struct Settings {
    /// Per table, the columns resolution needs from a pre-image.
    pub(crate) pre_image_links: PreImageColumns,
    /// The tables an index reads, by name as resolution matches them; a
    /// transaction touching none needs no visibility check. `None`: every table.
    pub(crate) read_tables: Option<HashSet<TableName>>,
    /// How many changes the stream reads ahead while a transaction is held.
    pub(crate) read_ahead: usize,
    /// Where the stream records how long it has been holding.
    pub(crate) hold_clock: HoldClock,
}

/// What the stream needs from the replication connection: its events, and a
/// place to report the confirmed position. A seam so tests can stand in for
/// the server.
pub(crate) trait Replication: Send + 'static {
    /// The next replication event; `None` once replication ended. Must be
    /// cancel-safe: the loop drops it when a confirmation wakes it.
    fn recv(
        &mut self,
    ) -> impl Future<Output = std::result::Result<Option<ReplicationEvent>, PgWireError>> + Send;

    /// Report everything through `lsn` as applied. Monotonic; the client
    /// sends a raised LSN to the server within its status interval.
    fn update_applied_lsn(&self, lsn: Lsn);

    /// End replication gracefully, once `report` (when there is one) has been
    /// sent to the server. Answers whether it was: `false` means the server
    /// may not hold the final position. The default does nothing beyond what
    /// `update_applied_lsn` did, and answers `true`.
    fn close(&mut self, report: Option<Lsn>) -> impl Future<Output = bool> + Send {
        let _ = report;
        async { true }
    }
}

/// The longest a closing stream waits for the server to be sent the final
/// position, and then for the replication worker to end.
const CLOSE_STEP: Duration = Duration::from_secs(5);

impl Replication for ReplicationClient {
    fn recv(
        &mut self,
    ) -> impl Future<Output = std::result::Result<Option<ReplicationEvent>, PgWireError>> + Send
    {
        ReplicationClient::recv(self)
    }

    fn update_applied_lsn(&self, lsn: Lsn) {
        ReplicationClient::update_applied_lsn(self, lsn);
    }

    /// Waits until a standby status update carried `lsn` (the worker sends a
    /// raised position within its status interval), then stops the worker,
    /// which sends CopyDone. Stopping first would lose the update: the worker
    /// checks its stop signal before a due status update.
    async fn close(&mut self, report: Option<Lsn>) -> bool {
        let mut sent = true;
        if let Some(lsn) = report {
            let metrics = self.metrics();
            sent = tokio::time::timeout(CLOSE_STEP, async {
                while metrics.last_applied_lsn() < lsn {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .is_ok();
        }
        match tokio::time::timeout(CLOSE_STEP, self.shutdown()).await {
            Ok(Ok(())) => tracing::info!(?report, sent, "replication stream closed"),
            Ok(Err(error)) => tracing::warn!(%error, "replication stream closed with an error"),
            Err(_) => tracing::warn!("replication stream did not close in time; dropping it"),
        }
        sent
    }
}

/// Everything the unfold loop carries between polls.
struct State<C, V> {
    client: C,
    /// Catalog connection for the per-relation primary-key lookup.
    catalog: PgPool,
    visibility: V,
    decode: DecodeState,
    read_ahead: usize,
    /// Snapshot reads, shared with the task that makes them.
    refresh: Arc<Refresh>,
    /// The task reading snapshots while a transaction awaits a check.
    refresher: Option<Refresher>,
    hold: Option<Hold>,
    hold_clock: HoldClock,
    /// The stream was closed on request: it ends.
    closed: bool,
}

/// Snapshot reads made on their own task, so they run while the engine builds
/// and publishes. The task lives as long as the stream: it reads while the
/// stream wants it (some transaction awaits a check) and otherwise sleeps on
/// `wake`, so a burst finds a recent snapshot whenever the engine comes back,
/// a lone change costs one read, and an idle stream none. It never exits on
/// its own, so a commit arriving as it goes idle can't find it gone: the
/// stream raises `wanted` then wakes it, and `Notify` keeps that wake-up for a
/// task not yet waiting.
#[derive(Debug)]
struct Refresh {
    /// The newest read and when it was asked for, not taken by the stream yet.
    latest: Mutex<Option<(Instant, Result<super::visibility::Snapshot>)>>,
    /// Woken by every read.
    arrived: tokio::sync::Notify,
    /// Woken by the stream when it starts wanting reads.
    wake: tokio::sync::Notify,
    /// Whether the stream wants another read.
    wanted: std::sync::atomic::AtomicBool,
    /// The pause between two reads, in microseconds: the poll schedule.
    spacing: std::sync::atomic::AtomicU64,
}

impl Default for Refresh {
    fn default() -> Self {
        Self {
            latest: Mutex::default(),
            arrived: tokio::sync::Notify::new(),
            wake: tokio::sync::Notify::new(),
            wanted: std::sync::atomic::AtomicBool::new(false),
            spacing: std::sync::atomic::AtomicU64::new(FIRST_SPACING_MICROS),
        }
    }
}

/// The pause between reads while no transaction has waited long.
const FIRST_SPACING_MICROS: u64 = 500;

/// How old the stream's newest snapshot may be and still be checked against
/// on a commit's arrival. Bounded so an xid is never widened against a
/// snapshot from 2^31 transactions ago.
const SNAPSHOT_REUSE: Duration = Duration::from_secs(1);

/// The refreshing task, aborted with the stream.
struct Refresher(tokio::task::JoinHandle<()>);

impl Drop for Refresher {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// How long one snapshot read may take before it counts as failed: a read on
/// a dead connection must not hold the stream silently.
pub(crate) const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// How long reads may keep failing before the stream ends with the error: a
/// connection blip or a pool timeout under load retries, a dead server stops
/// the stream (the engine's restart then retries the whole stream).
pub(crate) const READ_FAILURES_FOR: Duration = Duration::from_secs(30);

/// Read snapshots into `refresh` while the stream wants them, for the life of
/// the stream. A read that fails or times out is retried with backoff; one
/// still failing after [`READ_FAILURES_FOR`] is handed to the stream, which
/// ends with it.
async fn refresh<V: Visibility>(visibility: V, refresh: Arc<Refresh>) {
    use std::sync::atomic::Ordering;
    let mut failing_since: Option<Instant> = None;
    let mut backoff = Duration::from_millis(100);
    loop {
        while !refresh.wanted.load(Ordering::SeqCst) {
            refresh.wake.notified().await;
        }
        let asked = Instant::now();
        let read = tokio::time::timeout(READ_TIMEOUT, visibility.snapshot())
            .await
            .unwrap_or_else(|_| {
                Err(SourceError::Query(format!(
                    "reading a snapshot took longer than {READ_TIMEOUT:?}"
                )))
            });
        let pause = match read {
            Ok(snapshot) => {
                failing_since = None;
                backoff = Duration::from_millis(100);
                *refresh
                    .latest
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) = Some((asked, Ok(snapshot)));
                refresh.arrived.notify_one();
                Duration::from_micros(refresh.spacing.load(Ordering::Relaxed))
            }
            Err(error) => {
                let since = *failing_since.get_or_insert_with(Instant::now);
                if since.elapsed() >= READ_FAILURES_FOR {
                    *refresh
                        .latest
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner) = Some((asked, Err(error)));
                    refresh.arrived.notify_one();
                    return;
                }
                tracing::warn!(%error, "reading a snapshot failed; retrying");
                let pause = backoff;
                backoff = (backoff * 2).min(Duration::from_secs(2));
                pause
            }
        };
        tokio::time::sleep(pause).await;
    }
}

/// The oldest transaction in `pending`, found not visible yet.
#[derive(Debug, Clone, Copy)]
struct Hold {
    xid: u32,
    since: Instant,
    delay: Duration,
    warn_at: Instant,
}

/// One change of a committed transaction, waiting to be emitted.
struct Pending {
    event: ChangeEvent,
    /// The commit LSN to acknowledge at.
    lsn: u64,
    xid: u32,
}

/// The buffering/progress state [`handle`] folds replication events into —
/// everything except the connection, so tests can drive it with synthetic
/// events.
struct DecodeState {
    /// Relation metadata by OID, accumulated from `Relation` messages.
    relations: HashMap<u32, Relation>,
    /// Relations whose primary key has not been looked up yet.
    unkeyed: Vec<u32>,
    /// Per table, the columns resolution needs from a pre-image.
    pre_image_links: PreImageColumns,
    /// Per relation, the needed pre-image columns its replica identity does not
    /// carry — set when its `Relation` arrives.
    untraceable: HashMap<u32, Vec<ColumnName>>,
    /// Relations already warned about as untraceable.
    warned: HashSet<u32>,
    /// Relations already warned about for an old tuple without the key, per
    /// kind of change.
    warned_unkeyed: HashSet<(u32, Unkeyed)>,
    /// Row changes of the currently open transaction, awaiting its `Commit`.
    open_txn: Vec<ChangeEvent>,
    /// The open transaction's xid, from its `Begin`.
    open_xid: u32,
    /// Changes of committed transactions, in commit order. Only the front is
    /// emitted, and only once its transaction is in `readable`.
    pending: VecDeque<Pending>,
    /// The xids in `pending` that may be read back: visible to a snapshot, or
    /// touching no table an index reads. Dropped once emitted.
    readable: HashSet<u32>,
    /// The xids in `pending` no snapshot has shown visible yet: what the next
    /// snapshot read is checked against.
    unchecked: HashSet<u32>,
    /// The xid of the last change emitted.
    last_emitted: Option<u32>,
    /// The newest snapshot the stream read, and when: a commit it already
    /// shows visible needs no read of its own.
    last_snapshot: Option<(Instant, super::visibility::Snapshot)>,
    /// See [`Settings::read_tables`].
    read_tables: Option<HashSet<TableName>>,
    ack: Arc<Positions>,
    /// Set once the underlying stream is finished; drains `pending`, then ends.
    done: bool,
    /// The document builder's column types: a `Relation` naming other types
    /// than the cached ones drops its table's.
    column_types: Option<ColumnTypes>,
}

impl DecodeState {
    /// Take the next change ready to emit and register its position. Emitting
    /// the last change of a commit also moves the horizon to that commit: from
    /// here on, everything read through it is registered.
    /// A held front transaction emits nothing.
    fn next_change(&mut self) -> Option<LiveChange> {
        if !self.readable.contains(&self.pending.front()?.xid) {
            return None;
        }
        let Pending { event, lsn, xid } = self.pending.pop_front()?;
        self.last_emitted = Some(xid);
        let seq = self.ack.register(lsn);
        if self.pending.front().is_none_or(|next| next.lsn != lsn) {
            self.ack.read_through(lsn);
            self.readable.remove(&xid);
        }
        Some((Position::new(seq), event))
    }

    /// The xid of the front transaction while it may not be read back yet.
    fn held(&self) -> Option<u32> {
        self.pending
            .front()
            .map(|front| front.xid)
            .filter(|xid| !self.readable.contains(xid))
    }

    /// Mark readable every unchecked transaction `snapshot` sees, and keep it
    /// for the commits that arrive next. `read_at` is when the read was asked
    /// for, not when the stream got round to taking it: a read can wait a long
    /// time for a stream nobody pulls, and reuse is bounded by its real age.
    fn take_snapshot(&mut self, snapshot: super::visibility::Snapshot, read_at: Instant) {
        self.seen_by(&snapshot);
        self.last_snapshot = Some((read_at, snapshot));
    }

    /// Whether the newest snapshot, if recent, already shows `xid` visible.
    fn seen(&self, xid: u32) -> bool {
        self.last_snapshot
            .as_ref()
            .is_some_and(|(taken, snapshot)| taken.elapsed() < SNAPSHOT_REUSE && snapshot.sees(xid))
    }

    /// Mark readable every unchecked transaction `snapshot` sees.
    fn seen_by(&mut self, snapshot: &super::visibility::Snapshot) {
        let seen: Vec<u32> = self
            .unchecked
            .iter()
            .copied()
            .filter(|xid| snapshot.sees(*xid))
            .collect();
        for xid in seen {
            self.unchecked.remove(&xid);
            self.readable.insert(xid);
        }
    }
}

/// Build the live-changes stream from a connected client.
///
/// This carries only live changes; a backfill, when a sink needs one, is a
/// separate snapshot stream (see [`super::backfill`]). Each emitted change
/// registers its commit LSN against `ack` and is tagged with the resulting
/// [`Position`]; the capture's `confirm` reports how far the engine has durably
/// confirmed, so the slot's `confirmed_flush_lsn` advances only that far. While
/// the watched tables are idle, keepalive WAL ends and filtered-out commits
/// move the bookkeeping's horizon ([`Positions::read_through`]), so the slot
/// keeps advancing, the server can recycle WAL generated by unrelated traffic,
/// and a fence on a position past the last watched commit still passes.
pub(crate) fn build<C: Replication, V: Visibility>(
    client: C,
    catalog: PgPool,
    visibility: V,
    settings: Settings,
    ack: Arc<Positions>,
    column_types: Option<ColumnTypes>,
) -> BoxStream<'static, Result<LiveChange>> {
    let state = State {
        client,
        catalog,
        visibility,
        decode: DecodeState {
            relations: HashMap::new(),
            unkeyed: Vec::new(),
            pre_image_links: settings.pre_image_links,
            untraceable: HashMap::new(),
            warned: HashSet::new(),
            warned_unkeyed: HashSet::new(),
            open_txn: Vec::new(),
            open_xid: 0,
            pending: VecDeque::new(),
            readable: HashSet::new(),
            unchecked: HashSet::new(),
            last_emitted: None,
            last_snapshot: None,
            read_tables: settings.read_tables,
            ack,
            done: false,
            column_types,
        },
        read_ahead: settings.read_ahead.max(1),
        refresh: Arc::default(),
        refresher: None,
        hold: None,
        hold_clock: settings.hold_clock,
        closed: false,
    };
    into_stream(state)
}

/// The unfold loop over `state`.
fn into_stream<C: Replication, V: Visibility>(
    state: State<C, V>,
) -> BoxStream<'static, Result<LiveChange>> {
    Box::pin(stream::unfold(state, |mut state| async move {
        loop {
            // Report how far the engine has durably confirmed so the slot can
            // advance and the server can recycle WAL. Every turn, so the
            // horizon moving on idle progress is reported too.
            let confirmed = Lsn::from_u64(state.decode.ack.confirmed_lsn());
            state.client.update_applied_lsn(confirmed);
            if state.closed {
                return None;
            }
            if state.decode.ack.close_requested() {
                state.closed = true;
                let report =
                    (confirmed.as_u64() > state.decode.ack.start_lsn()).then_some(confirmed);
                if state.client.close(report).await {
                    return None;
                }
                return Some((
                    Err(SourceError::Connection(format!(
                        "the final position was not confirmed to the source: {confirmed} was not \
                         sent to Postgres within {CLOSE_STEP:?}; the next start reads the changes \
                         after the last confirmed position again"
                    ))),
                    state,
                ));
            }

            if let Err(e) = state.take_latest() {
                state.decode.done = true;
                return Some((Err(e), state));
            }
            if let Some(change) = state.decode.next_change() {
                state.emitted();
                state.want_snapshots();
                return Some((Ok(change), state));
            }
            state.want_snapshots();

            let Some(xid) = state.decode.held() else {
                state.hold = None;
                if state.decode.done {
                    return None;
                }
                // A confirmation moving the resume point turns the loop at
                // once, so the client hears it now rather than at the next
                // event (a keepalive can be 30 s away on a quiet stream).
                // Dropping `recv` loses nothing while the worker runs: it is a
                // channel receive. The one unsafe moment is after the channel
                // closed, when `recv` awaits the worker's `JoinHandle`: if a
                // wake-up wins then, the handle is dropped, and the next turn
                // reports a generic "replication worker already joined" error
                // instead of the worker's own outcome. The engine restarts the
                // stream either way.
                let received = tokio::select! {
                    biased;
                    received = state.client.recv() => received,
                    () = state.decode.ack.confirmed_moved() => continue,
                };
                if let Err(e) = state.receive(received).await {
                    return Some((Err(e), state));
                }
                continue;
            };

            if let Err(e) = state.holding(xid).await {
                state.decode.done = true;
                return Some((Err(e), state));
            }
            // Held: confirmations keep flowing, the stream reads on up to its
            // bound, and the next snapshot read turns the loop.
            let room = !state.decode.done && state.decode.pending.len() < state.read_ahead;
            let refresh = Arc::clone(&state.refresh);
            tokio::select! {
                biased;
                () = state.decode.ack.confirmed_moved() => {}
                () = refresh.arrived.notified() => {}
                received = state.client.recv(), if room => {
                    if let Err(e) = state.receive(received).await {
                        return Some((Err(e), state));
                    }
                }
            }
        }
    }))
}

impl<C: Replication, V: Visibility> State<C, V> {
    /// Fold one received event in; an error ends the stream.
    async fn receive(
        &mut self,
        received: std::result::Result<Option<ReplicationEvent>, PgWireError>,
    ) -> std::result::Result<(), SourceError> {
        let handled = match received {
            Ok(Some(event)) => match handle(&mut self.decode, event) {
                Ok(()) => key_relations(&self.catalog, &mut self.decode).await,
                Err(e) => Err(e),
            },
            Ok(None) => {
                self.decode.done = true;
                Ok(())
            }
            Err(e) => Err(map_pgwire(e)),
        };
        if handled.is_err() {
            self.decode.done = true;
        }
        handled
    }

    /// Have the snapshot task read while a transaction awaits a check, and
    /// rest otherwise. Started with the first check; it never stops on its
    /// own, so it is restarted only after a read failure ended it.
    fn want_snapshots(&mut self) {
        use std::sync::atomic::Ordering;
        let wanted = !self.decode.unchecked.is_empty();
        self.refresh.wanted.store(wanted, Ordering::SeqCst);
        if !wanted {
            return;
        }
        if self
            .refresher
            .as_ref()
            .is_none_or(|refresher| refresher.0.is_finished())
        {
            self.refresher = Some(Refresher(tokio::spawn(refresh(
                self.visibility.clone(),
                Arc::clone(&self.refresh),
            ))));
        }
        self.refresh.wake.notify_one();
    }

    /// A transaction was emitted: once the one the clock times is out, the
    /// clock is cleared.
    fn emitted(&mut self) {
        let Some(xid) = self.decode.last_emitted else {
            return;
        };
        if self.hold.is_some_and(|hold| hold.xid == xid) {
            self.hold = None;
        }
        let mut clock = self
            .hold_clock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if clock.is_some_and(|(held, _)| held == xid) {
            *clock = None;
        }
    }

    /// Take the newest snapshot read, if one arrived, and mark what it sees.
    /// While the oldest transaction stays invisible, reads follow the poll
    /// schedule; otherwise they come as fast as the first.
    fn take_latest(&mut self) -> std::result::Result<(), SourceError> {
        use std::sync::atomic::Ordering;
        let latest = self
            .refresh
            .latest
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        let Some((read_at, read)) = latest else {
            return Ok(());
        };
        self.decode.take_snapshot(read?, read_at);
        let spacing = match self.hold.as_mut() {
            Some(hold) if self.decode.held() == Some(hold.xid) => {
                hold.delay = poll_delay(hold.since.elapsed(), hold.delay);
                hold.delay
            }
            _ => Duration::from_micros(FIRST_SPACING_MICROS),
        };
        self.refresh.spacing.store(
            u64::try_from(spacing.as_micros()).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        Ok(())
    }

    /// The front transaction `xid` is held: time it from when it was first
    /// found invisible (by this stream or one before it), and past
    /// [`STALL_WARNING`] report it.
    async fn holding(&mut self, xid: u32) -> std::result::Result<(), SourceError> {
        let hold = match self.hold {
            Some(hold) if hold.xid == xid => hold,
            _ => {
                let since = {
                    let mut clock = self
                        .hold_clock
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner);
                    match *clock {
                        Some((held, since)) if held == xid => since,
                        _ => {
                            let now = Instant::now();
                            *clock = Some((xid, now));
                            now
                        }
                    }
                };
                let hold = Hold {
                    xid,
                    since,
                    delay: Duration::ZERO,
                    warn_at: since + STALL_WARNING,
                };
                self.hold = Some(hold);
                hold
            }
        };
        if Instant::now() >= hold.warn_at {
            self.report_stall(hold).await?;
        }
        Ok(())
    }

    /// A transaction held past [`STALL_WARNING`]: stop if the server now waits
    /// on flusso as a synchronous standby, else warn and name the sessions
    /// waiting on synchronous replication.
    ///
    /// The inspection is best effort: one that fails or takes longer than
    /// [`READ_TIMEOUT`] is reported in the warning and tried again with the
    /// next one, and the hold goes on.
    async fn report_stall(&mut self, hold: Hold) -> std::result::Result<(), SourceError> {
        let inspected = tokio::time::timeout(READ_TIMEOUT, self.visibility.stall(hold.xid))
            .await
            .unwrap_or_else(|_| {
                Err(SourceError::Query(format!(
                    "inspecting the held commit took longer than {READ_TIMEOUT:?}"
                )))
            });
        let waiting = match inspected {
            Ok(stall) => {
                if let Some(refusal) = stall.refusal {
                    clear_hold(&self.hold_clock);
                    return Err(refusal);
                }
                if stall.waiting.is_empty() {
                    "none this role can see (pg_read_all_stats shows every session)".to_owned()
                } else {
                    stall.waiting.join("; ")
                }
            }
            Err(error) => format!("unknown: {error}"),
        };
        tracing::warn!(
            xid = hold.xid,
            waited_seconds = hold.since.elapsed().as_secs(),
            sessions_waiting_on_synchronous_replication = %waiting,
            "a committed transaction is not visible yet: flusso waits for it, and every commit \
             after it waits too. Postgres keeps a commit invisible while it waits for a \
             synchronous standby; check the standby and the sessions listed"
        );
        if let Some(current) = self.hold.as_mut() {
            current.warn_at = Instant::now() + STALL_REPEAT;
        }
        Ok(())
    }
}

/// Look up the catalog primary key of every relation announced since the last
/// event, then which parent links its identity can't deliver (that depends on
/// the key). A table without one keeps an empty key and is keyed by its
/// replica identity instead. A domain column is retyped to its base type, so
/// its values decode as the document builder reads them back.
async fn key_relations(
    catalog: &PgPool,
    state: &mut DecodeState,
) -> std::result::Result<(), SourceError> {
    for oid in std::mem::take(&mut state.unkeyed) {
        let Some(relation) = state.relations.get_mut(&oid) else {
            continue;
        };
        let qualified = format!(
            "{}.{}",
            super::quote_ident(relation.namespace.as_ref()),
            super::quote_ident(relation.table.as_ref())
        );
        relation.partitioned = Some(crate::document::is_partitioned(catalog, &qualified).await?);
        retype_domains(catalog, &mut relation.columns).await?;
        relation.primary_key = crate::document::primary_key_column_names(catalog, qualified)
            .await?
            .into_iter()
            .map(|name| {
                ColumnName::try_new(name.clone()).map_err(|e| {
                    SourceError::Decode(format!("invalid primary key column {name:?}: {e}"))
                })
            })
            .collect::<std::result::Result<_, _>>()?;
        record_untraceable(state, oid);
    }
    Ok(())
}

/// Replace each domain column's type OID with its base type's, through nested
/// domains. A `Relation` names a domain column by the domain's OID, which
/// `text_value::typed_value` doesn't know, while a query's row description
/// names it by its base type: left as text, a domain key would miss its
/// document (issue #295). Only columns of a type `typed_value` doesn't
/// already decode are looked up.
async fn retype_domains(catalog: &PgPool, columns: &mut [pgoutput::Column]) -> Result<()> {
    let unknown: Vec<i64> = columns
        .iter()
        .filter(|column| !crate::text_value::is_decoded(column.type_oid))
        .map(|column| i64::from(column.type_oid))
        .collect();
    if unknown.is_empty() {
        return Ok(());
    }
    let bases: Vec<(i64, i64)> = sqlx::query_as(
        "WITH RECURSIVE base(oid, ty) AS ( \
           SELECT oid, oid FROM pg_type WHERE oid = ANY($1::oid[]) \
           UNION ALL \
           SELECT base.oid, t.typbasetype FROM base JOIN pg_type t ON t.oid = base.ty \
           WHERE t.typtype = 'd' \
         ) \
         SELECT base.oid::int8, base.ty::int8 FROM base JOIN pg_type t ON t.oid = base.ty \
         WHERE t.typtype <> 'd'",
    )
    .bind(unknown)
    .fetch_all(catalog)
    .await
    .map_err(|e| SourceError::Query(e.to_string()))?;
    for column in columns {
        if let Some((_, base)) = bases
            .iter()
            .find(|(oid, _)| *oid == i64::from(column.type_oid))
            && let Ok(base) = u32::try_from(*base)
        {
            column.type_oid = base;
        }
    }
    Ok(())
}

/// Fold one replication event into the state, possibly queueing changes.
fn handle(
    state: &mut DecodeState,
    event: ReplicationEvent,
) -> std::result::Result<(), SourceError> {
    match event {
        // The worker replies to keepalives; here the reported WAL end moves the
        // horizon so the slot advances while the watched tables are idle.
        // Buffered or queued changes are not registered yet, so the horizon
        // must not pass them — hence the emptiness guard. Changes already
        // emitted need no guard: their entries hold every answer back until
        // they are confirmed.
        ReplicationEvent::KeepAlive { wal_end, .. } => {
            if state.open_txn.is_empty() && state.pending.is_empty() {
                state.ack.read_through(wal_end.as_u64());
            }
        }

        // Logical messages are not changes.
        ReplicationEvent::Message { .. } => {}

        ReplicationEvent::Begin { xid, .. } => {
            state.open_txn.clear();
            state.open_xid = xid;
        }

        ReplicationEvent::Commit { end_lsn, .. } => {
            let lsn = end_lsn.as_u64();
            if state.open_txn.is_empty() {
                // A transaction the publication filtered out entirely (PG < 16
                // still streams its Begin/Commit) — account its position so it
                // doesn't pin WAL retention. Same guard rationale as keepalive.
                if state.pending.is_empty() {
                    state.ack.read_through(lsn);
                }
            } else {
                let xid = state.open_xid;
                let read = state.read_tables.as_ref().is_none_or(|read| {
                    state
                        .open_txn
                        .iter()
                        .any(|change| read.contains(change.table()))
                });
                if read && !state.seen(xid) {
                    state.unchecked.insert(xid);
                } else {
                    state.readable.insert(xid);
                }
                for event in state.open_txn.drain(..) {
                    state.pending.push_back(Pending { event, lsn, xid });
                }
            }
        }

        // Stop requested / stop_at_lsn reached: drain what we have, then end.
        ReplicationEvent::StoppedAt { .. } => state.done = true,

        ReplicationEvent::XLogData { data, .. } => handle_xlog(state, data.as_ref())?,
    }
    Ok(())
}

fn handle_xlog(state: &mut DecodeState, data: &[u8]) -> std::result::Result<(), SourceError> {
    match pgoutput::decode(data)? {
        Decoded::Relation(relation) => {
            if let Some(column_types) = &state.column_types {
                column_types.forget_changed(
                    relation.namespace.as_ref(),
                    relation.table.as_ref(),
                    relation.columns.iter().map(|column| {
                        (column.name.as_ref(), column.type_oid, column.type_modifier)
                    }),
                );
            }
            state.unkeyed.push(relation.oid);
            state.relations.insert(relation.oid, relation);
        }
        Decoded::Insert { rel, new } => {
            let relation = lookup_relation(state, rel)?;
            let table = relation.table.clone();
            let key = pgoutput::row_key(relation, &new)?;
            state.open_txn.push(ChangeEvent::Upsert {
                table,
                key,
                before: None,
                after: None,
            });
        }
        Decoded::Update { rel, old, new } => {
            let relation = lookup_relation(state, rel)?;
            let table = relation.table.clone();
            let new_key = pgoutput::row_key(relation, &new)?;
            // A primary-key change is a delete of the old document plus an
            // upsert of the new one.
            let old_key = match &old {
                Some(old) => Some(pgoutput::row_key(relation, old)?),
                None => None,
            };
            let old_key = match old_key {
                Some(old_key) if !addressable(relation, &old_key) => {
                    warn_unkeyed(state, rel, Unkeyed::Update);
                    None
                }
                old_key => old_key,
            };
            let relation = lookup_relation(state, rel)?;
            let before = old
                .as_ref()
                .and_then(|old| pgoutput::pre_image(relation, old));
            let after = old
                .as_ref()
                .filter(|_| before.is_some() && relation.partitioned == Some(false))
                .map(|old| pgoutput::post_image(relation, old, &new));
            if let Some(old_key) = old_key
                && old_key.0 != new_key.0
            {
                state.open_txn.push(ChangeEvent::Delete {
                    table: table.clone(),
                    key: old_key,
                    before: before.clone(),
                });
            }
            state.open_txn.push(ChangeEvent::Upsert {
                table,
                key: new_key,
                before,
                after,
            });
            warn_untraceable(state, rel);
        }
        Decoded::Delete { rel, old } => {
            let relation = lookup_relation(state, rel)?;
            let table = relation.table.clone();
            let key = pgoutput::row_key(relation, &old)?;
            let before = pgoutput::pre_image(relation, &old);
            let unkeyed = match &before {
                _ if addressable(relation, &key) => None,
                Some(_) => Some(Unkeyed::DeleteTraced),
                None => Some(Unkeyed::DeleteUntraced),
            };
            state
                .open_txn
                .push(ChangeEvent::Delete { table, key, before });
            if let Some(unkeyed) = unkeyed {
                warn_unkeyed(state, rel, unkeyed);
            }
            warn_untraceable(state, rel);
        }
        Decoded::Truncate { rels } => {
            for oid in rels {
                let table = state
                    .relations
                    .get(&oid)
                    .map(|r| r.table.to_string())
                    .unwrap_or_else(|| format!("oid {oid}"));
                // No Truncate variant in the core vocabulary yet; surface it
                // loudly rather than silently leaving the index stale.
                tracing::warn!(%table, "TRUNCATE received but not propagated; index may be stale");
            }
        }
        Decoded::Other => {}
    }
    Ok(())
}

/// Record which pre-image columns relation `oid` needs but can't deliver —
/// what an update or delete on it can't trace to the old parent.
fn record_untraceable(state: &mut DecodeState, oid: u32) {
    let Some(relation) = state.relations.get(&oid) else {
        return;
    };
    let table = QualifiedTable::new(relation.namespace.clone(), relation.table.clone());
    let required = state
        .pre_image_links
        .get(&table)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let carries = |name: &str| {
        relation
            .columns
            .iter()
            .any(|column| column.is_key && column.name.as_ref() == name)
    };
    let uncarried = replica_identity::uncarried(carries, &relation.primary_key, required);
    if uncarried.is_empty() {
        state.untraceable.remove(&oid);
    } else {
        state.untraceable.insert(oid, uncarried);
    }
}

/// Warn, once per relation, about an update or delete on a table whose replica
/// identity doesn't carry its parent link — so a delete or re-parent leaves
/// the old parent's document stale.
fn warn_untraceable(state: &mut DecodeState, oid: u32) {
    let (Some(uncarried), Some(relation)) =
        (state.untraceable.get(&oid), state.relations.get(&oid))
    else {
        return;
    };
    if !state.warned.insert(oid) {
        return;
    }
    let table = QualifiedTable::new(relation.namespace.clone(), relation.table.clone());
    replica_identity::warn_untraceable(
        &table,
        uncarried,
        "an update or delete arrived without it",
        &replica_identity::alter_sql(relation.namespace.as_ref(), relation.table.as_ref()),
    );
}

/// Whether an old tuple's `key` can address a document. A table with a primary
/// key never holds a null in it, so a null there means the tuple left the key
/// out: a partition whose replica identity omits it, decoded under its
/// parent's relation. A table without one is keyed by its identity columns,
/// which may hold nulls under `FULL`.
fn addressable(relation: &Relation, key: &RowKey) -> bool {
    relation.primary_key.is_empty() || !key.has_null()
}

/// What an old tuple without its primary key costs, by kind of change. Each is
/// warned once per relation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Unkeyed {
    /// A delete whose old row was logged: emitted, so parents rebuild where it
    /// carries the link.
    DeleteTraced,
    /// A delete that logged neither: emitted, but it reaches nothing.
    DeleteUntraced,
    /// An update: its upsert stands, a key change's delete is dropped.
    Update,
}

impl Unkeyed {
    fn consequence(self) -> &'static str {
        match self {
            Self::DeleteTraced => {
                "a delete arrived without its primary key: a document keyed by the row can't be \
                 addressed and stays in the index; parents are rebuilt only where the old row carries \
                 the link"
            }
            Self::DeleteUntraced => {
                "a delete arrived without its primary key or its old row: neither the row's key \
                 nor its old row was logged, so a document keyed by the row stays in the index \
                 and no parent is rebuilt"
            }
            Self::Update => {
                "an update arrived without its old primary key: the row's document is rebuilt, \
                 but if the update changed the key, the old document stays until a \
                 `flusso reindex` of that index"
            }
        }
    }
}

/// Warn, once per relation and [`Unkeyed`] kind, that a change's old tuple
/// lacked the primary key.
fn warn_unkeyed(state: &mut DecodeState, oid: u32, unkeyed: Unkeyed) {
    let Some(relation) = state.relations.get(&oid) else {
        return;
    };
    if !state.warned_unkeyed.insert((oid, unkeyed)) {
        return;
    }
    tracing::warn!(
        table = %QualifiedTable::new(relation.namespace.clone(), relation.table.clone()),
        missing_key = %replica_identity::join_columns(&relation.primary_key),
        remediation = "set REPLICA IDENTITY FULL on each partition whose identity leaves out the \
                       primary key (`flusso check` names them)",
        "{}",
        unkeyed.consequence(),
    );
}

/// Look up a relation by OID. A missing one means a DML message arrived before
/// its `Relation` — a protocol violation we can't decode past.
fn lookup_relation(state: &DecodeState, oid: u32) -> std::result::Result<&Relation, SourceError> {
    state.relations.get(&oid).ok_or_else(|| {
        SourceError::Decode(format!("pgoutput: change for unknown relation oid {oid}"))
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;

/// Map a replication error onto the source's transient/fatal split.
fn map_pgwire(error: PgWireError) -> SourceError {
    if error.is_transient() {
        return SourceError::Connection(error.to_string());
    }
    match error {
        PgWireError::Server(_) | PgWireError::Auth(_) | PgWireError::Tls(_) => {
            SourceError::Setup(error.to_string())
        }
        PgWireError::Protocol(_) => SourceError::Decode(error.to_string()),
        PgWireError::MessageTooLarge {
            payload_bytes,
            limit_bytes,
        } => SourceError::Setup(format!(
            "the server sent a replication message of {payload_bytes} bytes, past the \
             {limit_bytes}-byte limit; the stream cannot read past it, so every restart stops \
             at the same message (almost certainly a corrupt stream, or a proxy mangling it)"
        )),
        other => SourceError::Connection(other.to_string()),
    }
}

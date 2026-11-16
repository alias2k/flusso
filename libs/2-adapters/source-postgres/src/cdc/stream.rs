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

use futures::FutureExt as _;
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

/// Since when the oldest committed transaction has waited to become visible,
/// shared with the capture, which reports it.
pub(crate) type HoldClock = Arc<Mutex<Option<Instant>>>;

/// How long the hold `clock` records has lasted; `None` when nothing waits.
pub(crate) fn held_for(clock: &HoldClock) -> Option<Duration> {
    clock
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .map(|since| since.elapsed())
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
}

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
}

/// Everything the unfold loop carries between polls.
struct State<C, V> {
    client: C,
    /// Catalog connection for the per-relation primary-key lookup.
    catalog: PgPool,
    visibility: V,
    decode: DecodeState,
    read_ahead: usize,
    /// The snapshot read in flight, on its own task: it runs while the engine
    /// builds and publishes, so a burst's transactions are checked without the
    /// stream ever making the engine wait for a round trip it could overlap.
    fetch: Option<Fetch>,
    /// When the next snapshot may be read: now, unless the oldest transaction
    /// was just found still invisible, then on the poll schedule.
    next_fetch: Instant,
    hold: Option<Hold>,
    hold_clock: HoldClock,
}

/// A snapshot read on its own task, aborted with the stream.
struct Fetch(tokio::task::JoinHandle<Result<super::visibility::Snapshot>>);

impl Drop for Fetch {
    fn drop(&mut self) {
        self.0.abort();
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
        let seq = self.ack.register(lsn);
        if self.pending.front().is_none_or(|next| next.lsn != lsn) {
            self.ack.read_through(lsn);
            self.readable.remove(&xid);
        }
        Some((Position(seq), event))
    }

    /// The xid of the front transaction while it may not be read back yet.
    fn held(&self) -> Option<u32> {
        self.pending
            .front()
            .map(|front| front.xid)
            .filter(|xid| !self.readable.contains(xid))
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
            read_tables: settings.read_tables,
            ack,
            done: false,
            column_types,
        },
        read_ahead: settings.read_ahead.max(1),
        fetch: None,
        next_fetch: Instant::now(),
        hold: None,
        hold_clock: settings.hold_clock,
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
            state
                .client
                .update_applied_lsn(Lsn::from_u64(state.decode.ack.confirmed_lsn()));

            if let Err(e) = state.collect() {
                return Some((Err(e), state));
            }
            if let Some(change) = state.decode.next_change() {
                if state
                    .hold
                    .is_some_and(|hold| state.decode.held() != Some(hold.xid))
                {
                    state.end_hold();
                }
                state.prefetch();
                return Some((Ok(change), state));
            }
            state.prefetch();

            let Some(xid) = state.decode.held() else {
                state.end_hold();
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
            // bound, and the snapshot read in flight, or the next one's time,
            // turns the loop.
            let room = !state.decode.done && state.decode.pending.len() < state.read_ahead;
            let fetching = state.fetch.is_some();
            let fetch = state.fetch.as_mut();
            tokio::select! {
                biased;
                () = state.decode.ack.confirmed_moved() => {}
                joined = async {
                    match fetch {
                        Some(Fetch(handle)) => Some((&mut *handle).await),
                        None => None,
                    }
                }, if fetching => {
                    state.fetch = None;
                    if let Some(joined) = joined
                        && let Err(e) = state.apply(joined)
                    {
                        state.decode.done = true;
                        return Some((Err(e), state));
                    }
                }
                received = state.client.recv(), if room => {
                    if let Err(e) = state.receive(received).await {
                        return Some((Err(e), state));
                    }
                }
                () = tokio::time::sleep_until(state.next_fetch), if !fetching => {}
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

    /// Start reading a snapshot when a transaction awaits a check, none is in
    /// flight, and the poll schedule allows.
    fn prefetch(&mut self) {
        if self.fetch.is_none()
            && !self.decode.unchecked.is_empty()
            && Instant::now() >= self.next_fetch
        {
            let visibility = self.visibility.clone();
            self.fetch = Some(Fetch(tokio::spawn(
                async move { visibility.snapshot().await },
            )));
        }
    }

    /// Take a finished snapshot read, if any, and [`apply`](Self::apply) it.
    fn collect(&mut self) -> std::result::Result<(), SourceError> {
        let Some(Fetch(handle)) = self.fetch.as_mut() else {
            return Ok(());
        };
        let Some(joined) = handle.now_or_never() else {
            return Ok(());
        };
        self.fetch = None;
        self.apply(joined)
    }

    /// Mark what a finished snapshot read sees. While the oldest transaction
    /// stays invisible, the next read waits on the poll schedule; otherwise
    /// it may start at once.
    fn apply(
        &mut self,
        joined: std::result::Result<Result<super::visibility::Snapshot>, tokio::task::JoinError>,
    ) -> std::result::Result<(), SourceError> {
        let snapshot =
            joined.map_err(|e| SourceError::Query(format!("reading a snapshot: {e}")))??;
        self.decode.seen_by(&snapshot);
        let now = Instant::now();
        self.next_fetch = now;
        if let Some(hold) = self.hold.as_mut()
            && self.decode.held() == Some(hold.xid)
        {
            hold.delay = poll_delay(now - hold.since, hold.delay);
            self.next_fetch = now + hold.delay;
        }
        Ok(())
    }

    /// The front transaction `xid` is held: start timing a new hold, and past
    /// [`STALL_WARNING`] report it.
    async fn holding(&mut self, xid: u32) -> std::result::Result<(), SourceError> {
        let now = Instant::now();
        let hold = match self.hold {
            Some(hold) if hold.xid == xid => hold,
            _ => {
                *self
                    .hold_clock
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) = Some(now);
                let hold = Hold {
                    xid,
                    since: now,
                    delay: Duration::ZERO,
                    warn_at: now + STALL_WARNING,
                };
                self.hold = Some(hold);
                hold
            }
        };
        if now >= hold.warn_at {
            self.report_stall(hold).await?;
        }
        Ok(())
    }

    /// A transaction held past [`STALL_WARNING`]: stop if the server now waits
    /// on flusso as a synchronous standby, else warn and name the sessions
    /// waiting on synchronous replication.
    async fn report_stall(&mut self, hold: Hold) -> std::result::Result<(), SourceError> {
        let stall = self.visibility.stall(hold.xid).await?;
        if let Some(refusal) = stall.refusal {
            return Err(refusal);
        }
        let waiting = if stall.waiting.is_empty() {
            "none this role can see (pg_read_all_stats shows every session)".to_owned()
        } else {
            stall.waiting.join("; ")
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

    fn end_hold(&mut self) {
        if self.hold.take().is_some() {
            *self
                .hold_clock
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = None;
        }
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
                if read {
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

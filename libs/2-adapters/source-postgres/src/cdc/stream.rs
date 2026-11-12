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
//! The loop also hands the confirmed position to the replication client: on
//! every turn, and the moment a confirmation moves it, without waiting for the
//! next replication event. The client reports it to the server within its
//! status interval, so the slot releases WAL seconds after the sinks land it.

use std::collections::{HashMap, HashSet, VecDeque};
use std::future::Future;
use std::sync::Arc;

use futures::stream::{self, BoxStream};
use kernel::ColumnName;
use kernel::Position;
use pgwire_replication::{Lsn, PgWireError, ReplicationClient, ReplicationEvent};
use source::cdc::{ChangeEvent, LiveChange};
use source::{PreImageColumns, QualifiedTable, Result, RowKey, SourceError};
use sqlx::PgPool;

use super::ack::Positions;
use super::pgoutput::{self, Decoded, Relation};
use super::replica_identity;

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
struct State<C> {
    client: C,
    /// Catalog connection for the per-relation primary-key lookup.
    catalog: PgPool,
    decode: DecodeState,
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
    /// Changes ready to emit, each with the commit LSN to acknowledge at.
    pending: VecDeque<(ChangeEvent, u64)>,
    ack: Arc<Positions>,
    /// Set once the underlying stream is finished; drains `pending`, then ends.
    done: bool,
}

impl DecodeState {
    /// Take the next change ready to emit and register its position. Emitting
    /// the last change of a commit also moves the horizon to that commit: from
    /// here on, everything read through it is registered.
    fn next_change(&mut self) -> Option<LiveChange> {
        let (event, lsn) = self.pending.pop_front()?;
        let seq = self.ack.register(lsn);
        if self.pending.front().is_none_or(|(_, next)| *next != lsn) {
            self.ack.read_through(lsn);
        }
        Some((Position(seq), event))
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
pub(crate) fn build<C: Replication>(
    client: C,
    catalog: PgPool,
    pre_image_links: PreImageColumns,
    ack: Arc<Positions>,
) -> BoxStream<'static, Result<LiveChange>> {
    let state = State {
        client,
        catalog,
        decode: DecodeState {
            relations: HashMap::new(),
            unkeyed: Vec::new(),
            pre_image_links,
            untraceable: HashMap::new(),
            warned: HashSet::new(),
            warned_unkeyed: HashSet::new(),
            open_txn: Vec::new(),
            pending: VecDeque::new(),
            ack,
            done: false,
        },
    };

    Box::pin(stream::unfold(state, |mut state| async move {
        loop {
            // Report how far the engine has durably confirmed so the slot can
            // advance and the server can recycle WAL. Every turn, so the
            // horizon moving on idle progress is reported too.
            state
                .client
                .update_applied_lsn(Lsn::from_u64(state.decode.ack.confirmed_lsn()));

            if let Some(change) = state.decode.next_change() {
                return Some((Ok(change), state));
            }

            if state.decode.done {
                return None;
            }

            // A confirmation moving the resume point turns the loop at once,
            // so the client hears it now rather than at the next event (a
            // keepalive can be 30 s away on a quiet stream). Dropping `recv`
            // loses nothing while the worker runs: it is a channel receive.
            // The one unsafe moment is after the channel closed, when `recv`
            // awaits the worker's `JoinHandle`: if a wake-up wins then, the
            // handle is dropped, and the next turn reports a generic
            // "replication worker already joined" error instead of the
            // worker's own outcome. The engine restarts the stream either way.
            let received = tokio::select! {
                biased;
                received = state.client.recv() => received,
                () = state.decode.ack.confirmed_moved() => continue,
            };
            match received {
                Ok(Some(event)) => {
                    let handled = match handle(&mut state.decode, event) {
                        Ok(()) => key_relations(&state.catalog, &mut state.decode).await,
                        Err(e) => Err(e),
                    };
                    if let Err(e) = handled {
                        state.decode.done = true;
                        return Some((Err(e), state));
                    }
                }
                Ok(None) => state.decode.done = true,
                Err(e) => {
                    state.decode.done = true;
                    return Some((Err(map_pgwire(e)), state));
                }
            }
        }
    }))
}

/// Look up the catalog primary key of every relation announced since the last
/// event, then which parent links its identity can't deliver (that depends on
/// the key). A table without one keeps an empty key and is keyed by its
/// replica identity instead.
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
        relation.partitioned = Some(is_partitioned(catalog, &qualified).await?);
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

/// Whether `qualified` names a partitioned table.
async fn is_partitioned(catalog: &PgPool, qualified: &str) -> Result<bool> {
    sqlx::query_scalar::<_, bool>("SELECT relkind = 'p' FROM pg_class WHERE oid = $1::regclass")
        .bind(qualified)
        .fetch_one(catalog)
        .await
        .map_err(|e| SourceError::Query(e.to_string()))
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

        ReplicationEvent::Begin { .. } => state.open_txn.clear(),

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
                for change in state.open_txn.drain(..) {
                    state.pending.push_back((change, lsn));
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
    /// A delete whose old row was logged: emitted, so parents still rebuild.
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
                 addressed and stays in the index; parents are still rebuilt from the old row"
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

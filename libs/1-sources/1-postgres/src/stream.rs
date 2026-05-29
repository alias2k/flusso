//! The state machine that turns a `pgwire-replication` event stream into a
//! stream of thin [`Change`]s.
//!
//! Row changes are buffered per transaction and only emitted once the `Commit`
//! arrives, tagged with the commit LSN. That gives every change a clean,
//! commit-aligned position to acknowledge against, and matches logical
//! decoding's "whole transactions only" model. Because events are thin (a table
//! name and primary key), buffering even a large transaction is cheap.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use futures::stream::{self, BoxStream};
use pgwire_replication::{Lsn, ReplicationClient, ReplicationEvent};
use sources_core::{Ack, AckSink, Change, ChangeEvent, Result, SourceError};

use crate::ack::{AckShared, WalAckSink};
use crate::pgoutput::{self, Decoded, Relation};

/// Everything the unfold loop carries between polls.
struct State {
    client: ReplicationClient,
    /// Relation metadata by OID, accumulated from `Relation` messages.
    relations: HashMap<u32, Relation>,
    /// Row changes of the currently open transaction, awaiting its `Commit`.
    open_txn: Vec<ChangeEvent>,
    /// Changes ready to emit, each with the commit LSN to acknowledge at.
    pending: VecDeque<(ChangeEvent, u64)>,
    ack: Arc<AckShared>,
    sink: Arc<dyn AckSink>,
    /// Set once the underlying stream is finished; drains `pending`, then ends.
    done: bool,
}

/// Build the [`Change`] stream from a connected client and its starting LSN.
pub(crate) fn build(client: ReplicationClient, start_lsn: Lsn) -> BoxStream<'static, Result<Change>> {
    let ack = Arc::new(AckShared::new(start_lsn.as_u64()));
    let sink: Arc<dyn AckSink> = Arc::new(WalAckSink::new(Arc::clone(&ack)));

    // The backfill phase is currently empty: pgwire-replication is a pure
    // consumer, so an initial consistent snapshot needs a separate SQL query
    // client we haven't wired yet. Emitting SnapshotComplete first keeps the
    // contract shape — backfill (none), then live changes.
    let mut pending = VecDeque::new();
    pending.push_back((ChangeEvent::SnapshotComplete, start_lsn.as_u64()));

    let state = State {
        client,
        relations: HashMap::new(),
        open_txn: Vec::new(),
        pending,
        ack,
        sink,
        done: false,
    };

    Box::pin(stream::unfold(state, |mut state| async move {
        loop {
            // Report how far the engine has durably confirmed so the slot can
            // advance and the server can recycle WAL.
            state
                .client
                .update_applied_lsn(Lsn::from_u64(state.ack.confirmed_lsn()));

            if let Some((event, lsn)) = state.pending.pop_front() {
                let seq = state.ack.register(lsn);
                let ack = Ack::new(seq, Arc::clone(&state.sink));
                return Some((Ok(Change { event, ack }), state));
            }

            if state.done {
                return None;
            }

            match state.client.recv().await {
                Ok(Some(event)) => {
                    if let Err(e) = handle(&mut state, event) {
                        state.done = true;
                        return Some((Err(e), state));
                    }
                }
                Ok(None) => state.done = true,
                Err(e) => {
                    state.done = true;
                    return Some((Err(map_pgwire(e)), state));
                }
            }
        }
    }))
}

/// Fold one replication event into the state, possibly queueing changes.
fn handle(state: &mut State, event: ReplicationEvent) -> std::result::Result<(), SourceError> {
    match event {
        // Worker handles keepalive feedback; logical messages are not changes.
        ReplicationEvent::KeepAlive { .. } | ReplicationEvent::Message { .. } => {}

        // A fresh transaction: nothing buffered should remain from a prior one.
        ReplicationEvent::Begin { .. } => state.open_txn.clear(),

        // Commit closes the transaction: release its changes at the commit LSN.
        ReplicationEvent::Commit { end_lsn, .. } => {
            let lsn = end_lsn.as_u64();
            for change in state.open_txn.drain(..) {
                state.pending.push_back((change, lsn));
            }
        }

        // Stop requested / stop_at_lsn reached: drain what we have, then end.
        ReplicationEvent::StoppedAt { .. } => state.done = true,

        ReplicationEvent::XLogData { data, .. } => handle_xlog(state, data.as_ref())?,
    }
    Ok(())
}

fn handle_xlog(state: &mut State, data: &[u8]) -> std::result::Result<(), SourceError> {
    match pgoutput::decode(data)? {
        Decoded::Relation(relation) => {
            state.relations.insert(relation.oid, relation);
        }
        Decoded::Insert { rel, new } => {
            let relation = lookup_relation(state, rel)?;
            let table = relation.table.clone();
            let key = pgoutput::row_key(relation, &new)?;
            state.open_txn.push(ChangeEvent::Upsert { table, key });
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
            if let Some(old_key) = old_key
                && old_key.0 != new_key.0
            {
                state.open_txn.push(ChangeEvent::Delete {
                    table: table.clone(),
                    key: old_key,
                });
            }
            state.open_txn.push(ChangeEvent::Upsert {
                table,
                key: new_key,
            });
        }
        Decoded::Delete { rel, old } => {
            let relation = lookup_relation(state, rel)?;
            let table = relation.table.clone();
            let key = pgoutput::row_key(relation, &old)?;
            state.open_txn.push(ChangeEvent::Delete { table, key });
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

/// Look up a relation by OID. A missing one means a DML message arrived before
/// its `Relation` — a protocol violation we can't decode past.
fn lookup_relation(state: &State, oid: u32) -> std::result::Result<&Relation, SourceError> {
    state.relations.get(&oid).ok_or_else(|| {
        SourceError::Decode(format!("pgoutput: change for unknown relation oid {oid}"))
    })
}

/// Map a replication error onto the source's transient/fatal split.
fn map_pgwire(error: pgwire_replication::PgWireError) -> SourceError {
    use pgwire_replication::PgWireError;
    if error.is_transient() {
        return SourceError::Connection(error.to_string());
    }
    match error {
        PgWireError::Server(_) | PgWireError::Auth(_) | PgWireError::Tls(_) => {
            SourceError::Setup(error.to_string())
        }
        PgWireError::Protocol(_) => SourceError::Decode(error.to_string()),
        other => SourceError::Connection(other.to_string()),
    }
}

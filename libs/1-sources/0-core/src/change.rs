use schema_core::{ColumnName, GenericValue, TableName};

use crate::Ack;

/// One item emitted by a [`ChangeCapture`](crate::ChangeCapture) stream: a
/// change paired with the [`Ack`] that confirms it was durably processed.
///
/// Dropping the `ack` without calling [`Ack::confirm`] leaves the change
/// unconfirmed — the mechanism will redeliver it after a restart. That is what
/// makes delivery at-least-once: an event is only forgotten once the engine
/// says it landed downstream.
#[derive(Debug)]
pub struct Change {
    pub event: ChangeEvent,
    pub ack: Ack,
}

/// What happened to a row, identified only by its table and primary key.
///
/// Events are deliberately *thin*: they name the row, not its contents. The
/// engine re-reads the current row — and resolves the document's joins and
/// aggregates — at assembly time. This keeps every mechanism (WAL, polling, …)
/// identical from the engine's point of view and avoids depending on a table's
/// `REPLICA IDENTITY` to carry old or new values.
///
/// Note that the mechanism reports *raw per-table* changes. Mapping a change in
/// a joined or junction table back to the parent documents that must be rebuilt
/// is the engine's job, driven by the schema — not something the source knows.
#[derive(Debug, Clone)]
pub enum ChangeEvent {
    /// A row seen during the initial backfill at the head of the stream.
    ///
    /// Identical in effect to [`Upsert`](Self::Upsert), but kept distinct so
    /// the engine can recognise backfill — for example to defer checkpointing
    /// or batch differently until the snapshot completes.
    Snapshot { table: TableName, key: RowKey },

    /// The backfill is finished. Every change after this is a live change.
    SnapshotComplete,

    /// A row was inserted or updated.
    Upsert { table: TableName, key: RowKey },

    /// A row was deleted.
    Delete { table: TableName, key: RowKey },
}

/// The primary key of a changed row.
///
/// A list of column/value pairs so composite keys are represented naturally;
/// values reuse [`GenericValue`] from the schema model.
#[derive(Debug, Clone)]
pub struct RowKey(pub Vec<(ColumnName, GenericValue)>);

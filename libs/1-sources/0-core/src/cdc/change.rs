use schema_core::TableName;

use crate::RowKey;

use super::Ack;

/// One item emitted by a [`ChangeCapture`](super::ChangeCapture) stream: a
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
/// is the document layer's job — not something this layer knows.
///
/// There is no distinct "snapshot" variant: an initial backfill is a separate
/// finite stream of [`Upsert`](Self::Upsert)s (see
/// [`ChangeCapture::snapshot`](super::ChangeCapture::snapshot)), so the engine
/// knows it is seeding from *which stream* it is draining, not from the event.
#[derive(Debug, Clone)]
pub enum ChangeEvent {
    /// A row was inserted or updated.
    Upsert { table: TableName, key: RowKey },

    /// A row was deleted.
    Delete { table: TableName, key: RowKey },
}

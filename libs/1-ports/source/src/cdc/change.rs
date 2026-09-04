use kernel::TableName;

use crate::RowKey;

/// What happened to a row, identified only by its table and primary key.
///
/// Events are deliberately *thin*: they name the row, not its contents. The
/// ingest engine re-reads the current row — and resolves the document's joins
/// and aggregates — at build time. This keeps every mechanism (WAL, polling, …)
/// identical from the engine's point of view and avoids depending on a table's
/// `REPLICA IDENTITY` to carry old or new values.
///
/// The mechanism reports *raw per-table* changes. Mapping a change in a joined
/// or junction table back to the parent documents that must be rebuilt is the
/// document layer's job — not something this layer knows.
///
/// A live event travels with the [`Position`](kernel::Position) the source
/// assigned it (see [`ChangeCapture::live`](super::ChangeCapture::live)); a
/// snapshot row has none: an initial backfill is a separate finite stream of
/// [`Upsert`](Self::Upsert)s (see [`ChangeCapture::snapshot`](super::ChangeCapture::snapshot)),
/// and a crashed backfill simply re-runs, idempotently.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeEvent {
    /// A row was inserted or updated.
    Upsert { table: TableName, key: RowKey },

    /// A row was deleted.
    Delete { table: TableName, key: RowKey },
}

impl ChangeEvent {
    /// The table the change is in.
    pub fn table(&self) -> &TableName {
        match self {
            ChangeEvent::Upsert { table, .. } | ChangeEvent::Delete { table, .. } => table,
        }
    }

    /// The row's primary key.
    pub fn key(&self) -> &RowKey {
        match self {
            ChangeEvent::Upsert { key, .. } | ChangeEvent::Delete { key, .. } => key,
        }
    }
}

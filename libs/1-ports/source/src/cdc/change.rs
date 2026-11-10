use kernel::TableName;

use crate::{RowImage, RowKey};

/// What happened to a row, identified by its table and primary key.
///
/// Events are deliberately *thin*: they name the row, not its contents. The
/// ingest engine re-reads the current row — and resolves the document's joins
/// and aggregates — at build time. This keeps every mechanism (WAL, polling, …)
/// identical from the engine's point of view.
///
/// The exceptions are the optional [`RowImage`]s. Neither builds a document:
///
/// - `before`, the pre-image: resolution reads it to find the documents that
///   embedded the row's *old* version — a deleted or re-parented child whose
///   link to its parent no longer exists in the source.
/// - `after`, an update's post-image: compared with `before`, it tells
///   resolution which columns the update changed, so a path that uses none of
///   them is skipped. Without both images every path is resolved.
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
    Upsert {
        table: TableName,
        key: RowKey,
        /// The row before an update, when the change feed carried it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before: Option<RowImage>,
        /// The row after an update, when the change feed carried it beside a
        /// `before`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after: Option<RowImage>,
    },

    /// A row was deleted.
    Delete {
        table: TableName,
        key: RowKey,
        /// The deleted row, when the change feed carried more than its key.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before: Option<RowImage>,
    },
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

    /// The row's pre-image, when the change feed carried one.
    pub fn before(&self) -> Option<&RowImage> {
        match self {
            ChangeEvent::Upsert { before, .. } | ChangeEvent::Delete { before, .. } => {
                before.as_ref()
            }
        }
    }

    /// The row's post-image: an update's new column values, when the change
    /// feed carried them. Always `None` for a delete.
    pub fn after(&self) -> Option<&RowImage> {
        match self {
            ChangeEvent::Upsert { after, .. } => after.as_ref(),
            ChangeEvent::Delete { .. } => None,
        }
    }
}

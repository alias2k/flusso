use schema_core::{DatabaseSchema, TableName};

/// A schema-qualified table to snapshot during an initial backfill.
///
/// A mechanism-neutral primitive (like [`RowKey`](crate::RowKey)): the engine
/// computes which tables seed which indexes and hands the set to a
/// [`ChangeCapture::snapshot`](crate::cdc::ChangeCapture::snapshot), which reads
/// their current rows. It belongs to neither the capture nor the document
/// concern — both refer to it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SnapshotTable {
    pub db_schema: DatabaseSchema,
    pub table: TableName,
}

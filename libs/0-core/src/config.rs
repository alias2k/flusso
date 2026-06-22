mod aggregate;
mod content_hash;
mod field;
mod filter;
mod flusso_type;
mod index_mapping;
mod join;
mod projection;
mod schema;
mod secret;
mod sink;
mod soft_delete;
mod transform;

pub use aggregate::*;
pub use content_hash::*;
pub use field::*;
pub use filter::*;
pub use flusso_type::*;
pub use index_mapping::*;
pub use join::*;
pub use schema::*;
pub use secret::*;
pub use sink::*;
pub use soft_delete::*;
pub use transform::*;

use serde::{Deserialize, Serialize};

use crate::common;

/// What the pipeline does when a sink **rejects a document at the item level** —
/// it accepted the batch but refused a specific document (a mapping conflict, a
/// malformed value). Distinct from a flush-wide failure, which always stops the
/// run. Set globally on the config and overridable per index (both live in the
/// `schema` crate's `Config`/`Index`, which assemble this policy).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailurePolicy {
    /// Stop the run on the first rejected document. The batch is left
    /// unconfirmed and redelivered on restart, so a persistently-bad document
    /// halts sync until the data is fixed or the policy is changed. The default,
    /// because dropping data should be opt-in.
    #[default]
    Stop,
    /// Quarantine each rejected document (surfaced via metrics/status and logs)
    /// and continue: the rest of the batch is applied, the slot advances, and
    /// the poison is not redelivered — it simply never lands until its source
    /// row changes again.
    Skip,
}

/// The shape of a single search document: a root table and the fields built
/// from its columns and related tables.
#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
pub struct IndexSchema {
    pub version: u8,
    pub table: common::TableName,
    pub db_schema: DatabaseSchema,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_key: Option<common::ColumnName>,
    /// Reserved: the column whose value would become the document `_id`.
    /// Not honored yet — the schema layer rejects a set `doc_id`, so this is
    /// always `None`; the `_id` is derived from [`primary_key`](Self::primary_key).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc_id: Option<common::ColumnName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soft_delete: Option<SoftDelete>,
    /// Root filters: only root rows matching every filter become documents.
    /// A row that stops matching emits a tombstone, exactly like
    /// [`soft_delete`](Self::soft_delete) — both fold into the document
    /// query's `WHERE`, so "no row came back" means "this document should not
    /// exist".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<Vec<Filter>>,
    pub fields: Vec<Field>,
}

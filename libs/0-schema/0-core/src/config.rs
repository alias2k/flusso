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
mod source;
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
pub use source::*;
pub use transform::*;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::common;

/// What the pipeline does when a sink **rejects a document at the item level** —
/// it accepted the batch but refused a specific document (a mapping conflict, a
/// malformed value). Distinct from a flush-wide failure, which always stops the
/// run. Set globally with [`Config::on_error`] and overridable per index with
/// [`Index::on_error`].
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

/// A whole deployment: where data comes from, where it goes, and what to build.
///
/// Secrets are deferred (a literal or an environment reference, see [`Secret`]),
/// so a serialized `Config` carries only the literals it was given and resolves
/// the rest at runtime. Debug output redacts literal secrets either way.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub source: Source,
    pub sinks: BTreeMap<common::SinkName, Sink>,
    pub indexes: BTreeMap<common::IndexName, Index>,
    /// What to do when a sink rejects a document at the item level. The default
    /// for every index; override per index with [`Index::on_error`].
    #[serde(default)]
    pub on_error: FailurePolicy,
}

/// One index in a [`Config`], paired with whether it is built on this run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Index {
    pub enabled: bool,
    pub schema: IndexSchema,
    /// Per-index override of [`Config::on_error`]. `None` inherits the global
    /// policy. Lives here (not in [`IndexSchema`]) on purpose: it's operational,
    /// not part of the document shape, so changing it does not alter the index
    /// mapping hash or trigger a reindex.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_error: Option<FailurePolicy>,
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

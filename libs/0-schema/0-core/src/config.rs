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
}

/// One index in a [`Config`], paired with whether it is built on this run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Index {
    pub enabled: bool,
    pub schema: IndexSchema,
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
    pub fields: Vec<Field>,
}

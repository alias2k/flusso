mod aggregate;
mod content_hash;
mod field;
mod filter;
mod join;
mod schema;
mod sink;
mod soft_delete;
mod source;
mod transform;

pub use aggregate::*;
pub use content_hash::*;
pub use field::*;
pub use filter::*;
pub use join::*;
pub use schema::*;
pub use sink::*;
pub use soft_delete::*;
pub use source::*;
pub use transform::*;

use std::collections::BTreeMap;

use crate::common;

#[derive(Debug, Clone)]
pub struct Config {
    pub source: Source,
    pub sinks: BTreeMap<common::SinkName, Sink>,
    pub indexes: Vec<Index>,
}

#[derive(Debug, Clone)]
pub struct Index {
    pub name: common::IndexName,
    pub enabled: bool,
    pub schema: IndexSchema,
}

#[derive(Debug, Clone, Hash)]
pub struct IndexSchema {
    pub version: u8,
    pub table: common::TableName,
    pub db_schema: DatabaseSchema,
    pub primary_key: Option<common::ColumnName>,
    pub doc_id: Option<common::ColumnName>,
    pub soft_delete: Option<SoftDelete>,
    pub fields: Vec<Field>,
}

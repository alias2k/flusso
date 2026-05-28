mod aggregate;
mod field;
mod filter;
mod join;
mod sink;
mod soft_delete;
mod source;
mod transform;

use aggregate::*;
use field::*;
use filter::*;
use join::*;
use sink::*;
use soft_delete::*;
use source::*;
use transform::*;

use std::collections::HashMap;

use crate::common;

#[derive(Debug, Clone)]
pub struct Config {
    pub source: Source,
    pub sinks: HashMap<common::SinkName, Sink>,
    pub indexes: Vec<Index>,
}

#[derive(Debug, Clone)]
pub struct Index {
    pub name: common::IndexName,
    pub enabled: bool,
    pub schema: IndexSchema,
}

#[derive(Debug, Clone)]
pub struct IndexSchema {
    pub version: u8,
    pub table: String,
    pub db_schema: String,
    pub primary_key: Option<String>,
    pub doc_id: Option<String>,
    pub soft_delete: Option<SoftDelete>,
    pub fields: Vec<Field>,
}

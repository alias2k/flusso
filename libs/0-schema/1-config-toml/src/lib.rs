mod entities;
mod parser;

#[allow(unused_imports)]
pub use parser::*;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use entities::IndexEntry;
use entities::Sink;
use entities::Source;
use schema_core::common;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigToml {
    pub source: Source,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sinks: Option<HashMap<common::SinkName, Sink>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<Vec<IndexEntry>>,
}

impl From<ConfigToml> for schema_core::Config {
    fn from(_value: ConfigToml) -> Self {
        todo!()
    }
}

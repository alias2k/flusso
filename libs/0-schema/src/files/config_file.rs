mod entities;
mod parser;

pub use entities::*;
#[allow(unused_imports)]
pub use parser::*;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::common;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigFile {
    pub source: Source,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sinks: Option<HashMap<common::SinkName, Sink>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<Vec<IndexEntry>>,
}

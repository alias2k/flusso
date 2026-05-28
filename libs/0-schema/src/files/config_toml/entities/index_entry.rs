use serde::{Deserialize, Serialize};

use crate::common;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexEntry {
    pub name: common::IndexName,
    pub schema: common::SchemaPath,
    pub enabled: bool,
}

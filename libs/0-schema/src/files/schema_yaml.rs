mod entities;
mod parser;

pub use entities::*;
#[allow(unused_imports)]
pub use parser::ParseError;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemaYaml {
    pub version: u8,
    pub table: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub soft_delete: Option<SoftDelete>,
    pub fields: Vec<Field>,
}

impl From<SchemaYaml> for crate::config::IndexSchema {
    fn from(_value: SchemaYaml) -> Self {
        todo!()
    }
}

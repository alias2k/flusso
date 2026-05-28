mod conversion;
mod entities;
mod parser;

pub use entities::*;
pub use parser::ParseError;

use serde::{Deserialize, Serialize};

pub const SUPPORTED_VERSIONS: &[u8] = &[1];

#[derive(thiserror::Error, Debug)]
pub enum ConversionError {
    #[error("invalid table name: {0}")]
    TableName(#[from] schema_core::TableNameError),
    #[error("invalid column name: {0}")]
    ColumnName(#[from] schema_core::ColumnNameError),
    #[error("invalid database schema name: {0}")]
    DatabaseSchema(#[from] schema_core::DatabaseSchemaError),
    #[error("join must specify either `foreign_key` or `through`, not both or neither")]
    InvalidJoinKey,
    #[error("aggregate op '{op}' requires a `column`")]
    MissingAggregateColumn { op: &'static str },
    #[error("filter op '{op}' requires a value")]
    MissingFilterValue { op: &'static str },
    #[error("filter op 'between' requires exactly 2 values, got {got}")]
    InvalidBetweenArity { got: usize },
    #[error("filter op '{op}' requires a sequence value")]
    ExpectedListValue { op: &'static str },
    #[error("a field cannot have both `join` and `aggregate`")]
    ConflictingRelation,
}

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

impl TryFrom<SchemaYaml> for schema_core::IndexSchema {
    type Error = ConversionError;

    fn try_from(yaml: SchemaYaml) -> Result<Self, Self::Error> {
        use schema_core::common::{ColumnName, TableName};

        let table = TableName::try_new(yaml.table)?;
        let db_schema = match yaml.schema {
            Some(s) => schema_core::DatabaseSchema::try_new(s)?,
            None => schema_core::DatabaseSchema::default(),
        };
        let primary_key = yaml.primary_key
            .map(ColumnName::try_new)
            .transpose()?;
        let doc_id = yaml.doc_id
            .map(ColumnName::try_new)
            .transpose()?;
        let soft_delete = yaml.soft_delete
            .map(conversion::convert_soft_delete)
            .transpose()?;
        let fields = yaml.fields
            .into_iter()
            .map(conversion::convert_field)
            .collect::<Result<_, _>>()?;

        Ok(schema_core::IndexSchema {
            version: yaml.version,
            table,
            db_schema,
            primary_key,
            doc_id,
            soft_delete,
            fields,
        })
    }
}

//! Parse a `*.schema.yml` index definition into the core
//! [`IndexSchema`](schema_core::IndexSchema).
//!
//! A schema file describes one search document: its root table, its fields, and
//! how related tables fold in through joins and aggregates. Each field is
//! written **type-first** — `- <type>: <name>` (`keyword: email`,
//! `one_to_many: orders`, `count: orderCount`, `geo: location`) — and carries
//! only the siblings that type allows. Parsing is two stages:
//!
//! 1. [`SchemaYaml`] deserializes the file. Each field's type tag selects the
//!    body shape it parses into (see [`Field`]).
//!    [`ParseFrom`](schema_core::ParseFrom) also checks the declared `version`
//!    against [`SUPPORTED_VERSIONS`].
//! 2. `TryFrom<SchemaYaml>` converts it into the core model, validating
//!    identifiers and the arity rules YAML alone can't express: a join needs
//!    exactly one of `foreign_key` or `through`, `sum`/`min`/`max` aggregates
//!    need a `column` and a `value_type`, a `between` filter takes exactly two
//!    values, and a `geo` field needs either `lat`+`lon` or a single `column`.

mod conversion;
mod entities;
mod parser;

pub use entities::*;
pub use parser::ParseError;

use serde::Deserialize;

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
    #[error("aggregate op '{op}' requires a `value_type` (its result mirrors the column)")]
    MissingAggregateType { op: &'static str },
    #[error(
        "aggregate op '{op}' `value_type` must be a scalar type — `geo_point` and `custom` \
         are not valid aggregate result types"
    )]
    InvalidAggregateType { op: &'static str },
    #[error(
        "a `geo` field needs either both `lat` and `lon` (two columns) or a single `column` \
         holding a combined value — not a mix"
    )]
    InvalidGeoSource,
}

#[derive(Debug, Clone, Deserialize)]
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
        let primary_key = yaml.primary_key.map(ColumnName::try_new).transpose()?;
        let doc_id = yaml.doc_id.map(ColumnName::try_new).transpose()?;
        let soft_delete = yaml
            .soft_delete
            .map(conversion::convert_soft_delete)
            .transpose()?;
        let fields = yaml
            .fields
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

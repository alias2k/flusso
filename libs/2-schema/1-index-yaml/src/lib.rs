#![doc = include_str!("../README.md")]

mod conversion;
mod entities;
mod parser;

pub use entities::*;
pub use parser::ParseError;

use serde::Deserialize;

pub const SUPPORTED_VERSIONS: &[u8] = &[1];

/// The JSON Schema (authored as YAML) describing a `*.schema.yml` index file,
/// embedded from this crate's `schemas/` directory for editor assist and
/// programmatic access (both re-exported from `schema` and emitted by `flusso
/// schema index`). Kept in lockstep with this parser by `schema`'s `schema_drift`
/// test.
pub const INDEX_SCHEMA: &str = include_str!("../index.schema.yml");

#[derive(thiserror::Error, Debug)]
pub enum ConversionError {
    #[error("invalid table name: {0}")]
    TableName(#[from] schema_core::TableNameError),
    #[error("invalid column name: {0}")]
    ColumnName(#[from] schema_core::ColumnNameError),
    #[error("invalid database schema name: {0}")]
    DatabaseSchema(#[from] schema_core::DatabaseSchemaError),
    #[error("`{verb}` join is missing its key: it takes {expected}")]
    MissingJoinKey {
        verb: &'static str,
        expected: &'static str,
    },
    #[error("`{verb}` join does not take `{sibling}`; it takes {expected}")]
    UnexpectedJoinKey {
        verb: &'static str,
        sibling: &'static str,
        expected: &'static str,
    },
    #[error("`{verb}` join does not take `{sibling}` (a to-one join picks a single row)")]
    UnexpectedJoinSibling {
        verb: &'static str,
        sibling: &'static str,
    },
    #[error("aggregate must specify either `foreign_key` or `through`, not both or neither")]
    InvalidAggregateKey,
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
        "aggregate op 'ids' requires an `element_type` (`long` or `keyword`) — it states the \
         element type of the collected primary keys"
    )]
    MissingElementType,
    #[error(
        "aggregate op 'ids' `element_type` must be a scalar type — `geo_point` and `custom` \
         are not valid element types"
    )]
    InvalidElementType,
    #[error(
        "aggregate op 'ids' does not take `{sibling}` (it always collects the related table's primary key)"
    )]
    UnexpectedIdsSibling { sibling: &'static str },
    #[error("aggregate does not take `{sibling}` (only `ids` does)")]
    UnexpectedAggregateSibling { sibling: &'static str },
    #[error(
        "a `geo` field needs either both `lat` and `lon` (two columns) or a single `column` \
         holding a combined value — not a mix"
    )]
    InvalidGeoSource,
    #[error(
        "a `map` field's `values` must be a leaf type — `text`/`keyword` or a number/date kind \
         (`{got}` is not one); `boolean`, `binary`, `json`, `geo`, and `custom` are not valid \
         map value types"
    )]
    InvalidMapValueType { got: &'static str },
    #[error(
        "`doc_id` is not supported yet — the document `_id` is always derived from `primary_key`. \
         Remove `doc_id` from the schema."
    )]
    DocIdUnsupported,
    #[error(
        "a `default` must be a scalar value (string, number, bool, or date) — a `{got}` default \
         is not supported"
    )]
    NonScalarDefault { got: &'static str },
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
    /// Root filters: only matching root rows become documents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<Vec<Filter>>,
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
        // `doc_id` parses (so existing schemas still deserialize) but is rejected
        // here: honoring a non-pk `_id` needs the value at delete time, which the
        // pk-keyed tombstone path can't supply. Tracked as a follow-up feature.
        if yaml.doc_id.is_some() {
            return Err(ConversionError::DocIdUnsupported);
        }
        let doc_id = yaml.doc_id.map(ColumnName::try_new).transpose()?;
        let soft_delete = yaml
            .soft_delete
            .map(conversion::convert_soft_delete)
            .transpose()?;
        let filters = conversion::convert_filters_opt(yaml.filters)?;
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
            filters,
            fields,
        })
    }
}

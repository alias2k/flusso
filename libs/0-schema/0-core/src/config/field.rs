use std::collections::BTreeMap;

use crate::common;

use super::{Aggregate, Join, Transform};

/// One field of a document. In its simplest form it maps to a column; it can
/// also pull in a related table ([`relation`](Self::relation)), carry nested
/// [`fields`](Self::fields), apply [`transforms`](Self::transforms), and
/// override the inferred OpenSearch mapping.
#[derive(Debug, Clone, Hash)]
pub struct Field {
    pub field: common::FieldName,
    pub column: Option<common::ColumnName>,
    pub mapping: Option<Mapping>,
    pub relation: Option<FieldRelation>,
    pub transforms: Option<Vec<Transform>>,
    pub default: Option<common::GenericValue>,
    pub fields: Option<Vec<Field>>,
}

/// How a field draws on a related table: either folding its rows in as nested
/// documents ([`Join`]) or reducing them to a single value ([`Aggregate`]).
#[derive(Debug, Clone, Hash)]
pub enum FieldRelation {
    Join(Join),
    Aggregate(Aggregate),
}

/// OpenSearch mapping. `mapping_type` is required; all other properties are passed through as-is.
#[derive(Debug, Clone, Hash)]
pub struct Mapping {
    pub mapping_type: MappingType,
    pub extra: BTreeMap<String, common::GenericValue>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum MappingType {
    Text,
    Keyword,
    Boolean,
    Byte,
    Short,
    Integer,
    Long,
    Float,
    Double,
    HalfFloat,
    ScaledFloat,
    Date,
    Object,
    Nested,
    /// Any mapping type not covered above.
    Other(String),
}

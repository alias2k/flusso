use std::collections::BTreeMap;

use crate::common;

use super::{Aggregate, Filter, Join, JoinKey, Transform};

/// One field of a document: a name, an optional OpenSearch mapping override, and
/// a [`source`](Self::source) saying where its value comes from.
#[derive(Debug, Clone, Hash)]
pub struct Field {
    pub field: common::FieldName,
    pub mapping: Option<Mapping>,
    pub source: FieldSource,
}

/// Where a field's value comes from. The shapes are mutually exclusive — a field
/// is exactly one of them — which is why this is an enum rather than a bag of
/// optional `column` / `relation` / `fields` that can contradict each other.
#[derive(Debug, Clone, Hash)]
pub enum FieldSource {
    /// A column of the current row, optionally transformed, with an optional
    /// fallback when the column is null.
    Column(Column),
    /// A sub-object assembled from sibling fields of the *same* row (it adds a
    /// nesting level in the document without reading a related table).
    Group(Vec<Field>),
    /// Data drawn from a related table — folded in as nested documents
    /// ([`Join`](Relation::Join)) or reduced to a single value
    /// ([`Aggregate`](Relation::Aggregate)).
    Relation(Relation),
    /// A constant value with no database source — `None` renders as null.
    Constant(common::GenericValue),
}

/// A column-backed field: the column to read, the transforms to apply to it, and
/// a default to coalesce nulls to.
#[derive(Debug, Clone, Hash)]
pub struct Column {
    pub column: common::ColumnName,
    pub transforms: Vec<Transform>,
    pub default: Option<common::GenericValue>,
}

/// How a field draws on a related table: either folding its rows in as nested
/// documents ([`Join`](Self::Join)) or reducing them to a single value
/// ([`Aggregate`](Self::Aggregate)).
#[derive(Debug, Clone, Hash)]
pub enum Relation {
    /// Fold the related rows in as nested documents, projecting `fields` from
    /// each one.
    Join { join: Join, fields: Vec<Field> },
    /// Reduce the related rows to a single scalar.
    Aggregate(Aggregate),
}

impl Field {
    /// The fields nested directly under this one: a [`Group`](FieldSource::Group)'s
    /// members or a [`Join`](Relation::Join)'s projection. Columns, aggregates,
    /// and constants have none.
    pub fn children(&self) -> &[Field] {
        match &self.source {
            FieldSource::Group(fields) => fields,
            FieldSource::Relation(Relation::Join { fields, .. }) => fields,
            FieldSource::Column(_)
            | FieldSource::Relation(Relation::Aggregate(_))
            | FieldSource::Constant(_) => &[],
        }
    }

    /// The relation this field draws on, if it draws on a related table.
    pub fn relation(&self) -> Option<&Relation> {
        match &self.source {
            FieldSource::Relation(relation) => Some(relation),
            _ => None,
        }
    }

    /// The column this field reads, if it is a plain column field.
    pub fn column(&self) -> Option<&common::ColumnName> {
        match &self.source {
            FieldSource::Column(column) => Some(&column.column),
            _ => None,
        }
    }
}

impl Relation {
    /// The related table this relation targets.
    pub fn table(&self) -> &common::TableName {
        match self {
            Relation::Join { join, .. } => &join.table,
            Relation::Aggregate(aggregate) => &aggregate.table,
        }
    }

    /// The key tying the related rows back to the parent row.
    pub fn key(&self) -> &JoinKey {
        match self {
            Relation::Join { join, .. } => &join.key,
            Relation::Aggregate(aggregate) => &aggregate.key,
        }
    }

    /// Filters narrowing the related rows, if any.
    pub fn filters(&self) -> Option<&[Filter]> {
        match self {
            Relation::Join { join, .. } => join.filters.as_deref(),
            Relation::Aggregate(aggregate) => aggregate.filters.as_deref(),
        }
    }
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

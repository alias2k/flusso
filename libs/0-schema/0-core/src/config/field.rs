use std::collections::BTreeMap;

use serde::Serialize;

use crate::common;

use super::{Aggregate, Filter, Join, JoinKey, Transform};

/// One field of a document: a name, an optional OpenSearch mapping override, and
/// a [`source`](Self::source) saying where its value comes from.
#[derive(Debug, Clone, Hash, Serialize)]
pub struct Field {
    pub field: common::FieldName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mapping: Option<Mapping>,
    pub source: FieldSource,
}

/// Where a field's value comes from. The shapes are mutually exclusive — a field
/// is exactly one of them — which is why this is an enum rather than a bag of
/// optional `column` / `relation` / `fields` that can contradict each other.
#[derive(Debug, Clone, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
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
#[derive(Debug, Clone, Hash, Serialize)]
pub struct Column {
    pub column: common::ColumnName,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub transforms: Vec<Transform>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<common::GenericValue>,
}

/// How a field draws on a related table: either folding its rows in as nested
/// documents ([`Join`](Self::Join)) or reducing them to a single value
/// ([`Aggregate`](Self::Aggregate)).
#[derive(Debug, Clone, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
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

/// Serializes the way OpenSearch expects a field mapping — `{ "type": …, …extra }`
/// — rather than the struct's two named fields. The `extra` settings sit beside
/// `type`, exactly as they would in the index body.
impl Serialize for Mapping {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(1 + self.extra.len()))?;
        map.serialize_entry("type", &self.mapping_type)?;
        for (key, value) in &self.extra {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
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

impl MappingType {
    /// The OpenSearch type name (`keyword`, `half_float`, …). An [`Other`] type
    /// is its own verbatim name.
    ///
    /// [`Other`]: MappingType::Other
    pub fn name(&self) -> &str {
        match self {
            MappingType::Text => "text",
            MappingType::Keyword => "keyword",
            MappingType::Boolean => "boolean",
            MappingType::Byte => "byte",
            MappingType::Short => "short",
            MappingType::Integer => "integer",
            MappingType::Long => "long",
            MappingType::Float => "float",
            MappingType::Double => "double",
            MappingType::HalfFloat => "half_float",
            MappingType::ScaledFloat => "scaled_float",
            MappingType::Date => "date",
            MappingType::Object => "object",
            MappingType::Nested => "nested",
            MappingType::Other(name) => name,
        }
    }
}

/// Serializes as the bare type name (`"keyword"`).
/// Used instead of serde with inner at other because this code will not fail
/// So having the name function will keep a single point of failure.
impl Serialize for MappingType {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.name())
    }
}

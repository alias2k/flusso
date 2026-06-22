use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::common;

use super::{Aggregate, AggregateKey, Filter, FlussoType, Join, JoinKind, Through, Transform};

/// One field of a document: a name, optional OpenSearch mapping `options` passed
/// through to the index, and a [`source`](Self::source) saying where its value
/// comes from. A leaf field's *type* is declared on its source (a
/// [`Column`]'s [`ty`](Column::ty), an [`Aggregate`]'s
/// [`value_type`](Aggregate::value_type)) so the document shape is known without
/// a database.
#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
pub struct Field {
    pub field: common::FieldName,
    /// Extra OpenSearch mapping properties merged beside the derived `type`
    /// (e.g. `analyzer`, `scaling_factor`). Empty for most fields.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, common::GenericValue>,
    pub source: FieldSource,
}

/// Where a field's value comes from. The shapes are mutually exclusive — a field
/// is exactly one of them — which is why this is an enum rather than a bag of
/// optional `column` / `relation` / `fields` that can contradict each other.
#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldSource {
    /// A column of the current row, optionally transformed, with an optional
    /// fallback when the column is null.
    Column(Column),
    /// A sub-object assembled from sibling fields of the *same* row (it adds a
    /// nesting level in the document without reading a related table).
    Group(Vec<Field>),
    /// A geographic point assembled from two same-row columns
    /// ([`lat`](Geo::lat)/[`lon`](Geo::lon)) into an OpenSearch `geo_point`.
    Geo(Geo),
    /// Data drawn from a related table — folded in as nested documents
    /// ([`Join`](Relation::Join)) or reduced to a single value
    /// ([`Aggregate`](Relation::Aggregate)).
    Relation(Relation),
    /// A constant value with no database source — `None` renders as null.
    Constant(common::GenericValue),
}

/// A column-backed field: the column to read, its declared type and nullability,
/// the transforms to apply, and a default to coalesce nulls to.
#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
pub struct Column {
    pub column: common::ColumnName,
    /// The declared type — the OpenSearch mapping derives from it, and a live
    /// database (when reachable) is checked against it.
    pub ty: FlussoType,
    /// Whether the column admits null. The resolver still forces non-null for a
    /// primary key or a column with a `default`.
    pub nullable: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transforms: Vec<Transform>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<common::GenericValue>,
}

/// A geographic point built from two same-row columns. Resolves to an
/// OpenSearch `geo_point`; the document carries `{ "lat": …, "lon": … }`, or
/// SQL `NULL` when either column is null (so a nullable point is absent rather
/// than `{lat: null, lon: null}`, which OpenSearch would reject).
#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
pub struct Geo {
    /// The latitude column (degrees).
    pub lat: common::ColumnName,
    /// The longitude column (degrees).
    pub lon: common::ColumnName,
    /// Whether the point may be absent — true unless the field is `required`.
    pub nullable: bool,
}

/// How a field draws on a related table: either folding its rows in as nested
/// documents ([`Join`](Self::Join)) or reducing them to a single value
/// ([`Aggregate`](Self::Aggregate)).
#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    /// Fold the related rows in as nested documents, projecting `fields` from
    /// each one.
    Join(Join),
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
            FieldSource::Relation(Relation::Join(join)) => &join.fields,
            FieldSource::Column(_)
            | FieldSource::Geo(_)
            | FieldSource::Relation(Relation::Aggregate(_))
            | FieldSource::Constant(_) => &[],
        }
    }

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

/// A relation's key, viewed uniformly across joins and aggregates — the three
/// physical shapes a "these tables connect" fact can take. Traversal code
/// (document SQL, reverse resolution) matches on this instead of caring whether
/// the relation is a join or an aggregate.
#[derive(Debug, Clone, Copy)]
pub enum RelationKey<'a> {
    /// The **parent** row holds the key: `parent.column → target.primary_key`
    /// (a `belongs_to`).
    Local(&'a common::ColumnName),
    /// The **related** rows hold the key: `target.foreign_key → parent.pk`
    /// (a `has_one`/`has_many`, or a direct-keyed aggregate).
    Direct(&'a common::ColumnName),
    /// Both sides connect through a junction table.
    Through(&'a Through),
}

impl Relation {
    pub fn table(&self) -> &common::TableName {
        match self {
            Relation::Join(join) => &join.table,
            Relation::Aggregate(aggregate) => &aggregate.table,
        }
    }

    /// The key tying the related rows and the parent row together.
    pub fn key(&self) -> RelationKey<'_> {
        match self {
            Relation::Join(join) => match &join.kind {
                JoinKind::BelongsTo { column } => RelationKey::Local(column),
                JoinKind::HasOne { foreign_key } | JoinKind::HasMany { foreign_key } => {
                    RelationKey::Direct(foreign_key)
                }
                JoinKind::ManyToMany { through } => RelationKey::Through(through),
            },
            Relation::Aggregate(aggregate) => match &aggregate.key {
                AggregateKey::Direct(foreign_key) => RelationKey::Direct(foreign_key),
                AggregateKey::Through(through) => RelationKey::Through(through),
            },
        }
    }

    /// Filters narrowing the related rows, if any.
    pub fn filters(&self) -> Option<&[Filter]> {
        match self {
            Relation::Join(join) => join.filters.as_deref(),
            Relation::Aggregate(aggregate) => aggregate.filters.as_deref(),
        }
    }
}

/// OpenSearch mapping. `mapping_type` is required; all other properties are passed through as-is.
#[derive(Debug, Clone, Hash)]
pub struct Mapping {
    pub mapping_type: MappingType,
    pub extra: BTreeMap<String, common::GenericValue>,
    /// For a `map` field (a dynamic-key object), the mapping type of every
    /// value; `None` for every other field. Internal metadata only — it is
    /// **not** serialized into the index body (a map carries just
    /// `{"type":"object","dynamic":true}`, the latter via `extra`). It exists so
    /// a consumer turning the mapping into typed bindings can tell a `map` from a
    /// plain `object` and offer a value-kind-typed handle.
    pub map_values: Option<MappingType>,
    /// Whether this numeric field came from a [`FlussoType::Decimal`] — a PG
    /// `numeric`/`decimal`. It maps to OpenSearch `double` like a true `double`,
    /// so [`mapping_type`](Self::mapping_type) alone can't tell them apart.
    /// Internal metadata only — **not** serialized into the index body. It lets a
    /// consumer turning the mapping into typed bindings offer a `Decimal`-kind
    /// handle (exact) instead of an `f64`-kind one.
    ///
    /// [`FlussoType::Decimal`]: super::FlussoType::Decimal
    pub decimal: bool,
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

    /// The mapping type for an OpenSearch type name — the inverse of
    /// [`name`](Self::name). An unrecognized name becomes [`Other`].
    ///
    /// [`Other`]: MappingType::Other
    pub fn from_name(name: &str) -> MappingType {
        match name {
            "text" => MappingType::Text,
            "keyword" => MappingType::Keyword,
            "boolean" => MappingType::Boolean,
            "byte" => MappingType::Byte,
            "short" => MappingType::Short,
            "integer" => MappingType::Integer,
            "long" => MappingType::Long,
            "float" => MappingType::Float,
            "double" => MappingType::Double,
            "half_float" => MappingType::HalfFloat,
            "scaled_float" => MappingType::ScaledFloat,
            "date" => MappingType::Date,
            "object" => MappingType::Object,
            "nested" => MappingType::Nested,
            other => MappingType::Other(other.to_owned()),
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

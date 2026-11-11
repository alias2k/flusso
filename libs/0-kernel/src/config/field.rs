use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

use crate::common;

use super::{
    Aggregate, AggregateKey, ColumnPair, Filter, FlussoType, Join, JoinKind, Through, Transform,
    ViaHop,
};

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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// For an [`Enum`](FlussoType::Enum) column, its variants in rank order;
    /// empty for a bare enum or any non-enum column. When non-empty the field
    /// sorts by this order instead of alphabetically — the OpenSearch sink
    /// prebakes the rank into a `.sort` subfield. Belongs on the column, not the
    /// [`FlussoType`], because it is a property of this specific field.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enum_order: Vec<String>,
    /// Whether a sink may add its automatic subfields to this string field
    /// (the OpenSearch `.keyword`/`.keyword_lowercase`/`.text`). `false` maps
    /// the bare field whatever the sink's own switch says. Only a `text`,
    /// `keyword` or `uuid` column may turn it off.
    #[serde(default = "enabled", skip_serializing_if = "is_enabled")]
    pub subfields: bool,
    /// Whether a sink indexes this field's content. `false` keeps the value in
    /// the stored document but maps it as one disabled object, so nothing
    /// under it is searchable. Only a `json` column may turn it off.
    #[serde(default = "enabled", skip_serializing_if = "is_enabled")]
    pub indexed: bool,
}

/// The serde default of an opt-out flag: on.
pub(crate) fn enabled() -> bool {
    true
}

/// Whether an opt-out flag is still on, so it is left out of the serialized form.
#[allow(clippy::trivially_copy_pass_by_ref)]
pub(crate) fn is_enabled(flag: &bool) -> bool {
    *flag
}

/// Bumped when the index a `map` column produces changes shape, so every
/// schema with a map gets a new content hash (a new physical index, rebuilt
/// once) while no other schema moves. `1`: keys mapped as their `values` type.
/// `2`: a keyword key also gets an exact `.keyword` subfield. Both land in
/// 0.17.0, so a 0.16 map schema still moves once; `2` moves a main-tip index
/// built under `1`, which lacks the subfield.
const MAP_KEY_MAPPING_REVISION: u8 = 2;

/// Hashes the fields in declaration order (exactly what the derive did), plus
/// `MAP_KEY_MAPPING_REVISION` for a `map` column only. `subfields` and `indexed`
/// are hashed only when turned off, so a schema that never opts out keeps the
/// content hash (and so the physical index) it had before they existed.
impl Hash for Column {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let Self {
            column,
            ty,
            nullable,
            transforms,
            default,
            enum_order,
            subfields,
            indexed,
        } = self;
        column.hash(state);
        ty.hash(state);
        nullable.hash(state);
        transforms.hash(state);
        default.hash(state);
        enum_order.hash(state);
        if matches!(ty, FlussoType::Map { .. }) {
            MAP_KEY_MAPPING_REVISION.hash(state);
        }
        if !subfields {
            "subfields".hash(state);
        }
        if !indexed {
            "indexed".hash(state);
        }
    }
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

    /// The tables between the parent and [`table`](Self::table), in order from
    /// the parent: an aggregate's `via`. Empty for a join and a one-hop
    /// aggregate. When non-empty, the [`key`](Self::key) ties the related rows
    /// to the last hop rather than to the parent.
    pub fn via(&self) -> &[ViaHop] {
        match self {
            Relation::Join(_) => &[],
            Relation::Aggregate(aggregate) => &aggregate.via,
        }
    }

    /// The relation's own extra column equalities: `local` on the table before
    /// [`table`](Self::table) (the parent, or the last `via` hop), `related` on
    /// [`table`](Self::table).
    pub fn on(&self) -> &[ColumnPair] {
        match self {
            Relation::Join(join) => &join.on,
            Relation::Aggregate(aggregate) => &aggregate.on,
        }
    }

    /// The pairs whose `local` column sits on the **parent** row: the first
    /// `via` hop's, or the relation's own when it has no `via`.
    pub fn parent_on(&self) -> &[ColumnPair] {
        match self.via().first() {
            Some(hop) => &hop.on,
            None => self.on(),
        }
    }
}

/// OpenSearch mapping. `mapping_type` is required; all other properties are passed through as-is.
#[derive(Debug, Clone, Hash)]
pub struct Mapping {
    pub mapping_type: MappingType,
    pub extra: BTreeMap<String, common::GenericValue>,
    /// For a `map` field (a dynamic-key object), the mapping every runtime key
    /// gets — the same mapping a plain field of the map's `values` type
    /// resolves to; `None` for every other field. A sink maps each key with it
    /// (the OpenSearch sink as a `dynamic_templates` entry scoped to the map's
    /// path); a consumer turning the mapping into typed bindings uses it to tell
    /// a `map` from a plain `object` and offer a value-kind-typed handle.
    /// Serialized as `values` beside the map's own `type`.
    pub map_values: Option<Box<Mapping>>,
    /// Whether this numeric field came from a [`FlussoType::Decimal`] — a PG
    /// `numeric`/`decimal`. It maps to OpenSearch `double` like a true `double`,
    /// so [`mapping_type`](Self::mapping_type) alone can't tell them apart.
    /// Internal metadata only — **not** serialized into the index body. It lets a
    /// consumer turning the mapping into typed bindings offer a `Decimal`-kind
    /// handle (exact) instead of an `f64`-kind one.
    ///
    /// [`FlussoType::Decimal`]: super::FlussoType::Decimal
    pub decimal: bool,
    /// For an [`Enum`](super::FlussoType::Enum) field with a declared order, its
    /// variants in rank order; `None` for a bare enum or any other field. The
    /// OpenSearch sink turns it into a `.sort` subfield (a per-field normalizer
    /// remapping each variant to a zero-padded rank) so the field sorts by
    /// declared order rather than alphabetically; a consumer building typed
    /// bindings uses it to offer an order-aware sort handle. Internal metadata —
    /// the enum itself is still serialized as a plain `keyword`.
    pub enum_order: Option<Vec<String>>,
    /// Whether a sink may add its automatic subfields to this string field —
    /// `false` when the schema field says `subfields: false`. A consumer
    /// building typed bindings reads it to drop the subfield accessors.
    /// Serialized as `subfields: false` only when off.
    pub subfields: bool,
    /// Whether a sink indexes this field — `false` when the schema field says
    /// `indexed: false`. The [`mapping_type`](Self::mapping_type) and the
    /// resolved children still describe the *stored* value's shape; the
    /// OpenSearch sink maps the field as one `object` with `enabled: false`
    /// whatever they say, and a consumer building typed bindings offers no
    /// query handle for it. Serialized as `indexed: false` only when off.
    pub indexed: bool,
}

impl Mapping {
    /// The mapping a field of the declared type `ty` gets with no options of its
    /// own: its OpenSearch type, the type's [default options](FlussoType::default_options)
    /// and its `decimal` flag. What every key of a `map` of `ty` is mapped as.
    pub fn of_type(ty: &FlussoType) -> Self {
        Self {
            mapping_type: ty.opensearch(),
            extra: ty.default_options(),
            map_values: None,
            decimal: matches!(ty, FlussoType::Decimal),
            enum_order: None,
            subfields: true,
            indexed: true,
        }
    }
}

/// Serializes the way OpenSearch expects a field mapping — `{ "type": …, …extra }`
/// — rather than the struct's named fields. The `extra` settings sit beside
/// `type`, exactly as they would in the index body. A `map` adds `values`: the
/// mapping each of its keys gets. A field that opted out adds `subfields: false`
/// or `indexed: false`.
impl Serialize for Mapping {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let len = 1
            + self.extra.len()
            + usize::from(self.map_values.is_some())
            + usize::from(!self.subfields)
            + usize::from(!self.indexed);
        let mut map = serializer.serialize_map(Some(len))?;
        map.serialize_entry("type", &self.mapping_type)?;
        for (key, value) in &self.extra {
            map.serialize_entry(key, value)?;
        }
        if let Some(values) = &self.map_values {
            map.serialize_entry("values", values)?;
        }
        if !self.subfields {
            map.serialize_entry("subfields", &false)?;
        }
        if !self.indexed {
            map.serialize_entry("indexed", &false)?;
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

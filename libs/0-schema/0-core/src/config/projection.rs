//! Projecting a self-describing schema into a fully-typed mapping — without a
//! database.
//!
//! Every gap a thin config once left to the source is now stated in the schema:
//! a column field carries its [`FlussoType`](super::FlussoType) and nullability,
//! an aggregate its result type. So the mapping follows from the schema alone.
//! The structural rules are unchanged from when the source derived them — a
//! group is an `object`, a to-many join is a `nested` array, a `count` is a
//! non-null `long`, a primary key is never null — they just no longer need a
//! round-trip to ask.

use crate::common::{ColumnName, GenericValue, IndexName};

use super::{
    Aggregate, AggregateOp, Column, Config, ContentHash, Field, FieldSource, IndexMapping,
    IndexSchema, Mapping, MappingType, Relation, ResolvedField,
};

impl Config {
    /// Project every **enabled** index into a fully-typed [`IndexMapping`],
    /// using only the declared schema. This is the database-free counterpart to
    /// the source's old enrichment: the engine runs it up front so the index is
    /// created from a complete description.
    pub fn resolve_mappings(&self) -> Vec<IndexMapping> {
        self.indexes
            .iter()
            .filter(|(_, index)| index.enabled)
            .map(|(name, index)| resolve_index(name.clone(), &index.schema))
            .collect()
    }
}

impl IndexSchema {
    /// Project this schema into its fully-typed [`IndexMapping`].
    pub fn resolve(&self, index: IndexName) -> IndexMapping {
        resolve_index(index, self)
    }
}

fn resolve_index(index: IndexName, schema: &IndexSchema) -> IndexMapping {
    IndexMapping {
        index,
        // Hash the parsed schema, not the file: structural changes (including a
        // declared type) flip the hash; cosmetic file changes do not.
        hash: ContentHash::of(schema),
        fields: resolve_fields(&schema.fields, schema.primary_key.as_ref()),
    }
}

/// Resolve a list of fields. `primary_key` is the root table's key while we are
/// still on the root row (it passes through groups, which stay on the same row);
/// it is `None` once we cross into a related table via a join.
fn resolve_fields(fields: &[Field], primary_key: Option<&ColumnName>) -> Vec<ResolvedField> {
    fields
        .iter()
        .map(|field| resolve_field(field, primary_key))
        .collect()
}

fn resolve_field(field: &Field, primary_key: Option<&ColumnName>) -> ResolvedField {
    // A group stays on the same row (the root key still applies); a join crosses
    // into a related table and brings its own primary key, so that table's key
    // forces its projected key column non-null — exactly as the root key does.
    // Columns/aggregates/constants have no children.
    let (child_fields, child_pk): (&[Field], Option<&ColumnName>) = match &field.source {
        FieldSource::Relation(Relation::Join(join)) => (&join.fields, Some(&join.primary_key)),
        FieldSource::Group(fields) => (fields, primary_key),
        _ => (&[], primary_key),
    };
    let children = resolve_fields(child_fields, child_pk);

    let (mapping_type, nullable) = type_and_nullability(field, primary_key);
    let mapping = Mapping {
        mapping_type,
        extra: field.options.clone(),
    };

    ResolvedField {
        name: field.field.clone(),
        mapping,
        nullable,
        children,
    }
}

/// The OpenSearch type and nullability of one field, from the declared schema.
fn type_and_nullability(field: &Field, primary_key: Option<&ColumnName>) -> (MappingType, bool) {
    match &field.source {
        // A column's declared type and nullability — except a primary key is
        // never null (it backs the id) and a `default` coalesces null away.
        FieldSource::Column(Column {
            column,
            ty,
            nullable,
            default,
            ..
        }) => {
            let forced_non_null = primary_key == Some(column) || default.is_some();
            (ty.opensearch(), *nullable && !forced_non_null)
        }
        // A group is always assembled — an object, never null.
        FieldSource::Group(_) => (MappingType::Object, false),
        // A geo point resolves to `geo_point`; its nullability is declared (a
        // `required` point is non-null, otherwise it may be absent).
        FieldSource::Geo(geo) => (MappingType::Other("geo_point".to_owned()), geo.nullable),
        // A constant is null exactly when the value is null.
        FieldSource::Constant(value) => (
            constant_mapping_type(value),
            matches!(value, GenericValue::Null),
        ),
        // A join's verb decides its shape and nullability: a to-one join
        // (`belongs_to`/`has_one`) is an object that may be absent; a to-many
        // join is a nested array, never null.
        FieldSource::Relation(Relation::Join(join)) => {
            if join.kind.is_to_many() {
                (MappingType::Nested, false)
            } else {
                (MappingType::Object, true)
            }
        }
        // An aggregate's type follows its op; only `count` is guaranteed
        // non-null. `sum`/`min`/`max` carry a declared `value_type`.
        FieldSource::Relation(Relation::Aggregate(aggregate)) => aggregate_type(aggregate),
    }
}

fn aggregate_type(aggregate: &Aggregate) -> (MappingType, bool) {
    match &aggregate.op {
        AggregateOp::Count => (MappingType::Long, false),
        AggregateOp::Avg(_) => (MappingType::Double, true),
        AggregateOp::Sum(_) | AggregateOp::Min(_) | AggregateOp::Max(_) => {
            let mapping_type = aggregate
                .value_type
                .as_ref()
                .map(|ty| ty.opensearch())
                // Conversion requires a `value_type` for these ops; `double` is
                // a defensive fallback that should never be reached.
                .unwrap_or(MappingType::Double);
            (mapping_type, true)
        }
    }
}

/// The mapping type a constant value's shape implies.
fn constant_mapping_type(value: &GenericValue) -> MappingType {
    match value {
        GenericValue::Bool(_) => MappingType::Boolean,
        GenericValue::Int(_) => MappingType::Long,
        GenericValue::Decimal(_) => MappingType::Double,
        GenericValue::Array(items) => items
            .first()
            .map(constant_mapping_type)
            .unwrap_or(MappingType::Keyword),
        GenericValue::Map(_) => MappingType::Object,
        GenericValue::String(_) | GenericValue::Null => MappingType::Keyword,
    }
}

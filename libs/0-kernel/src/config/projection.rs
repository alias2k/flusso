//! Projecting a self-describing schema into a fully-typed mapping — without a
//! database.
//!
//! Every gap a thin config once left to the source is now stated in the schema:
//! a column field carries its [`FlussoType`] and nullability,
//! an aggregate its result type. So the mapping follows from the schema alone.
//! The structural rules are unchanged from when the source derived them — a
//! group is an `object`, a to-many join is a `nested` array, a `count` is a
//! non-null `long`, a primary key is never null — they just no longer need a
//! round-trip to ask.

use crate::common::{ColumnName, GenericValue, IndexName};

use super::{
    Aggregate, AggregateOp, Column, ContentHash, Field, FieldSource, FlussoType, IndexMapping,
    IndexSchema, Mapping, MappingType, Relation, ResolvedField, Script,
    constant_is_stored_unindexed,
};

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
/// it is `None` once we cross into a related table via a join. An input-only
/// (`emit: false`) field never reaches the document, so it has no mapping.
fn resolve_fields(fields: &[Field], primary_key: Option<&ColumnName>) -> Vec<ResolvedField> {
    fields
        .iter()
        .filter(|field| field.emit)
        .map(|field| resolve_field(field, primary_key))
        .collect()
}

fn resolve_field(field: &Field, primary_key: Option<&ColumnName>) -> ResolvedField {
    let (child_fields, child_pk): (&[Field], Option<&ColumnName>) = match &field.source {
        FieldSource::Relation(Relation::Join(join)) => (&join.fields, Some(&join.primary_key)),
        FieldSource::Group(fields) => (fields, primary_key),
        _ => (&[], primary_key),
    };
    let children = resolve_fields(child_fields, child_pk);

    let (mapping_type, nullable, array) = type_and_nullability(field, primary_key);
    let mapping = Mapping {
        mapping_type,
        extra: field.options.clone(),
        map_values: map_value_type(field),
        decimal: is_decimal(field),
        enum_order: enum_order(field),
        subfields: has_subfields(field),
        indexed: is_indexed(field),
    };

    ResolvedField {
        name: field.field.clone(),
        mapping,
        nullable,
        array,
        children,
    }
}

/// The mapping every key of a `map` field gets — the mapping a plain field of
/// its `values` type resolves to — or `None` for any other field. This is the
/// only thing distinguishing a `map`'s `object` mapping from a plain one.
fn map_value_type(field: &Field) -> Option<Box<Mapping>> {
    match &field.source {
        FieldSource::Column(Column {
            ty: FlussoType::Map { values },
            ..
        }) => Some(Box::new(Mapping::of_type(values))),
        _ => None,
    }
}

/// Whether a sink may add its automatic subfields: off only for a column that
/// says `subfields: false`.
fn has_subfields(field: &Field) -> bool {
    match &field.source {
        FieldSource::Column(column) => column.subfields,
        FieldSource::Script(script) => script.subfields,
        _ => true,
    }
}

/// Whether a sink indexes this field: off for a `json` column or a join that
/// says `indexed: false`, and for a constant
/// [stored unindexed](constant_is_stored_unindexed).
fn is_indexed(field: &Field) -> bool {
    match &field.source {
        FieldSource::Column(column) => column.indexed,
        FieldSource::Relation(Relation::Join(join)) => join.indexed,
        FieldSource::Constant(value) => !constant_is_stored_unindexed(value),
        _ => true,
    }
}

/// The declared variant order of an [`Enum`](FlussoType::Enum) column field, or
/// `None` for a bare enum (empty order) or any other field. Drives the
/// order-aware `.sort` subfield the OpenSearch sink emits.
fn enum_order(field: &Field) -> Option<Vec<String>> {
    match &field.source {
        FieldSource::Column(Column {
            ty: FlussoType::Enum,
            enum_order,
            ..
        })
        | FieldSource::Script(Script {
            ty: FlussoType::Enum,
            enum_order,
            ..
        }) if !enum_order.is_empty() => Some(enum_order.clone()),
        _ => None,
    }
}

/// Whether this field's leaf type is [`FlussoType::Decimal`] — a PG
/// `numeric`/`decimal` (or a decimal-typed `sum`/`min`/`max`/`ids` aggregate, or
/// a decimal constant). It maps to OpenSearch `double` either way; this is what
/// lets a typed binding offer a `Decimal` handle instead of `f64`.
fn is_decimal(field: &Field) -> bool {
    let ty_is_decimal = |ty: &FlussoType| matches!(ty, FlussoType::Decimal);
    match &field.source {
        FieldSource::Column(Column { ty, .. }) | FieldSource::Script(Script { ty, .. }) => {
            ty_is_decimal(ty)
        }
        FieldSource::Constant(GenericValue::Decimal(_)) => true,
        FieldSource::Relation(Relation::Aggregate(aggregate)) => match &aggregate.op {
            AggregateOp::Sum(_) | AggregateOp::Min(_) | AggregateOp::Max(_) => {
                aggregate.value_type.as_ref().is_some_and(ty_is_decimal)
            }
            AggregateOp::Ids { element_type } => ty_is_decimal(element_type),
            AggregateOp::Count | AggregateOp::Avg(_) | AggregateOp::Exists => false,
        },
        _ => false,
    }
}

/// Returns `(mapping_type, nullable, array)`. `array` is true for an array
/// column, an `ids` aggregate or an array-returning script, whose
/// `mapping_type` is then the element type.
fn type_and_nullability(
    field: &Field,
    primary_key: Option<&ColumnName>,
) -> (MappingType, bool, bool) {
    match &field.source {
        FieldSource::Column(Column {
            column,
            ty,
            nullable,
            default,
            array,
            ..
        }) => {
            let forced_non_null = primary_key == Some(column) || default.is_some();
            (ty.opensearch(), *nullable && !forced_non_null, *array)
        }
        FieldSource::Group(_) => (MappingType::Object, false, false),
        FieldSource::Geo(geo) => (
            MappingType::Other("geo_point".to_owned()),
            geo.nullable,
            false,
        ),
        FieldSource::Constant(value) if constant_is_stored_unindexed(value) => {
            (MappingType::Object, false, false)
        }
        FieldSource::Constant(value) => (
            constant_mapping_type(value),
            matches!(value, GenericValue::Null),
            false,
        ),
        FieldSource::Relation(Relation::Join(join)) => {
            let mapping_type = if join.kind.is_to_many() {
                MappingType::Nested
            } else {
                MappingType::Object
            };
            (mapping_type, join.nullable, false)
        }
        FieldSource::Relation(Relation::Aggregate(aggregate)) => aggregate_type(aggregate),
        FieldSource::Script(script) => (script.ty.opensearch(), script.nullable, script.array),
    }
}

fn aggregate_type(aggregate: &Aggregate) -> (MappingType, bool, bool) {
    match &aggregate.op {
        AggregateOp::Count => (MappingType::Long, false, false),
        AggregateOp::Exists => (MappingType::Boolean, false, false),
        AggregateOp::Avg(_) => (MappingType::Double, true, false),
        AggregateOp::Sum(_) | AggregateOp::Min(_) | AggregateOp::Max(_) => {
            let mapping_type = aggregate
                .value_type
                .as_ref()
                .map(|ty| ty.opensearch())
                // Conversion requires a `value_type` for these ops; `double` is
                // a defensive fallback that should never be reached.
                .unwrap_or(MappingType::Double);
            (mapping_type, true, false)
        }
        AggregateOp::Ids { element_type } => (element_type.opensearch(), false, true),
    }
}

#[cfg(test)]
mod tests;

/// The mapping type a constant value's shape implies.
fn constant_mapping_type(value: &GenericValue) -> MappingType {
    match value {
        GenericValue::Bool(_) => MappingType::Boolean,
        GenericValue::SmallInt(_) => MappingType::Short,
        GenericValue::Int(_) => MappingType::Integer,
        GenericValue::BigInt(_) => MappingType::Long,
        GenericValue::Float(_) => MappingType::Float,
        GenericValue::Double(_) | GenericValue::Decimal(_) => MappingType::Double,
        GenericValue::Date(_)
        | GenericValue::Time(_)
        | GenericValue::Timestamp(_)
        | GenericValue::TimestampTz(_) => MappingType::Date,
        GenericValue::Bytes(_) => MappingType::Other("binary".to_owned()),
        GenericValue::Array(items) => items
            .first()
            .map(constant_mapping_type)
            .unwrap_or(MappingType::Keyword),
        GenericValue::Map(_) => MappingType::Object,
        GenericValue::String(_) | GenericValue::Uuid(_) | GenericValue::Null => {
            MappingType::Keyword
        }
    }
}

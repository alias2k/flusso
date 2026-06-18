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
    Aggregate, AggregateOp, Column, ContentHash, Field, FieldSource, IndexMapping, IndexSchema,
    Mapping, MappingType, Relation, ResolvedField,
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
/// it is `None` once we cross into a related table via a join.
fn resolve_fields(fields: &[Field], primary_key: Option<&ColumnName>) -> Vec<ResolvedField> {
    fields
        .iter()
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
    };

    ResolvedField {
        name: field.field.clone(),
        mapping,
        nullable,
        array,
        children,
    }
}

/// Returns `(mapping_type, nullable, array)`. `array` is true only for an `ids`
/// aggregate, whose `mapping_type` is then the element type.
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
            ..
        }) => {
            let forced_non_null = primary_key == Some(column) || default.is_some();
            (ty.opensearch(), *nullable && !forced_non_null, false)
        }
        FieldSource::Group(_) => (MappingType::Object, false, false),
        FieldSource::Geo(geo) => (
            MappingType::Other("geo_point".to_owned()),
            geo.nullable,
            false,
        ),
        FieldSource::Constant(value) => (
            constant_mapping_type(value),
            matches!(value, GenericValue::Null),
            false,
        ),
        FieldSource::Relation(Relation::Join(join)) => {
            if join.kind.is_to_many() {
                (MappingType::Nested, false, false)
            } else {
                (MappingType::Object, true, false)
            }
        }
        FieldSource::Relation(Relation::Aggregate(aggregate)) => aggregate_type(aggregate),
    }
}

fn aggregate_type(aggregate: &Aggregate) -> (MappingType, bool, bool) {
    match &aggregate.op {
        AggregateOp::Count => (MappingType::Long, false, false),
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
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use crate::common::{FieldName, IndexName, TableName};
    use crate::config::{
        Aggregate, AggregateKey, AggregateOp, DatabaseSchema, Field, FieldSource, FlussoType,
        IndexSchema, MappingType, Relation,
    };

    fn ids_schema(element_type: FlussoType) -> IndexSchema {
        IndexSchema {
            version: 1,
            table: TableName::try_new("users").unwrap(),
            db_schema: DatabaseSchema::default(),
            primary_key: None,
            doc_id: None,
            soft_delete: None,
            filters: None,
            fields: vec![Field {
                field: FieldName::try_new("orderIds").unwrap(),
                options: Default::default(),
                source: FieldSource::Relation(Relation::Aggregate(Aggregate {
                    table: TableName::try_new("orders").unwrap(),
                    op: AggregateOp::Ids { element_type },
                    key: AggregateKey::Direct(
                        crate::common::ColumnName::try_new("user_id").unwrap(),
                    ),
                    value_type: None,
                    filters: None,
                })),
            }],
        }
    }

    #[test]
    fn ids_projects_to_a_non_null_element_typed_array() {
        let mapping = ids_schema(FlussoType::Long).resolve(IndexName::try_new("users").unwrap());
        let field = &mapping.fields[0];
        assert_eq!(field.mapping.mapping_type, MappingType::Long);
        assert!(field.array);
        assert!(!field.nullable);

        let mapping = ids_schema(FlussoType::Keyword).resolve(IndexName::try_new("users").unwrap());
        let field = &mapping.fields[0];
        assert_eq!(field.mapping.mapping_type, MappingType::Keyword);
        assert!(field.array);
        assert!(!field.nullable);
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

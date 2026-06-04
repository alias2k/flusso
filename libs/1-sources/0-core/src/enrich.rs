//! Enriching a sparse config into a fully-typed mapping.
//!
//! The config a human writes is deliberately thin: a field may name only a
//! column and leave its type and nullability unsaid. Those gaps are real
//! information the index needs — and only the **source** can fill them, because
//! only the source knows how the store types and constrains its columns. Turning
//! the thin config into a complete description is therefore every source's job.
//!
//! Almost all of that job is the same regardless of the store. Whether a field
//! is an `object` or a `nested` array follows from a join's cardinality; a
//! `count` is always a non-null `long`; a constant's type follows from its shape;
//! a primary key is never null; a `default` coalesces null away. None of that
//! depends on whether the source is Postgres or anything else, so it lives here,
//! in [`resolve_index_mapping`], once.
//!
//! The one genuinely source-specific question — *what type and nullability does
//! the store give this base-table column?* — is the [`Catalog`] trait. A source
//! implements that single method and gets correct, uniform resolution (and the
//! same nullability rules) for free, rather than re-deriving the whole walk and
//! getting the edges wrong.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

use async_trait::async_trait;
use schema_core::{
    AggregateOp, ColumnName, Config, ContentHash, DatabaseSchema, Field, FieldSource, GenericValue,
    IndexMapping, IndexName, IndexSchema, JoinType, Mapping, MappingType, Relation, ResolvedField,
    TableName,
};

use crate::Result;

/// How a store types and constrains one base-table column — the single piece of
/// mapping resolution that is genuinely source-specific.
#[derive(Debug, Clone)]
pub struct ColumnInfo {
    /// The mapping type the column's native type implies (the source maps its own
    /// type system onto [`MappingType`]).
    pub mapping_type: MappingType,
    /// Whether the column itself admits null (its `NOT NULL` constraint). The
    /// resolver may still override this to non-null — for a primary key or a
    /// field with a `default` — so this is the column's intrinsic nullability,
    /// not necessarily the field's.
    pub nullable: bool,
}

/// A source's view of its own catalog: the type and nullability of a base-table
/// column. This is all [`resolve_index_mapping`] needs from a source to fill the
/// gaps a thin config leaves — everything else it derives itself.
#[async_trait]
pub trait Catalog: Send + Sync {
    /// The type and nullability of `column` in `table` (within `schema`), as the
    /// store defines it. An unknown column is an error — a field naming a column
    /// that does not exist is a misconfiguration.
    async fn column(
        &self,
        schema: &DatabaseSchema,
        table: &TableName,
        column: &ColumnName,
    ) -> Result<ColumnInfo>;
}

/// Enrich every **enabled** index in a [`Config`] into a fully-typed
/// [`IndexMapping`], filling each gap the config leaves with what the source's
/// [`Catalog`] knows. This is the read side's counterpart to loading the config:
/// the engine runs it up front so the index is created — and consumers are
/// generated — from a complete description rather than a thin one.
pub async fn enrich_indexes(config: &Config, catalog: &dyn Catalog) -> Result<Vec<IndexMapping>> {
    let mut mappings = Vec::new();
    for (name, index) in &config.indexes {
        if !index.enabled {
            continue;
        }
        mappings.push(resolve_index_mapping(name.clone(), &index.schema, catalog).await?);
    }
    Ok(mappings)
}

/// Resolve one index schema into its fully-typed [`IndexMapping`]: every field
/// given a concrete type (the explicit `mapping` where one is set, otherwise the
/// source's column type or the field's own shape) and a `nullable` flag.
pub async fn resolve_index_mapping(
    index: IndexName,
    schema: &IndexSchema,
    catalog: &dyn Catalog,
) -> Result<IndexMapping> {
    let fields = resolve_fields(
        &schema.db_schema,
        &schema.table,
        &schema.fields,
        schema.primary_key.as_ref(),
        catalog,
    )
    .await?;
    Ok(IndexMapping {
        index,
        // Hash the parsed schema, not the file: structural changes flip the
        // hash; cosmetic file changes (whitespace, comments) do not.
        hash: ContentHash::of(schema),
        fields,
    })
}

/// Resolve a list of fields under `table`. `primary_key` is the root table's key
/// while we are still on the root row (through groups, which stay on the same
/// row); it is `None` once we cross into a related table, where it no longer
/// applies.
///
/// Boxed because the recursion is through an `async fn`; the tree is shallow
/// (field nesting), so a heap allocation per level is negligible.
fn resolve_fields<'a>(
    db_schema: &'a DatabaseSchema,
    table: &'a TableName,
    fields: &'a [Field],
    primary_key: Option<&'a ColumnName>,
    catalog: &'a dyn Catalog,
) -> Pin<Box<dyn Future<Output = Result<Vec<ResolvedField>>> + Send + 'a>> {
    Box::pin(async move {
        let mut out = Vec::with_capacity(fields.len());
        for field in fields {
            out.push(resolve_field(db_schema, table, field, primary_key, catalog).await?);
        }
        Ok(out)
    })
}

/// Resolve one field: its children (if any), its mapping, and its nullability.
async fn resolve_field(
    db_schema: &DatabaseSchema,
    table: &TableName,
    field: &Field,
    primary_key: Option<&ColumnName>,
    catalog: &dyn Catalog,
) -> Result<ResolvedField> {
    // Children, the table whose columns they read, and whether the root primary
    // key still applies to them. A group stays on the same row (key still
    // applies); a join crosses into a related table (it does not).
    let (child_table, child_fields, child_pk): (&TableName, &[Field], Option<&ColumnName>) =
        match &field.source {
            FieldSource::Relation(Relation::Join { join, fields }) => (&join.table, fields, None),
            FieldSource::Group(fields) => (table, fields, primary_key),
            _ => (table, &[], primary_key),
        };
    let children = if child_fields.is_empty() {
        Vec::new()
    } else {
        resolve_fields(db_schema, child_table, child_fields, child_pk, catalog).await?
    };

    let (mapping, nullable) =
        resolve_mapping(db_schema, table, field, primary_key, catalog).await?;

    Ok(ResolvedField {
        name: field.field.clone(),
        mapping,
        nullable,
        children,
    })
}

/// The mapping and nullability of one field. An explicit `mapping` in the config
/// fixes the type; nullability is always derived (OpenSearch mappings don't
/// express it), per the rules in the table below — the source is consulted only
/// for what it alone knows.
async fn resolve_mapping(
    db_schema: &DatabaseSchema,
    table: &TableName,
    field: &Field,
    primary_key: Option<&ColumnName>,
    catalog: &dyn Catalog,
) -> Result<(Mapping, bool)> {
    // (inferred type, nullable). The type is used only when no explicit mapping
    // is given; the nullability always is.
    let (inferred_type, nullable) = match &field.source {
        // A column mirrors the store's own type and NOT NULL — except that a
        // primary key is never null (it backs the id) and a `default` coalesces
        // null away.
        FieldSource::Column(column) => {
            let info = catalog.column(db_schema, table, &column.column).await?;
            let forced_non_null = primary_key == Some(&column.column) || column.default.is_some();
            (info.mapping_type, info.nullable && !forced_non_null)
        }
        // A group is always assembled — an object, never null.
        FieldSource::Group(_) => (MappingType::Object, false),
        // A constant is null exactly when the value is null.
        FieldSource::Constant(value) => (
            constant_mapping_type(value),
            matches!(value, GenericValue::Null),
        ),
        // A join's arity decides its shape and nullability: one-to-one is an
        // object that may be absent; to-many is a nested array, empty but never
        // null.
        FieldSource::Relation(Relation::Join { join, .. }) => match join.join_type {
            JoinType::OneToOne => (MappingType::Object, true),
            JoinType::OneToMany | JoinType::ManyToMany => (MappingType::Nested, false),
        },
        // An aggregate's type follows its op; only `count` is guaranteed
        // non-null (zero rows is `0`) — the rest are null over zero rows.
        FieldSource::Relation(Relation::Aggregate(aggregate)) => match &aggregate.op {
            AggregateOp::Count => (MappingType::Long, false),
            AggregateOp::Avg(_) => (MappingType::Double, true),
            AggregateOp::Sum(column) | AggregateOp::Min(column) | AggregateOp::Max(column) => {
                // Only the type needs the catalog; nullability is fixed.
                let mapping_type = match &field.mapping {
                    Some(mapping) => mapping.mapping_type.clone(),
                    None => {
                        catalog
                            .column(db_schema, &aggregate.table, column)
                            .await?
                            .mapping_type
                    }
                };
                (mapping_type, true)
            }
        },
    };

    let mapping = match &field.mapping {
        Some(mapping) => mapping.clone(),
        None => Mapping {
            mapping_type: inferred_type,
            extra: BTreeMap::new(),
        },
    };
    Ok((mapping, nullable))
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

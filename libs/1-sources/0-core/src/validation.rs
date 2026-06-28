//! Validating a self-describing schema against a live store.
//!
//! The schema now states every field's type and nullability itself, so the
//! mapping is derived without a database (see
//! [`SourceSpec::index_mappings`](crate::SourceSpec::index_mappings)). A
//! database, *when reachable*, is still useful as a check: does each column
//! exist, and does its real type and nullability agree with what the schema
//! declares? That is this module's job — it reports disagreements as
//! [`Diagnostic`]s rather than filling anything in. With no database, it is
//! simply skipped.
//!
//! The one store-specific piece is the [`Catalog`] trait: how a store reports a
//! column's type and nullability. Everything else — the field walk, which table
//! a field reads from — is shared here.

use std::future::Future;
use std::pin::Pin;

use async_trait::async_trait;
use schema_core::common::{ColumnName, IndexName};
use schema_core::{
    AggregateOp, Column, DatabaseSchema, Field, FieldSource, FlussoType, Geo, Relation, TableName,
};

use crate::{Result, SourceError, SourceSpec};

/// How a store reports one base-table column: its native type name (as the
/// store spells it, e.g. Postgres `character varying(255)`) and whether it
/// admits null.
#[derive(Debug, Clone)]
pub struct ColumnInfo {
    pub sql_type: String,
    pub nullable: bool,
}

/// A source's view of its own catalog: the type and nullability of a base-table
/// column. This is all [`validate_indexes`] needs from a source to check a
/// declared schema against the live store.
#[async_trait]
pub trait Catalog: Send + Sync {
    /// The type and nullability of `column` in `table` (within `schema`), as the
    /// store defines it.
    async fn column(
        &self,
        schema: &DatabaseSchema,
        table: &TableName,
        column: &ColumnName,
    ) -> Result<ColumnInfo>;
}

/// How serious a [`Diagnostic`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// The declared schema and the database disagree in a way that will produce
    /// a wrong or rejected mapping.
    Error,
    /// A softer mismatch worth surfacing (e.g. a column declared non-null that
    /// the database allows to be null).
    Warning,
}

/// One disagreement between a declared schema and the live database.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub index: IndexName,
    pub field: FieldName,
    pub severity: Severity,
    pub message: String,
}

type FieldName = schema_core::common::FieldName;

/// Look up a column, turning a "no such column" into an Error [`Diagnostic`] on
/// the field — so validation keeps going and points right at it — instead of
/// aborting. A real transport failure (the store became unreachable) still
/// propagates. Returns `None` when the column was missing (a diagnostic was
/// emitted); callers skip their remaining checks for that field.
async fn lookup_column(
    catalog: &dyn Catalog,
    index: &IndexName,
    db_schema: &DatabaseSchema,
    table: &TableName,
    field: &FieldName,
    column: &ColumnName,
    out: &mut Vec<Diagnostic>,
) -> Result<Option<ColumnInfo>> {
    match catalog.column(db_schema, table, column).await {
        Ok(info) => Ok(Some(info)),
        Err(SourceError::UnknownColumn(what)) => {
            out.push(Diagnostic {
                index: index.clone(),
                field: field.clone(),
                severity: Severity::Error,
                message: format!("references unknown column `{what}`"),
            });
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

/// Validate every index in `spec` against the store behind `catalog`, returning
/// the disagreements found. An empty result means the declared schema matches
/// the database. The spec already holds only enabled indexes.
pub async fn validate_indexes(spec: &SourceSpec, catalog: &dyn Catalog) -> Result<Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();
    for (name, schema) in spec.indexes() {
        validate_fields(
            name,
            &schema.db_schema,
            &schema.table,
            &schema.fields,
            schema.primary_key.as_ref(),
            catalog,
            &mut diagnostics,
        )
        .await?;
    }
    Ok(diagnostics)
}

/// Validate a list of fields under `table`. Boxed because the recursion is
/// through an `async fn`.
fn validate_fields<'a>(
    index: &'a IndexName,
    db_schema: &'a DatabaseSchema,
    table: &'a TableName,
    fields: &'a [Field],
    primary_key: Option<&'a ColumnName>,
    catalog: &'a dyn Catalog,
    out: &'a mut Vec<Diagnostic>,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
    Box::pin(async move {
        for field in fields {
            validate_field(index, db_schema, table, field, primary_key, catalog, out).await?;
        }
        Ok(())
    })
}

async fn validate_field(
    index: &IndexName,
    db_schema: &DatabaseSchema,
    table: &TableName,
    field: &Field,
    primary_key: Option<&ColumnName>,
    catalog: &dyn Catalog,
    out: &mut Vec<Diagnostic>,
) -> Result<()> {
    match &field.source {
        FieldSource::Column(column) => {
            validate_column(
                index,
                db_schema,
                table,
                &field.field,
                column,
                primary_key,
                catalog,
                out,
            )
            .await?;
        }
        FieldSource::Relation(Relation::Aggregate(aggregate)) => {
            let column = match &aggregate.op {
                AggregateOp::Sum(c) | AggregateOp::Min(c) | AggregateOp::Max(c) => Some(c),
                AggregateOp::Count | AggregateOp::Avg(_) | AggregateOp::Ids { .. } => None,
            };
            if let (Some(column), Some(value_type)) = (column, &aggregate.value_type) {
                check_type(
                    index,
                    db_schema,
                    &aggregate.table,
                    &field.field,
                    column,
                    value_type,
                    catalog,
                    out,
                )
                .await?;
            }
        }
        FieldSource::Group(fields) => {
            validate_fields(index, db_schema, table, fields, primary_key, catalog, out).await?;
        }
        FieldSource::Relation(Relation::Join(join)) => {
            validate_fields(
                index,
                db_schema,
                &join.table,
                &join.fields,
                Some(&join.primary_key),
                catalog,
                out,
            )
            .await?;
        }
        FieldSource::Geo(geo) => {
            validate_geo(index, db_schema, table, &field.field, geo, catalog, out).await?;
        }
        FieldSource::Constant(_) => {}
    }
    Ok(())
}

/// Confirm both coordinate columns of a geo point exist and hold a numeric type.
async fn validate_geo(
    index: &IndexName,
    db_schema: &DatabaseSchema,
    table: &TableName,
    field: &FieldName,
    geo: &Geo,
    catalog: &dyn Catalog,
    out: &mut Vec<Diagnostic>,
) -> Result<()> {
    const NUMERIC: &[FlussoType] = &[
        FlussoType::Double,
        FlussoType::Float,
        FlussoType::Decimal,
        FlussoType::Integer,
        FlussoType::Long,
        FlussoType::Short,
    ];
    for column in [&geo.lat, &geo.lon] {
        let Some(info) =
            lookup_column(catalog, index, db_schema, table, field, column, out).await?
        else {
            continue;
        };
        if !NUMERIC.iter().any(|ty| ty.accepts_pg(&info.sql_type)) {
            out.push(Diagnostic {
                index: index.clone(),
                field: field.clone(),
                severity: Severity::Error,
                message: format!(
                    "geo_point coordinate column `{column}` must be numeric, found `{}`",
                    info.sql_type
                ),
            });
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn validate_column(
    index: &IndexName,
    db_schema: &DatabaseSchema,
    table: &TableName,
    field: &FieldName,
    column: &Column,
    primary_key: Option<&ColumnName>,
    catalog: &dyn Catalog,
    out: &mut Vec<Diagnostic>,
) -> Result<()> {
    let Some(info) =
        lookup_column(catalog, index, db_schema, table, field, &column.column, out).await?
    else {
        return Ok(());
    };

    if !column.ty.accepts_pg(&info.sql_type) {
        out.push(Diagnostic {
            index: index.clone(),
            field: field.clone(),
            severity: Severity::Error,
            message: format!(
                "declared type does not accept the column's database type `{}`",
                info.sql_type
            ),
        });
    }

    // A primary key or a `default` makes the field non-null regardless, so only
    // a plain non-null declaration over a nullable column is worth flagging.
    let forced_non_null = primary_key == Some(&column.column) || column.default.is_some();
    if !column.nullable && info.nullable && !forced_non_null {
        out.push(Diagnostic {
            index: index.clone(),
            field: field.clone(),
            severity: Severity::Warning,
            message: "declared non-null (`required`) but the database column allows null"
                .to_owned(),
        });
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn check_type(
    index: &IndexName,
    db_schema: &DatabaseSchema,
    table: &TableName,
    field: &FieldName,
    column: &ColumnName,
    declared: &FlussoType,
    catalog: &dyn Catalog,
    out: &mut Vec<Diagnostic>,
) -> Result<()> {
    let Some(info) = lookup_column(catalog, index, db_schema, table, field, column, out).await?
    else {
        return Ok(());
    };
    if !declared.accepts_pg(&info.sql_type) {
        out.push(Diagnostic {
            index: index.clone(),
            field: field.clone(),
            severity: Severity::Error,
            message: format!(
                "declared aggregate type does not accept the column's database type `{}`",
                info.sql_type
            ),
        });
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeMap;

    use schema_core::IndexSchema;
    use schema_core::common::IndexName;
    use serde_json::json;

    use super::*;

    /// A catalog whose every column lookup fails as "unknown" — the post-stale-DB
    /// case from the designer.
    struct NoColumns;

    #[async_trait]
    impl Catalog for NoColumns {
        async fn column(
            &self,
            schema: &DatabaseSchema,
            table: &TableName,
            column: &ColumnName,
        ) -> Result<ColumnInfo> {
            Err(SourceError::UnknownColumn(format!(
                "{schema}.{table}.{column}"
            )))
        }
    }

    /// An unknown column is a per-field Error diagnostic, not a fatal error that
    /// aborts validation (and gets mislabelled "database not reachable").
    #[test]
    fn unknown_column_is_a_diagnostic_not_a_fatal_error() {
        let schema: IndexSchema = serde_json::from_value(json!({
            "version": 1,
            "table": "products",
            "db_schema": "public",
            "fields": [{
                "field": "title",
                "source": { "column": { "column": "title", "ty": "keyword", "nullable": true } },
            }],
        }))
        .unwrap();
        let mut indexes = BTreeMap::new();
        indexes.insert(IndexName::try_new("products").unwrap(), schema);
        let spec = SourceSpec::new(indexes);

        let diagnostics = futures::executor::block_on(validate_indexes(&spec, &NoColumns)).unwrap();

        assert_eq!(
            diagnostics.len(),
            1,
            "the unknown column should produce one diagnostic"
        );
        assert_eq!(diagnostics[0].severity, Severity::Error);
        assert!(
            diagnostics[0].message.contains("unknown column"),
            "got: {}",
            diagnostics[0].message,
        );
    }
}

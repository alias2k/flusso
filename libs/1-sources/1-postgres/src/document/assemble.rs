//! Assembling a document body from a root row and its relations: column fields
//! (with transforms and defaults), joins (recursing for nested relations), and
//! aggregates.

use std::collections::{BTreeMap, HashMap};

use schema_core::{
    Aggregate, DatabaseSchema, Field, FieldRelation, GenericValue, Join, JoinKey, JoinType,
    Transform,
};
use sea_query::PostgresQueryBuilder;
use sea_query_binder::SqlxBinder;
use sources_core::{Result, SourceError};

use super::fields::{collect_column_fields, contains_relation};
use super::{PgDocumentBuilder, push_unique, query_err, sql, value};

impl PgDocumentBuilder {
    /// Assemble a row's fields. `current_pk` is this row's primary-key value,
    /// used as the parent key for any relation among `fields`.
    pub(super) async fn assemble_row(
        &self,
        db: &DatabaseSchema,
        fields: &[Field],
        row: &HashMap<String, GenericValue>,
        current_pk: &GenericValue,
    ) -> Result<BTreeMap<String, GenericValue>> {
        let mut object = BTreeMap::new();
        for field in fields {
            let value = match &field.relation {
                Some(FieldRelation::Join(join)) => {
                    self.assemble_join(db, field, join, current_pk).await?
                }
                Some(FieldRelation::Aggregate(aggregate)) => {
                    self.assemble_aggregate(db, aggregate, current_pk).await?
                }
                None => match (&field.column, &field.fields) {
                    (Some(column), _) => {
                        let raw = row.get(column.as_ref()).cloned().unwrap_or(GenericValue::Null);
                        finalize_scalar(raw, field)
                    }
                    // Same-row nested group: read from the same row and key.
                    (None, Some(nested)) => GenericValue::Map(
                        Box::pin(self.assemble_row(db, nested, row, current_pk)).await?,
                    ),
                    (None, None) => field.default.clone().unwrap_or(GenericValue::Null),
                },
            };
            object.insert(field.field.to_string(), value);
        }
        Ok(object)
    }

    async fn assemble_join(
        &self,
        db: &DatabaseSchema,
        field: &Field,
        join: &Join,
        parent_pk: &GenericValue,
    ) -> Result<GenericValue> {
        require_pk(parent_pk)?;
        let sub_fields = field.fields.as_deref().unwrap_or_default();
        let needs_child_pk = contains_relation(sub_fields);
        let is_through = matches!(&join.key, JoinKey::Through(_));

        let mut sub_columns = Vec::new();
        collect_column_fields(sub_fields, &mut sub_columns);

        // The joined table's own primary key — needed to reach the far table of
        // a through join, to key nested relations, or to have a column to select
        // when the join projects none.
        let joined_pk = if needs_child_pk || is_through || sub_columns.is_empty() {
            Some(self.table_primary_key(db, &join.table).await?)
        } else {
            None
        };
        if let Some(pk) = &joined_pk {
            push_unique(&mut sub_columns, pk);
        }

        let query = match &join.key {
            JoinKey::Direct(foreign_key) => {
                sql::join_select(db, join, foreign_key, &sub_columns, parent_pk)?
            }
            JoinKey::Through(through) => {
                let far_pk = joined_pk
                    .as_ref()
                    .ok_or_else(|| SourceError::Query("internal: missing far primary key".into()))?;
                sql::through_select(db, join, through, far_pk, &sub_columns, parent_pk)?
            }
        };
        let (statement, values) = query.build_sqlx(PostgresQueryBuilder);
        let rows = sqlx::query_with(&statement, values)
            .fetch_all(&self.pool)
            .await
            .map_err(query_err)?;

        let mut objects = Vec::with_capacity(rows.len());
        for row in &rows {
            let row_map = value::row_to_map(row);
            let row_pk = match (needs_child_pk, &joined_pk) {
                (true, Some(pk)) => {
                    row_map.get(pk.as_ref()).cloned().unwrap_or(GenericValue::Null)
                }
                _ => GenericValue::Null,
            };
            objects.push(GenericValue::Map(
                Box::pin(self.assemble_row(db, sub_fields, &row_map, &row_pk)).await?,
            ));
        }

        Ok(match join.join_type {
            JoinType::OneToOne => objects.into_iter().next().unwrap_or(GenericValue::Null),
            JoinType::OneToMany | JoinType::ManyToMany => GenericValue::Array(objects),
        })
    }

    async fn assemble_aggregate(
        &self,
        db: &DatabaseSchema,
        aggregate: &Aggregate,
        parent_pk: &GenericValue,
    ) -> Result<GenericValue> {
        require_pk(parent_pk)?;
        let query = match &aggregate.key {
            JoinKey::Direct(foreign_key) => {
                sql::aggregate_select(db, aggregate, foreign_key, parent_pk)?
            }
            JoinKey::Through(through) => {
                let far_pk = self.table_primary_key(db, &aggregate.table).await?;
                sql::through_aggregate_select(db, aggregate, through, &far_pk, parent_pk)?
            }
        };
        let (statement, values) = query.build_sqlx(PostgresQueryBuilder);
        let row = sqlx::query_with(&statement, values)
            .fetch_optional(&self.pool)
            .await
            .map_err(query_err)?;

        Ok(match row {
            Some(row) => value::row_to_map(&row)
                .into_values()
                .next()
                .unwrap_or(GenericValue::Null),
            None => GenericValue::Null,
        })
    }
}

fn finalize_scalar(raw: GenericValue, field: &Field) -> GenericValue {
    match apply_transforms(raw, field.transforms.as_deref()) {
        GenericValue::Null => field.default.clone().unwrap_or(GenericValue::Null),
        value => value,
    }
}

fn apply_transforms(value: GenericValue, transforms: Option<&[Transform]>) -> GenericValue {
    let Some(transforms) = transforms else {
        return value;
    };
    let mut value = value;
    for transform in transforms {
        value = match value {
            // Transforms only apply to strings; anything else passes through.
            GenericValue::String(s) => GenericValue::String(match transform {
                Transform::Lowercase => s.to_lowercase(),
                Transform::Trim => s.trim().to_owned(),
            }),
            other => other,
        };
    }
    value
}

/// Relations need a single primary-key value to match against; a null one means
/// the index lacks a usable single-column primary key.
fn require_pk(pk: &GenericValue) -> Result<()> {
    if matches!(pk, GenericValue::Null) {
        return Err(SourceError::Unsupported(
            "relations require a single-column primary key value (declare `primary_key`)".into(),
        ));
    }
    Ok(())
}

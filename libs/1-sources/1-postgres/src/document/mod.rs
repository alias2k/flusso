//! [`PgDocumentBuilder`] — the read half of the Postgres source.
//!
//! Resolves which documents a changed row affects and assembles them from the
//! schema with sea-query + sqlx. Each relation is resolved with its own
//! single-table query (see [`sql`]) and stitched into a [`GenericValue`] tree.
//!
//! ## Coverage
//!
//! Implemented: root-table resolution, reverse resolution of direct
//! foreign-key relations, column fields with transforms and defaults,
//! one-to-one / one-to-many direct joins (with filters, ordering, limit),
//! aggregates, boolean soft-delete, and tombstones for missing rows.
//!
//! Not yet supported (each surfaces a clear [`SourceError::Unsupported`] or a
//! warning rather than silently producing wrong data): many-to-many / `through`
//! relations, relations nested inside a joined field, composite primary keys on
//! indexes that use joins or aggregates, and soft-delete `when` filters. Column
//! decoding covers the common scalar types; others decode to null with a
//! warning (so e.g. timestamp-based soft delete is not yet detected).

mod sql;
mod value;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use schema_core::{
    Aggregate, ColumnName, Config, DatabaseSchema, Field, FieldName, FieldRelation, GenericValue,
    IndexSchema, Join, JoinKey, JoinType, SoftDelete, TableName, Transform,
};
use sea_query::PostgresQueryBuilder;
use sea_query_binder::SqlxBinder;
use sources_core::document::{Document, DocumentBuilder, DocumentId};
use sources_core::{Result, RowKey, SourceError};
use sqlx::PgPool;

/// Builds index documents from a Postgres database, driven by the loaded
/// [`Config`]. Cheap to clone — the pool and config are shared.
#[derive(Debug, Clone)]
pub struct PgDocumentBuilder {
    pool: PgPool,
    config: Arc<Config>,
}

impl PgDocumentBuilder {
    /// Create a builder over a connection pool and the resolved config.
    pub fn new(pool: PgPool, config: Arc<Config>) -> Self {
        Self { pool, config }
    }

    /// Find which root rows a changed child row belongs to, by selecting the
    /// foreign key from the (still-present) child row. A child *delete* finds
    /// nothing here — its row is already gone — which is the known limit of
    /// reverse-resolving from a thin, key-only change.
    async fn reverse_lookup(
        &self,
        schema: &DatabaseSchema,
        child: &TableName,
        foreign_key: &ColumnName,
        child_key: &RowKey,
    ) -> Result<Vec<GenericValue>> {
        let query = sql::reverse_select(schema, child, foreign_key, &child_key.0)?;
        let (statement, values) = query.build_sqlx(PostgresQueryBuilder);
        let rows = sqlx::query_with(&statement, values)
            .fetch_all(&self.pool)
            .await
            .map_err(query_err)?;

        let mut seen = HashSet::new();
        let mut roots = Vec::new();
        for row in &rows {
            if let Some(value) = value::row_to_map(row).remove(foreign_key.as_ref())
                && !matches!(value, GenericValue::Null)
                && seen.insert(value.clone())
            {
                roots.push(value);
            }
        }
        Ok(roots)
    }

    /// Assemble every field of an index into a document body.
    async fn assemble(
        &self,
        schema: &IndexSchema,
        root: &HashMap<String, GenericValue>,
        key: &RowKey,
    ) -> Result<BTreeMap<String, GenericValue>> {
        let mut body = BTreeMap::new();
        for field in &schema.fields {
            let value = self.assemble_field(schema, field, root, key).await?;
            body.insert(field.field.to_string(), value);
        }
        Ok(body)
    }

    async fn assemble_field(
        &self,
        schema: &IndexSchema,
        field: &Field,
        root: &HashMap<String, GenericValue>,
        key: &RowKey,
    ) -> Result<GenericValue> {
        match &field.relation {
            Some(FieldRelation::Join(join)) => self.assemble_join(schema, field, join, key).await,
            Some(FieldRelation::Aggregate(aggregate)) => {
                self.assemble_aggregate(schema, aggregate, key).await
            }
            None => assemble_scalar_or_nested(field, root),
        }
    }

    async fn assemble_join(
        &self,
        schema: &IndexSchema,
        field: &Field,
        join: &Join,
        key: &RowKey,
    ) -> Result<GenericValue> {
        let foreign_key = match &join.key {
            JoinKey::Direct(fk) => fk,
            JoinKey::Through(_) => {
                return Err(SourceError::Unsupported(
                    "through (many-to-many) joins are not yet supported".into(),
                ));
            }
        };
        let root_pk = single_key_value(key)?;
        let sub_fields = field.fields.as_deref().unwrap_or_default();
        let sub_columns = column_names(sub_fields)?;

        let query = sql::join_select(&schema.db_schema, join, foreign_key, &sub_columns, root_pk)?;
        let (statement, values) = query.build_sqlx(PostgresQueryBuilder);
        let rows = sqlx::query_with(&statement, values)
            .fetch_all(&self.pool)
            .await
            .map_err(query_err)?;

        let mut objects = Vec::with_capacity(rows.len());
        for row in &rows {
            let row_map = value::row_to_map(row);
            objects.push(GenericValue::Map(assemble_columns(sub_fields, &row_map)?));
        }

        match join.join_type {
            JoinType::OneToOne => Ok(objects.into_iter().next().unwrap_or(GenericValue::Null)),
            JoinType::OneToMany => Ok(GenericValue::Array(objects)),
            JoinType::ManyToMany => Err(SourceError::Unsupported(
                "many-to-many joins are not yet supported".into(),
            )),
        }
    }

    async fn assemble_aggregate(
        &self,
        schema: &IndexSchema,
        aggregate: &Aggregate,
        key: &RowKey,
    ) -> Result<GenericValue> {
        let foreign_key = match &aggregate.key {
            JoinKey::Direct(fk) => fk,
            JoinKey::Through(_) => {
                return Err(SourceError::Unsupported(
                    "aggregates through a junction table are not yet supported".into(),
                ));
            }
        };
        let root_pk = single_key_value(key)?;

        let query = sql::aggregate_select(&schema.db_schema, aggregate, foreign_key, root_pk)?;
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

#[async_trait]
impl DocumentBuilder for PgDocumentBuilder {
    async fn resolve(&self, table: &TableName, key: &RowKey) -> Result<Vec<DocumentId>> {
        let mut ids = Vec::new();
        for (name, index) in &self.config.indexes {
            if !index.enabled {
                continue;
            }
            let schema = &index.schema;

            // Change on the document's own root table: the key is the id.
            if schema.table == *table {
                ids.push(DocumentId {
                    index: name.clone(),
                    key: key.clone(),
                });
                continue;
            }

            // Change on a related table: reverse-resolve to the affected roots.
            match find_reverse(schema, table) {
                Reverse::None => {}
                Reverse::Unsupported => {
                    tracing::warn!(
                        index = %name, table = %table,
                        "change on a through/many-to-many related table is not yet reverse-resolved; documents may be stale",
                    );
                }
                Reverse::Direct(foreign_key) => {
                    let Some(pk_column) = schema.primary_key.clone() else {
                        tracing::warn!(
                            index = %name, table = %table,
                            "cannot reverse-resolve: index has no primary_key",
                        );
                        continue;
                    };
                    let roots = self
                        .reverse_lookup(&schema.db_schema, table, &foreign_key, key)
                        .await?;
                    for root in roots {
                        ids.push(DocumentId {
                            index: name.clone(),
                            key: RowKey(vec![(pk_column.clone(), root)]),
                        });
                    }
                }
            }
        }
        Ok(ids)
    }

    async fn build(&self, id: &DocumentId) -> Result<Document> {
        let index = self
            .config
            .indexes
            .get(&id.index)
            .ok_or_else(|| SourceError::Query(format!("unknown index `{}`", id.index)))?;
        let schema = &index.schema;

        let mut columns = root_columns(schema);
        for (column, _) in &id.key.0 {
            push_unique(&mut columns, column);
        }

        let query = sql::root_select(&schema.db_schema, &schema.table, &columns, &id.key.0)?;
        let (statement, values) = query.build_sqlx(PostgresQueryBuilder);
        let row = sqlx::query_with(&statement, values)
            .fetch_optional(&self.pool)
            .await
            .map_err(query_err)?;

        // No root row, or it is soft-deleted → the document should not exist.
        let Some(row) = row else {
            return Ok(Document::Delete { id: id.clone() });
        };
        let root = value::row_to_map(&row);
        if is_soft_deleted(schema, &root) {
            return Ok(Document::Delete { id: id.clone() });
        }

        let body = self.assemble(schema, &root, &id.key).await?;
        Ok(Document::Upsert {
            id: id.clone(),
            body: GenericValue::Map(body),
        })
    }
}

/// How a changed table relates to an index, for reverse resolution.
enum Reverse {
    /// Not part of this index.
    None,
    /// A direct foreign-key relation; reverse-resolve via this key.
    Direct(ColumnName),
    /// A relation we can't reverse-resolve yet (through / many-to-many).
    Unsupported,
}

fn find_reverse(schema: &IndexSchema, table: &TableName) -> Reverse {
    let mut relations = Vec::new();
    collect_relations(&schema.fields, &mut relations);
    for relation in relations {
        let (related_table, key) = match relation {
            FieldRelation::Join(join) => (&join.table, &join.key),
            FieldRelation::Aggregate(aggregate) => (&aggregate.table, &aggregate.key),
        };
        match key {
            JoinKey::Direct(fk) if related_table == table => return Reverse::Direct(fk.clone()),
            JoinKey::Through(through) if &through.table == table || related_table == table => {
                return Reverse::Unsupported;
            }
            JoinKey::Direct(_) | JoinKey::Through(_) => {}
        }
    }
    Reverse::None
}

fn collect_relations<'a>(fields: &'a [Field], out: &mut Vec<&'a FieldRelation>) {
    for field in fields {
        if let Some(relation) = &field.relation {
            out.push(relation);
        }
        if let Some(nested) = &field.fields {
            collect_relations(nested, out);
        }
    }
}

/// The root-table columns the document reads: primary key, doc id, soft-delete
/// column, and every column-backed field (including same-row nested groups).
fn root_columns(schema: &IndexSchema) -> Vec<ColumnName> {
    let mut columns = Vec::new();
    if let Some(pk) = &schema.primary_key {
        push_unique(&mut columns, pk);
    }
    if let Some(doc_id) = &schema.doc_id {
        push_unique(&mut columns, doc_id);
    }
    match &schema.soft_delete {
        Some(SoftDelete::Column(c)) => push_unique(&mut columns, &c.column),
        Some(SoftDelete::Field(f)) => {
            if let Some(column) = field_column(&schema.fields, &f.field) {
                push_unique(&mut columns, column);
            }
        }
        None => {}
    }
    collect_column_fields(&schema.fields, &mut columns);
    columns
}

fn collect_column_fields(fields: &[Field], out: &mut Vec<ColumnName>) {
    for field in fields {
        if field.relation.is_some() {
            continue;
        }
        if let Some(column) = &field.column {
            push_unique(out, column);
        }
        if let Some(nested) = &field.fields {
            collect_column_fields(nested, out);
        }
    }
}

/// The columns a joined field projects. Errors if a sub-field is itself a
/// relation — joins nested inside joins are not yet supported.
fn column_names(fields: &[Field]) -> Result<Vec<ColumnName>> {
    let mut columns = Vec::new();
    for field in fields {
        if field.relation.is_some() {
            return Err(SourceError::Unsupported(
                "relations nested inside a joined field are not yet supported".into(),
            ));
        }
        if let Some(column) = &field.column {
            push_unique(&mut columns, column);
        }
        if let Some(nested) = &field.fields {
            columns.extend(column_names(nested)?);
        }
    }
    Ok(columns)
}

/// Project column-backed fields (and same-row nested groups) from a row.
fn assemble_columns(
    fields: &[Field],
    row: &HashMap<String, GenericValue>,
) -> Result<BTreeMap<String, GenericValue>> {
    let mut object = BTreeMap::new();
    for field in fields {
        if field.relation.is_some() {
            return Err(SourceError::Unsupported(
                "relations nested inside a joined field are not yet supported".into(),
            ));
        }
        object.insert(field.field.to_string(), assemble_scalar_or_nested(field, row)?);
    }
    Ok(object)
}

fn assemble_scalar_or_nested(
    field: &Field,
    row: &HashMap<String, GenericValue>,
) -> Result<GenericValue> {
    if let Some(column) = &field.column {
        let raw = row.get(column.as_ref()).cloned().unwrap_or(GenericValue::Null);
        Ok(finalize_scalar(raw, field))
    } else if let Some(nested) = &field.fields {
        Ok(GenericValue::Map(assemble_columns(nested, row)?))
    } else {
        Ok(field.default.clone().unwrap_or(GenericValue::Null))
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

fn is_soft_deleted(schema: &IndexSchema, root: &HashMap<String, GenericValue>) -> bool {
    let (value, when) = match &schema.soft_delete {
        None => return false,
        Some(SoftDelete::Column(c)) => (root.get(c.column.as_ref()), &c.when),
        Some(SoftDelete::Field(f)) => match field_column(&schema.fields, &f.field) {
            Some(column) => (root.get(column.as_ref()), &f.when),
            None => return false,
        },
    };
    if when.is_some() {
        tracing::warn!("soft_delete `when` filters are not yet evaluated; ignoring them");
    }
    soft_truthy(value)
}

/// A row counts as soft-deleted when the marker is a true boolean or a present
/// (non-null) value. Columns whose type does not decode (e.g. timestamps) read
/// as null and therefore as not-deleted — a known gap.
fn soft_truthy(value: Option<&GenericValue>) -> bool {
    match value {
        None | Some(GenericValue::Null) => false,
        Some(GenericValue::Bool(b)) => *b,
        Some(_) => true,
    }
}

fn field_column<'a>(fields: &'a [Field], name: &FieldName) -> Option<&'a ColumnName> {
    for field in fields {
        if &field.field == name {
            return field.column.as_ref();
        }
        if let Some(nested) = &field.fields
            && let Some(column) = field_column(nested, name)
        {
            return Some(column);
        }
    }
    None
}

/// The single primary-key value a join/aggregate matches its foreign key
/// against. Indexes with joins or aggregates must have a single-column key.
fn single_key_value(key: &RowKey) -> Result<&GenericValue> {
    match key.0.as_slice() {
        [(_, value)] => Ok(value),
        _ => Err(SourceError::Unsupported(
            "joins and aggregates require a single-column primary key".into(),
        )),
    }
}

fn push_unique(columns: &mut Vec<ColumnName>, column: &ColumnName) {
    if !columns.iter().any(|c| c == column) {
        columns.push(column.clone());
    }
}

fn query_err(error: sqlx::Error) -> SourceError {
    SourceError::Query(error.to_string())
}

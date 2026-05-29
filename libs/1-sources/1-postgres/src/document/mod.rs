//! [`PgDocumentBuilder`] — the read half of the Postgres source.
//!
//! Resolves which documents a changed row affects and assembles them from the
//! schema with sea-query + sqlx. Each relation is resolved with its own query
//! (see [`sql`]) and stitched into a [`GenericValue`] tree, recursing for
//! relations nested inside joined fields.
//!
//! ## Coverage
//!
//! - Resolution: root table; direct foreign-key relations; many-to-many
//!   (`through`) relations on either the far or junction table; and tables
//!   reachable through *multiple* hops of nesting, chained back to the root.
//! - Assembly: column fields (transforms, defaults); one-to-one / one-to-many /
//!   many-to-many joins (filters, ordering, limit); joins nested inside joins;
//!   aggregates, including over a junction; boolean and timestamp soft-delete
//!   with optional `when` filters; tombstones for missing rows.
//! - Decoding covers the common scalar types plus timestamps, dates, UUIDs, and
//!   JSON (carried as text / value trees).
//!
//! Relation targets are matched on each table's real primary key, looked up
//! from the Postgres catalog and cached (see
//! [`PgDocumentBuilder::table_primary_key`]). The index's own root key comes
//! from its declared `primary_key`.
//!
//! ## Remaining limits
//!
//! - A child-row *delete* on a related table can't be reverse-resolved from a
//!   key-only change (the row is already gone); this follows from the thin-event
//!   CDC design.
//! - Nested joins and multi-hop resolution issue one query per parent row / hop
//!   (N+1). `Raw` soft-delete `when` filters are not evaluated (logged and
//!   skipped); `LIKE`/`ILIKE` in `when` filters are matched approximately.

mod sql;
mod value;

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::str::FromStr;
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use rust_decimal::Decimal;
use schema_core::{
    Aggregate, ColumnName, Config, DatabaseSchema, Field, FieldName, FieldRelation, Filter,
    FilterOp, FilterValue, GenericValue, IndexSchema, Join, JoinKey, JoinType, NullOp, SoftDelete,
    TableName, Transform, ValueOpFilter,
};
use sea_query::{PostgresQueryBuilder, SelectStatement};
use sea_query_binder::SqlxBinder;
use sources_core::document::{Document, DocumentBuilder, DocumentId};
use sources_core::{Result, RowKey, SourceError};
use sqlx::{PgPool, Row};

/// Builds index documents from a Postgres database, driven by the loaded
/// [`Config`]. Cheap to clone — the pool, config, and primary-key cache are
/// shared.
#[derive(Debug, Clone)]
pub struct PgDocumentBuilder {
    pool: PgPool,
    config: Arc<Config>,
    /// Cache of each `(schema, table)`'s single-column primary key.
    pk_cache: Arc<Mutex<HashMap<(String, String), ColumnName>>>,
}

impl PgDocumentBuilder {
    /// Create a builder over a connection pool and the resolved config.
    pub fn new(pool: PgPool, config: Arc<Config>) -> Self {
        Self {
            pool,
            config,
            pk_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// The single-column primary key of a table, from the Postgres catalog
    /// (cached). Relations match against this, so a composite or missing
    /// primary key is an error.
    async fn table_primary_key(
        &self,
        schema: &DatabaseSchema,
        table: &TableName,
    ) -> Result<ColumnName> {
        let cache_key = (schema.to_string(), table.to_string());
        {
            let cache = self.pk_cache.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(column) = cache.get(&cache_key) {
                return Ok(column.clone());
            }
        }
        let column = match self.fetch_primary_key(schema, table).await?.as_slice() {
            [single] => single.clone(),
            [] => {
                return Err(SourceError::Query(format!(
                    "table `{schema}.{table}` has no primary key"
                )));
            }
            _ => {
                return Err(SourceError::Unsupported(format!(
                    "table `{schema}.{table}` has a composite primary key; relations require a single-column key"
                )));
            }
        };
        self.pk_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(cache_key, column.clone());
        Ok(column)
    }

    async fn fetch_primary_key(
        &self,
        schema: &DatabaseSchema,
        table: &TableName,
    ) -> Result<Vec<ColumnName>> {
        let qualified = format!("{schema}.{table}");
        let sql = "SELECT a.attname AS name \
                   FROM pg_index i \
                   JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey) \
                   WHERE i.indrelid = $1::regclass AND i.indisprimary";
        let rows = sqlx::query(sql)
            .bind(qualified)
            .fetch_all(&self.pool)
            .await
            .map_err(query_err)?;

        let mut columns = Vec::with_capacity(rows.len());
        for row in &rows {
            let name: String = row.try_get("name").map_err(query_err)?;
            columns.push(
                ColumnName::try_new(name)
                    .map_err(|e| SourceError::Query(format!("invalid primary key column: {e}")))?,
            );
        }
        Ok(columns)
    }

    /// Run a reverse-resolution query and collect the distinct, non-null values
    /// of its single selected column.
    async fn run_reverse(
        &self,
        query: SelectStatement,
        result_column: &str,
    ) -> Result<Vec<GenericValue>> {
        let (statement, values) = query.build_sqlx(PostgresQueryBuilder);
        let rows = sqlx::query_with(&statement, values)
            .fetch_all(&self.pool)
            .await
            .map_err(query_err)?;

        let mut seen = HashSet::new();
        let mut roots = Vec::new();
        for row in &rows {
            if let Some(value) = value::row_to_map(row).remove(result_column)
                && !matches!(value, GenericValue::Null)
                && seen.insert(value.clone())
            {
                roots.push(value);
            }
        }
        Ok(roots)
    }

    /// Resolve one path (root → … → changed table) back to root key values, by
    /// walking the relations from the changed table up to the root.
    async fn resolve_path(
        &self,
        schema: &IndexSchema,
        changed_table: &TableName,
        change_key: &RowKey,
        path: &[&FieldRelation],
    ) -> Result<Vec<GenericValue>> {
        let mut current_keys = vec![change_key.clone()];
        let mut current_table = changed_table.clone();

        for depth in (0..path.len()).rev() {
            let relation = *path
                .get(depth)
                .ok_or_else(|| SourceError::Query("internal: path index".into()))?;

            // The parent at this hop is the previous relation's table, or the
            // root table at the top.
            let parent_table = if depth == 0 {
                schema.table.clone()
            } else {
                let prev = *path
                    .get(depth - 1)
                    .ok_or_else(|| SourceError::Query("internal: path index".into()))?;
                relation_target(prev).0.clone()
            };
            let parent_pk = if depth == 0 {
                schema.primary_key.clone().ok_or_else(|| {
                    SourceError::Unsupported(
                        "index without primary_key cannot resolve relations".into(),
                    )
                })?
            } else {
                self.table_primary_key(&schema.db_schema, &parent_table).await?
            };

            let mut next = Vec::new();
            let mut seen = HashSet::new();
            for key in &current_keys {
                for value in self.reverse_hop(&schema.db_schema, relation, &current_table, key).await? {
                    if seen.insert(value.clone()) {
                        next.push(RowKey(vec![(parent_pk.clone(), value)]));
                    }
                }
            }
            current_keys = next;
            current_table = parent_table;
        }

        Ok(current_keys
            .into_iter()
            .filter_map(|key| key.0.into_iter().next().map(|(_, value)| value))
            .collect())
    }

    /// One reverse hop: from a key in `current_table`, find the parent key
    /// values via `relation`.
    async fn reverse_hop(
        &self,
        schema: &DatabaseSchema,
        relation: &FieldRelation,
        current_table: &TableName,
        key: &RowKey,
    ) -> Result<Vec<GenericValue>> {
        let (target, join_key) = relation_target(relation);
        match join_key {
            JoinKey::Direct(foreign_key) => {
                self.reverse_direct(schema, target, foreign_key, key).await
            }
            JoinKey::Through(through) if *current_table == through.table => {
                // The change is on the junction table itself.
                self.reverse_through_junction(schema, &through.table, &through.left_key, key)
                    .await
            }
            JoinKey::Through(through) => {
                // The key is in the far table; reach roots across the junction.
                self.reverse_through_far(
                    schema,
                    &through.table,
                    &through.left_key,
                    &through.right_key,
                    key,
                )
                .await
            }
        }
    }

    /// Direct foreign key: the child row holds the parent key in `foreign_key`.
    /// A child *delete* finds nothing — its row is already gone.
    async fn reverse_direct(
        &self,
        schema: &DatabaseSchema,
        child: &TableName,
        foreign_key: &ColumnName,
        child_key: &RowKey,
    ) -> Result<Vec<GenericValue>> {
        let query = sql::reverse_select(schema, child, foreign_key, &child_key.0)?;
        self.run_reverse(query, foreign_key.as_ref()).await
    }

    /// Many-to-many, key in the far table: it matches `right_key` in the
    /// junction, and the parents are the junction's `left_key` values.
    async fn reverse_through_far(
        &self,
        schema: &DatabaseSchema,
        junction: &TableName,
        left_key: &ColumnName,
        right_key: &ColumnName,
        far_key: &RowKey,
    ) -> Result<Vec<GenericValue>> {
        let far_pk = single_far_key(far_key)?.clone();
        let query = sql::reverse_select(schema, junction, left_key, &[(right_key.clone(), far_pk)])?;
        self.run_reverse(query, left_key.as_ref()).await
    }

    /// Many-to-many, change on the junction itself: if the key already carries
    /// `left_key` (a composite junction key) use it directly — which also
    /// handles deletes — otherwise look it up by the junction key.
    async fn reverse_through_junction(
        &self,
        schema: &DatabaseSchema,
        junction: &TableName,
        left_key: &ColumnName,
        junction_key: &RowKey,
    ) -> Result<Vec<GenericValue>> {
        if let Some((_, value)) = junction_key.0.iter().find(|(column, _)| column == left_key) {
            return Ok(vec![value.clone()]);
        }
        let query = sql::reverse_select(schema, junction, left_key, &junction_key.0)?;
        self.run_reverse(query, left_key.as_ref()).await
    }

    /// Assemble a row's fields. `current_pk` is this row's primary-key value,
    /// used as the parent key for any relation among `fields`.
    async fn assemble_row(
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

            // Change on a related table: resolve every path back to the root.
            let mut paths = Vec::new();
            let mut prefix = Vec::new();
            find_paths(&schema.fields, table, &mut prefix, &mut paths);
            if paths.is_empty() {
                continue;
            }
            let Some(pk_column) = schema.primary_key.clone() else {
                tracing::warn!(
                    index = %name, table = %table,
                    "cannot reverse-resolve: index has no primary_key",
                );
                continue;
            };

            let mut seen = HashSet::new();
            for path in &paths {
                for root in self.resolve_path(schema, table, key, path).await? {
                    if seen.insert(root.clone()) {
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

        // The root's primary-key value keys any top-level relations.
        let root_pk = match &schema.primary_key {
            Some(pk) => primary_key_value(&id.key, pk)?.clone(),
            None => GenericValue::Null,
        };
        let body = self.assemble_row(&schema.db_schema, &schema.fields, &root, &root_pk).await?;
        Ok(Document::Upsert {
            id: id.clone(),
            body: GenericValue::Map(body),
        })
    }
}

/// The target table and key of a relation.
fn relation_target(relation: &FieldRelation) -> (&TableName, &JoinKey) {
    match relation {
        FieldRelation::Join(join) => (&join.table, &join.key),
        FieldRelation::Aggregate(aggregate) => (&aggregate.table, &aggregate.key),
    }
}

/// Collect every relation path from the root down to `table`, at any depth.
/// `prefix` is the chain of relations to the current point; a same-row group
/// adds no hop, a relation does.
fn find_paths<'a>(
    fields: &'a [Field],
    table: &TableName,
    prefix: &mut Vec<&'a FieldRelation>,
    out: &mut Vec<Vec<&'a FieldRelation>>,
) {
    for field in fields {
        match &field.relation {
            Some(relation) => {
                prefix.push(relation);
                let (target, key) = relation_target(relation);
                let hit = target == table
                    || matches!(key, JoinKey::Through(through) if through.table == *table);
                if hit {
                    out.push(prefix.clone());
                }
                if let Some(nested) = &field.fields {
                    find_paths(nested, table, prefix, out);
                }
                prefix.pop();
            }
            None => {
                if let Some(nested) = &field.fields {
                    find_paths(nested, table, prefix, out);
                }
            }
        }
    }
}

/// Whether any relation appears among these fields (or their same-row groups).
fn contains_relation(fields: &[Field]) -> bool {
    fields.iter().any(|field| {
        field.relation.is_some()
            || matches!(&field.fields, Some(nested) if field.relation.is_none() && contains_relation(nested))
    })
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

/// Collect the columns of column-backed fields (recursing into same-row groups,
/// skipping relations — those are fetched by their own queries).
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
    let (marker, when) = match &schema.soft_delete {
        None => return false,
        Some(SoftDelete::Column(c)) => (root.get(c.column.as_ref()), c.when.as_deref()),
        Some(SoftDelete::Field(f)) => match field_column(&schema.fields, &f.field) {
            Some(column) => (root.get(column.as_ref()), f.when.as_deref()),
            None => return false,
        },
    };
    if !soft_truthy(marker) {
        return false;
    }
    match when {
        None => true,
        Some(filters) => when_matches(filters, root),
    }
}

/// A row counts as soft-deleted when the marker is a true boolean or a present
/// (non-null) value.
fn soft_truthy(value: Option<&GenericValue>) -> bool {
    match value {
        None | Some(GenericValue::Null) => false,
        Some(GenericValue::Bool(b)) => *b,
        Some(_) => true,
    }
}

/// Evaluate soft-delete `when` filters against the root row (an AND of all).
fn when_matches(filters: &[Filter], row: &HashMap<String, GenericValue>) -> bool {
    filters.iter().all(|filter| filter_matches(filter, row))
}

fn filter_matches(filter: &Filter, row: &HashMap<String, GenericValue>) -> bool {
    match filter {
        Filter::Raw(_) => {
            tracing::warn!("raw soft_delete `when` filters are not evaluated; ignoring");
            true
        }
        Filter::NullCheck(check) => {
            let is_null = matches!(row.get(check.column.as_ref()), None | Some(GenericValue::Null));
            match check.op {
                NullOp::IsNull => is_null,
                NullOp::IsNotNull => !is_null,
            }
        }
        Filter::ValueOp(op) => value_op_matches(op, row.get(op.column.as_ref())),
    }
}

fn value_op_matches(filter: &ValueOpFilter, value: Option<&GenericValue>) -> bool {
    let Some(text) = value.and_then(scalar_to_string) else {
        return false; // null or non-scalar never matches a value comparison
    };
    match (&filter.op, &filter.value) {
        (FilterOp::Eq, FilterValue::Single(v)) => text == *v,
        (FilterOp::Neq, FilterValue::Single(v)) => text != *v,
        (FilterOp::In, FilterValue::List(vs)) => vs.contains(&text),
        (FilterOp::NotIn, FilterValue::List(vs)) => !vs.contains(&text),
        (FilterOp::Lt, FilterValue::Single(v)) => compare(&text, v) == Ordering::Less,
        (FilterOp::Lte, FilterValue::Single(v)) => compare(&text, v) != Ordering::Greater,
        (FilterOp::Gt, FilterValue::Single(v)) => compare(&text, v) == Ordering::Greater,
        (FilterOp::Gte, FilterValue::Single(v)) => compare(&text, v) != Ordering::Less,
        (FilterOp::Between, FilterValue::Range(lo, hi)) => {
            compare(&text, lo) != Ordering::Less && compare(&text, hi) != Ordering::Greater
        }
        (FilterOp::Like, FilterValue::Single(v)) => like_match(&text, v, false),
        (FilterOp::Ilike, FilterValue::Single(v)) => like_match(&text, v, true),
        _ => false,
    }
}

/// Compare numerically when both sides parse as decimals, else lexically.
fn compare(a: &str, b: &str) -> Ordering {
    match (Decimal::from_str(a), Decimal::from_str(b)) {
        (Ok(x), Ok(y)) => x.cmp(&y),
        _ => a.cmp(b),
    }
}

/// Approximate SQL `LIKE`: handles leading/trailing `%`; ignores `_` and
/// interior wildcards.
fn like_match(text: &str, pattern: &str, case_insensitive: bool) -> bool {
    let (text, pattern) = if case_insensitive {
        (text.to_lowercase(), pattern.to_lowercase())
    } else {
        (text.to_owned(), pattern.to_owned())
    };
    let core = pattern.trim_matches('%');
    match (pattern.starts_with('%'), pattern.ends_with('%')) {
        (true, true) => text.contains(core),
        (true, false) => text.ends_with(core),
        (false, true) => text.starts_with(core),
        (false, false) => text == core,
    }
}

fn scalar_to_string(value: &GenericValue) -> Option<String> {
    match value {
        GenericValue::Bool(b) => Some(b.to_string()),
        GenericValue::Int(i) => Some(i.to_string()),
        GenericValue::Decimal(d) => Some(d.to_string()),
        GenericValue::String(s) => Some(s.clone()),
        GenericValue::Null | GenericValue::Array(_) | GenericValue::Map(_) => None,
    }
}

/// The value of the index's primary-key column within a document key. Supports
/// composite keys: relations match on the declared primary key.
fn primary_key_value<'a>(key: &'a RowKey, primary_key: &ColumnName) -> Result<&'a GenericValue> {
    key.0
        .iter()
        .find(|(column, _)| column == primary_key)
        .map(|(_, value)| value)
        .ok_or_else(|| {
            SourceError::Query(format!("document key is missing primary key column `{primary_key}`"))
        })
}

/// The single value of a far/junction row's key, for matching a junction's
/// `right_key`. Through relations require a single-column key on the far side.
fn single_far_key(key: &RowKey) -> Result<&GenericValue> {
    match key.0.as_slice() {
        [(_, value)] => Ok(value),
        _ => Err(SourceError::Unsupported(
            "many-to-many relations require a single-column key on the far/junction table".into(),
        )),
    }
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

fn push_unique(columns: &mut Vec<ColumnName>, column: &ColumnName) {
    if !columns.iter().any(|c| c == column) {
        columns.push(column.clone());
    }
}

fn query_err(error: sqlx::Error) -> SourceError {
    SourceError::Query(error.to_string())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use schema_core::NullCheckFilter;

    fn col(name: &str) -> ColumnName {
        ColumnName::try_new(name).unwrap()
    }

    fn row(pairs: &[(&str, GenericValue)]) -> HashMap<String, GenericValue> {
        pairs.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect()
    }

    fn value_op(column: &str, op: FilterOp, value: FilterValue) -> ValueOpFilter {
        ValueOpFilter {
            column: col(column),
            op,
            value,
        }
    }

    #[test]
    fn compare_is_numeric_then_lexical() {
        assert_eq!(compare("9", "10"), Ordering::Less); // numeric, not "9" > "1…"
        assert_eq!(compare("b", "a"), Ordering::Greater); // lexical fallback
    }

    #[test]
    fn like_match_handles_anchors_and_case() {
        assert!(like_match("hello world", "%world", false));
        assert!(like_match("hello", "hel%", false));
        assert!(like_match("HELLO", "%ell%", true));
        assert!(!like_match("hello", "bye", false));
    }

    #[test]
    fn value_op_eq_in_and_between() {
        let active = GenericValue::String("active".into());
        assert!(value_op_matches(
            &value_op("status", FilterOp::Eq, FilterValue::Single("active".into())),
            Some(&active),
        ));
        assert!(value_op_matches(
            &value_op(
                "status",
                FilterOp::In,
                FilterValue::List(vec!["a".into(), "active".into()]),
            ),
            Some(&active),
        ));
        let between = value_op(
            "n",
            FilterOp::Between,
            FilterValue::Range("1".into(), "10".into()),
        );
        assert!(value_op_matches(&between, Some(&GenericValue::Int(5))));
        assert!(!value_op_matches(&between, Some(&GenericValue::Int(20))));
    }

    #[test]
    fn value_op_null_never_matches() {
        let eq = value_op("x", FilterOp::Eq, FilterValue::Single("v".into()));
        assert!(!value_op_matches(&eq, None));
        assert!(!value_op_matches(&eq, Some(&GenericValue::Null)));
    }

    #[test]
    fn when_matches_is_conjunction() {
        let r = row(&[
            ("status", GenericValue::String("deleted".into())),
            ("n", GenericValue::Int(3)),
        ]);
        let filters = vec![
            Filter::ValueOp(value_op(
                "status",
                FilterOp::Eq,
                FilterValue::Single("deleted".into()),
            )),
            Filter::ValueOp(value_op("n", FilterOp::Gte, FilterValue::Single("2".into()))),
        ];
        assert!(when_matches(&filters, &r));

        let null_check = vec![Filter::NullCheck(NullCheckFilter {
            column: col("missing"),
            op: NullOp::IsNull,
        })];
        assert!(when_matches(&null_check, &r));
    }

    #[test]
    fn soft_truthy_treats_present_or_true_as_deleted() {
        assert!(!soft_truthy(None));
        assert!(!soft_truthy(Some(&GenericValue::Null)));
        assert!(!soft_truthy(Some(&GenericValue::Bool(false))));
        assert!(soft_truthy(Some(&GenericValue::Bool(true))));
        // e.g. a non-null deleted_at timestamp decoded as text
        assert!(soft_truthy(Some(&GenericValue::String("2024-01-01T00:00:00Z".into()))));
    }
}

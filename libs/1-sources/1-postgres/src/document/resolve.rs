//! Reverse resolution: given a changed row, find the keys of the root documents
//! it affects by walking relation paths back up to the root.

use std::collections::HashSet;

use schema_core::{
    ColumnName, DatabaseSchema, GenericValue, IndexSchema, JoinKey, Relation, TableName,
};
use sources_core::{Result, RowKey, SourceError};

use super::fields::relation_target;
use super::{PgDocumentBuilder, query, query_err, value};

impl PgDocumentBuilder {
    /// Resolve one path (root → … → changed table) back to root key values, by
    /// walking the relations from the changed table up to the root.
    pub(super) async fn resolve_path(
        &self,
        schema: &IndexSchema,
        changed_table: &TableName,
        change_key: &RowKey,
        path: &[&Relation],
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
                self.table_primary_key(&schema.db_schema, &parent_table)
                    .await?
            };

            let mut next = Vec::new();
            let mut seen = HashSet::new();
            for key in &current_keys {
                for value in self
                    .reverse_hop(&schema.db_schema, relation, &current_table, key)
                    .await?
                {
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
        relation: &Relation,
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
        let (sql, params) = query::reverse_query(schema, child, foreign_key, &child_key.0)?;
        self.run_reverse(sql, params, foreign_key.as_ref()).await
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
        let (sql, params) =
            query::reverse_query(schema, junction, left_key, &[(right_key.clone(), far_pk)])?;
        self.run_reverse(sql, params, left_key.as_ref()).await
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
        let (sql, params) = query::reverse_query(schema, junction, left_key, &junction_key.0)?;
        self.run_reverse(sql, params, left_key.as_ref()).await
    }

    /// Run a reverse query and collect the distinct, non-null values of its
    /// single selected column.
    async fn run_reverse(
        &self,
        sql: query::SqlString,
        params: Vec<GenericValue>,
        result_column: &str,
    ) -> Result<Vec<GenericValue>> {
        let mut query = sqlx::query(sql);
        for param in &params {
            query = query::bind_param(query, param)?;
        }
        let rows = query.fetch_all(&self.pool).await.map_err(query_err)?;

        let mut seen = HashSet::new();
        let mut roots = Vec::new();
        for row in &rows {
            let value = value::decode_named_column(row, result_column);
            if !matches!(value, GenericValue::Null) && seen.insert(value.clone()) {
                roots.push(value);
            }
        }
        Ok(roots)
    }
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

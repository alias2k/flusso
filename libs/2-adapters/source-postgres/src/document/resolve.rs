//! Reverse resolution: given a changed row, find the keys of the root documents
//! it affects by walking relation paths back up to the root.
//!
//! A path is a list of [`Step`]s: one per relation, plus one per table of an
//! aggregate's `via`, each a plain foreign-key hop to the table before it.
//!
//! Every hop reads the **current** database, except one input to the first: when
//! the changed row itself holds the link to its parent (a `has_one`/`has_many`
//! child's or a `via` table's `foreign_key`, a junction's `left_key`), the parent is also read from
//! the change's pre-image. That is how a deleted child — whose row is gone — and
//! a re-parented one — whose row names the new parent — still rebuild the
//! document they used to sit in. Without a pre-image (a table whose replica
//! identity doesn't carry the link column) only the current parent is found.
//!
//! When the changed row's key itself carries that link column, the parent is
//! read straight from the key, with no query: the value a reverse query would
//! return for that row, and still there once the row is deleted. That is how
//! a table without a primary key resolves: under `REPLICA IDENTITY FULL` its
//! change is keyed by every column, so its link is in the key, and a null in
//! another column is ordinary row data. A null link links no parent.
//!
//! A hop's `on` pairs are not applied: the key alone finds every parent a row
//! is linked to, a superset of the ones it joins, and each rebuild evaluates
//! the pairs. A row that only changes a `related` column keeps its key, so the
//! parent it left is still found.

use std::collections::{HashMap, HashSet};

use kernel::{
    ColumnName, DatabaseSchema, GenericValue, IndexName, IndexSchema, Relation, RelationKey,
    TableName,
};
use source::document::DocumentId;
use source::{Result, RowImage, RowKey, SourceError};

use super::fields::Step;
use super::{PgDocumentBuilder, query, query_err, value};

impl PgDocumentBuilder {
    /// Resolve one path (root → … → changed table) back to root key values, by
    /// walking the relations from the changed table up to the root. `before`
    /// feeds only the first hop: rows further up are matched by their current
    /// state. A key carrying the hop's link column names the parent itself; a
    /// key with a null part and no link in it matches no stored row, so its
    /// hop reads only `before` (see [`reverse_hop`](Self::reverse_hop)).
    pub(super) async fn resolve_path(
        &self,
        schema: &IndexSchema,
        changed_table: &TableName,
        change_key: &RowKey,
        before: Option<&RowImage>,
        path: &[Step<'_>],
    ) -> Result<Vec<GenericValue>> {
        let mut current_keys = vec![change_key.clone()];
        let mut current_table = changed_table.clone();
        let mut origin: Option<(TableName, ColumnName)> = None;

        for depth in (0..path.len()).rev() {
            let step = *path
                .get(depth)
                .ok_or_else(|| SourceError::Query("internal: path index".into()))?;

            let parent_table = if depth == 0 {
                schema.table.clone()
            } else {
                let prev = *path
                    .get(depth - 1)
                    .ok_or_else(|| SourceError::Query("internal: path index".into()))?;
                prev.table().clone()
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

            let old_link = before
                .filter(|_| depth + 1 == path.len())
                .zip(link_on_row(step, &current_table));
            let mut next = Vec::new();
            let mut seen = HashSet::new();
            for key in &current_keys {
                for value in self
                    .reverse_hop(
                        &schema.db_schema,
                        step,
                        &current_table,
                        (&parent_table, &parent_pk),
                        key,
                        origin.as_ref(),
                    )
                    .await
                    .map(|parents| match old_link {
                        Some((image, link)) => with_old_parent(parents, image, link),
                        None => parents,
                    })?
                {
                    if seen.insert(value.clone()) {
                        next.push(RowKey(vec![(parent_pk.clone(), value)]));
                    }
                }
            }
            current_keys = next;
            current_table = parent_table;
            origin = step
                .row_link()
                .map(|(table, column)| (table.clone(), column.clone()));
        }

        Ok(current_keys
            .into_iter()
            .filter_map(|key| key.0.into_iter().next().map(|(_, value)| value))
            .collect())
    }

    /// One reverse hop: from a key in `current_table`, find the parent key
    /// values via `step`. `parent` (table and key column) names the hop's parent
    /// side — a `belongs_to` stores its key *there*, so its reverse query runs
    /// against the parent table rather than the changed one.
    ///
    /// A key that carries the hop's link column (see [`link_on_row`]) answers
    /// without a query. Otherwise a key with a null part runs none: it matches
    /// no stored row, and binding the null would fail the query. Only a table
    /// with a primary key reaches that, when its old row left the key out (a
    /// partition decoded under its partitioned parent); its parent comes from
    /// `before` alone, since such an update carries no post-image. A table
    /// without one never does on an index that builds: under `FULL` its key is
    /// every column, so a link on its row is in the key, and any other relation
    /// over it fails every document build first ("has no primary key").
    async fn reverse_hop(
        &self,
        schema: &DatabaseSchema,
        step: Step<'_>,
        current_table: &TableName,
        parent: (&TableName, &ColumnName),
        key: &RowKey,
        origin: Origin<'_>,
    ) -> Result<Vec<GenericValue>> {
        if let Some(link) = link_on_row(step, current_table)
            && let Some((_, value)) = key.0.iter().find(|(column, _)| column == link)
        {
            return Ok(match value {
                GenericValue::Null => Vec::new(),
                value => vec![value.clone()],
            });
        }
        if key.has_null() {
            return Ok(Vec::new());
        }
        match step.key() {
            RelationKey::Through(through) if *current_table == through.table => {
                self.reverse_through_junction(
                    schema,
                    &through.table,
                    &through.left_key,
                    key,
                    origin,
                )
                .await
            }
            RelationKey::Direct(foreign_key) => {
                self.reverse_direct(schema, step.table(), foreign_key, key, origin)
                    .await
            }
            RelationKey::Local(column) => {
                let Step::Relation(relation) = step else {
                    return Err(SourceError::Query(
                        "internal: a `via` hop is never local".into(),
                    ));
                };
                let target = (current_table, origin);
                self.reverse_local(schema, relation, parent, column, key, target)
                    .await
            }
            RelationKey::Through(through) => {
                self.reverse_through_far(
                    schema,
                    &through.table,
                    &through.left_key,
                    &through.right_key,
                    key,
                    (current_table, origin),
                )
                .await
            }
        }
    }

    /// `belongs_to`: the **parent** rows hold the changed row's key in
    /// `column`, so the referrers are found on the parent table itself —
    /// `SELECT parent_pk FROM parent WHERE column = <changed pk>`. Unlike a
    /// direct-FK child delete, a deleted *target* still resolves: the parent
    /// rows pointing at it are alive and rebuild with a null object.
    async fn reverse_local(
        &self,
        schema: &DatabaseSchema,
        relation: &Relation,
        (parent_table, parent_pk): (&TableName, &ColumnName),
        column: &ColumnName,
        target_key: &RowKey,
        (target_table, origin): (&TableName, Origin<'_>),
    ) -> Result<Vec<GenericValue>> {
        let (target_column, value) = local_target_value(relation, target_key)?;
        let key = [(column.clone(), value.clone())];
        let col_types = self
            .cast_types(schema, parent_table, &key, |_| {
                origin
                    .cloned()
                    .unwrap_or_else(|| (target_table.clone(), target_column.clone()))
            })
            .await?;
        let base = self.base_name(schema, parent_table, parent_pk).await?;
        let (sql, params) =
            query::reverse_query(schema, parent_table, parent_pk, &base, &key, &col_types)?;
        self.run_reverse(sql, params, parent_pk.as_ref()).await
    }

    /// Direct foreign key: the child row holds the parent key in `foreign_key`.
    /// A child *delete* finds nothing here — its row is already gone; the old
    /// parent comes from the pre-image instead (see [`with_old_parent`]).
    async fn reverse_direct(
        &self,
        schema: &DatabaseSchema,
        child: &TableName,
        foreign_key: &ColumnName,
        child_key: &RowKey,
        origin: Origin<'_>,
    ) -> Result<Vec<GenericValue>> {
        let col_types = self
            .cast_types(schema, child, &child_key.0, |column| {
                origin
                    .cloned()
                    .unwrap_or_else(|| (child.clone(), column.clone()))
            })
            .await?;
        let base = self.base_name(schema, child, foreign_key).await?;
        let (sql, params) =
            query::reverse_query(schema, child, foreign_key, &base, &child_key.0, &col_types)?;
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
        (far_table, origin): (&TableName, Origin<'_>),
    ) -> Result<Vec<GenericValue>> {
        let (far_column, far_pk) = single_far_key(far_key)?;
        let key = [(right_key.clone(), far_pk.clone())];
        let col_types = self
            .cast_types(schema, junction, &key, |_| {
                origin
                    .cloned()
                    .unwrap_or_else(|| (far_table.clone(), far_column.clone()))
            })
            .await?;
        let base = self.base_name(schema, junction, left_key).await?;
        let (sql, params) =
            query::reverse_query(schema, junction, left_key, &base, &key, &col_types)?;
        self.run_reverse(sql, params, left_key.as_ref()).await
    }

    /// Many-to-many, change on the junction itself, keyed without `left_key`
    /// (a surrogate key): look `left_key` up by the junction key. A key that
    /// carries it never gets here (see [`reverse_hop`](Self::reverse_hop)).
    async fn reverse_through_junction(
        &self,
        schema: &DatabaseSchema,
        junction: &TableName,
        left_key: &ColumnName,
        junction_key: &RowKey,
        origin: Origin<'_>,
    ) -> Result<Vec<GenericValue>> {
        let col_types = self
            .cast_types(schema, junction, &junction_key.0, |column| {
                origin
                    .cloned()
                    .unwrap_or_else(|| (junction.clone(), column.clone()))
            })
            .await?;
        let base = self.base_name(schema, junction, left_key).await?;
        let (sql, params) = query::reverse_query(
            schema,
            junction,
            left_key,
            &base,
            &junction_key.0,
            &col_types,
        )?;
        self.run_reverse(sql, params, left_key.as_ref()).await
    }

    /// The type each key value on `table` is cast to (`= $n::<type>`): the
    /// catalog type of the column the value was read from, `source(column)`,
    /// so the comparison is the one the document query's join makes between
    /// the two columns (a `char(n)` link against a `text` key compares as
    /// `text`, padding trimmed). A value read from the column it is compared
    /// with is cast to that column's own type.
    async fn cast_types(
        &self,
        db: &DatabaseSchema,
        table: &TableName,
        key: &[(ColumnName, GenericValue)],
        source: impl Fn(&ColumnName) -> (TableName, ColumnName),
    ) -> Result<HashMap<(String, String), String>> {
        let mut types = HashMap::new();
        for (column, _) in key {
            let entry = (table.to_string(), column.to_string());
            if types.contains_key(&entry) {
                continue;
            }
            let (from_table, from_column) = source(column);
            types.insert(
                entry,
                self.column_type(db, &from_table, &from_column).await?,
            );
        }
        Ok(types)
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
            let value = value::decode_key(row, result_column);
            if !matches!(value, GenericValue::Null) && seen.insert(value.clone()) {
                roots.push(value);
            }
        }
        Ok(roots)
    }
}

/// The link values a batch's paths ended on, per index and link column: each
/// is read back as the root row's own key by
/// [`read_root_links`](PgDocumentBuilder::read_root_links), one query per
/// index and link column for the whole batch.
#[derive(Debug, Default)]
pub(super) struct RootLinks(HashMap<(IndexName, Link), (Vec<GenericValue>, HashSet<GenericValue>)>);

/// A link column: the table that holds it, and its name.
pub(super) type Link = (TableName, ColumnName);

impl RootLinks {
    pub(super) fn add(&mut self, index: &IndexName, link: Link, links: Vec<GenericValue>) {
        let (ordered, seen) = self.0.entry((index.clone(), link)).or_default();
        for link in links {
            if seen.insert(link.clone()) {
                ordered.push(link);
            }
        }
    }
}

impl PgDocumentBuilder {
    /// The documents `links` name: per index, the root rows' own keys.
    pub(super) async fn read_root_links(&self, links: RootLinks) -> Result<Vec<DocumentId>> {
        let mut ids = Vec::new();
        for ((index, link), (links, _)) in links.0 {
            let Some(schema) = self.spec.schema(&index) else {
                continue;
            };
            let Some(pk) = schema.primary_key.as_ref() else {
                continue;
            };
            for root in self.root_keys(schema, pk, &link, &links).await? {
                ids.push(DocumentId {
                    index: index.clone(),
                    key: RowKey(vec![(pk.clone(), root)]),
                });
            }
        }
        Ok(ids)
    }

    /// The link column `path` ends on, when the root keys it resolves to
    /// are that column's values rather than the root row's own key and may be
    /// spelled differently: a key whose equal values render differently (see
    /// `ColumnMeta::equal_is_identical`), or a link column of another type
    /// than the key. Those go through [`root_keys`](Self::root_keys). A
    /// `belongs_to` from the root reads the root's own key: never.
    pub(super) async fn reads_root_from_a_link(
        &self,
        schema: &IndexSchema,
        pk: &ColumnName,
        path: &[Step<'_>],
    ) -> Result<Option<Link>> {
        let Some((table, column)) = path.first().and_then(|step| step.row_link()) else {
            return Ok(None);
        };
        let root = self
            .column_meta(&schema.db_schema, &schema.table, pk)
            .await?;
        let link = self.column_meta(&schema.db_schema, table, column).await?;
        Ok(
            (!root.equal_is_identical() || link.sql_type != root.sql_type)
                .then(|| (table.clone(), column.clone())),
        )
    }

    /// The root rows' own keys for `links`, values read from the `link` column
    /// that compare equal to them as the document query's join compares that
    /// column with the key: the document ids they name, spelled as the backfill
    /// and the root's own changes spell them. A link no root row equals names
    /// no document.
    pub(super) async fn root_keys(
        &self,
        schema: &IndexSchema,
        pk: &ColumnName,
        (link_table, link_column): &Link,
        links: &[GenericValue],
    ) -> Result<Vec<GenericValue>> {
        if links.is_empty() {
            return Ok(Vec::new());
        }
        let sql_type = self
            .column_type(&schema.db_schema, link_table, link_column)
            .await?;
        let base = self.base_name(&schema.db_schema, &schema.table, pk).await?;
        let (sql, params) = query::root_keys_query(
            &schema.db_schema,
            &schema.table,
            pk,
            &base,
            links,
            &sql_type,
        )?;
        self.run_reverse(sql, params, pk.as_ref()).await
    }
}

/// The column a hop's key values were read from, when not the key's own:
/// the previous hop's link column.
type Origin<'a> = Option<&'a (TableName, ColumnName)>;

/// The column on `row_table` that names its parent across `step`, when the
/// link lives on that row: a direct child's or `via` table's `foreign_key`, or
/// a junction's `left_key` when the junction itself changed. Read from the
/// pre-image on the first hop, and from the key on any hop that carries it.
fn link_on_row<'a>(step: Step<'a>, row_table: &TableName) -> Option<&'a ColumnName> {
    step.row_link()
        .filter(|(table, _)| *table == row_table)
        .map(|(_, column)| column)
}

/// The parents found in the current database, plus the one the pre-image's
/// `link` column named, if it carried a non-null one not already found.
fn with_old_parent(
    mut parents: Vec<GenericValue>,
    before: &RowImage,
    link: &ColumnName,
) -> Vec<GenericValue> {
    if let Some(old) = before.get(link)
        && !matches!(old, GenericValue::Null)
        && !parents.contains(old)
    {
        parents.push(old.clone());
    }
    parents
}

/// The changed target row's primary-key value, for matching a `belongs_to`
/// parent's `column`. Prefer the entry named by the join's declared
/// `primary_key`; a single-column key is used as-is.
fn local_target_value<'a>(
    relation: &Relation,
    key: &'a RowKey,
) -> Result<(&'a ColumnName, &'a GenericValue)> {
    if let Relation::Join(join) = relation
        && let Some((column, value)) = key.0.iter().find(|(column, _)| *column == join.primary_key)
    {
        return Ok((column, value));
    }
    match key.0.as_slice() {
        [(column, value)] => Ok((column, value)),
        _ => Err(SourceError::Unsupported(
            "belongs_to relations require the changed row's key to carry its primary key".into(),
        )),
    }
}

/// The single value of a far/junction row's key, for matching a junction's
/// `right_key`. Through relations require a single-column key on the far side.
fn single_far_key(key: &RowKey) -> Result<(&ColumnName, &GenericValue)> {
    match key.0.as_slice() {
        [(column, value)] => Ok((column, value)),
        _ => Err(SourceError::Unsupported(
            "many-to-many relations require a single-column key on the far/junction table".into(),
        )),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;

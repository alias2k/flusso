//! [`SourceSpec`] — the source's own view of what to build.
//!
//! A source needs only a fraction of the top-level `Config`: the indexes it
//! must keep in sync and the shape of each (root table, field sources, filters).
//! It needs neither the sink list nor any connection or OpenSearch detail. That
//! genuine subset is [`SourceSpec`], expressed entirely in [`schema_core`] types
//! the source already speaks.
//!
//! The composition root translates the top-level config into a `SourceSpec` and
//! hands the backend the spec, so the backend knows nothing about how the
//! application is configured. `SourceSpec` is expressed purely in `schema-core`
//! vocabulary and never references the assembled `Config` (which lives a layer
//! up), so a source is reusable and unit-testable without constructing one, and
//! the `flusso.toml` shape can evolve without recompiling the backend.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use schema_core::{
    DatabaseSchema, Field, IndexMapping, IndexName, IndexSchema, RelationKey, TableName,
};

/// A schema-qualified table, the unit a source needs to reason about coverage
/// (which tables it must be able to stream). Ordered by `(schema, table)` so a
/// [`BTreeSet`] of them is deterministic.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct QualifiedTable {
    pub schema: DatabaseSchema,
    pub table: TableName,
}

impl QualifiedTable {
    pub fn new(schema: DatabaseSchema, table: TableName) -> Self {
        Self { schema, table }
    }
}

impl fmt::Display for QualifiedTable {
    /// Renders `schema.table` — the form Postgres accepts in `FOR TABLE` lists.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.schema.as_ref(), self.table.as_ref())
    }
}

impl PartialOrd for QualifiedTable {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QualifiedTable {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.schema.as_ref(), self.table.as_ref())
            .cmp(&(other.schema.as_ref(), other.table.as_ref()))
    }
}

/// The enabled indexes a source must build, each paired with its schema.
///
/// Everything here is treated as live — disabled indexes are dropped by the
/// composition root when it translates the config into this spec, so the source
/// never has to re-check an `enabled` flag.
#[derive(Debug, Clone, Default)]
pub struct SourceSpec {
    indexes: BTreeMap<IndexName, IndexSchema>,
}

impl SourceSpec {
    /// Build a spec from an explicit set of `(index, schema)` pairs. Every entry
    /// is treated as live; the caller (the composition root) is responsible for
    /// having filtered out disabled indexes when translating from a config.
    pub fn new(indexes: BTreeMap<IndexName, IndexSchema>) -> Self {
        Self { indexes }
    }

    /// Iterate the indexes and their schemas, in index-name order.
    pub fn indexes(&self) -> impl Iterator<Item = (&IndexName, &IndexSchema)> {
        self.indexes.iter()
    }

    pub fn schema(&self, index: &IndexName) -> Option<&IndexSchema> {
        self.indexes.get(index)
    }

    /// Project every index into its fully-typed [`IndexMapping`], using only the
    /// declared schema — the database-free counterpart the engine creates
    /// indexes from up front.
    pub fn index_mappings(&self) -> Vec<IndexMapping> {
        self.indexes
            .iter()
            .map(|(name, schema)| schema.resolve(name.clone()))
            .collect()
    }

    /// Every table any enabled index reads — each index's root table plus every
    /// table a join or aggregate (and any `through` junction) pulls from.
    ///
    /// Relations carry no schema of their own; the resolver qualifies a related
    /// table with the *index's* `db_schema` (see the Postgres `resolve` module),
    /// so this does the same. This is the set a source must be able to stream —
    /// what [`CaptureProvisioning`](crate::CaptureProvisioning) checks coverage
    /// against.
    pub fn all_tables(&self) -> BTreeSet<QualifiedTable> {
        let mut tables = BTreeSet::new();
        for schema in self.indexes.values() {
            tables.insert(QualifiedTable::new(
                schema.db_schema.clone(),
                schema.table.clone(),
            ));
            collect_relation_tables(&schema.fields, &schema.db_schema, &mut tables);
        }
        tables
    }
}

/// Walk the field tree, adding every relation target table (and any `through`
/// junction table) under `db_schema` to `out`.
fn collect_relation_tables(
    fields: &[Field],
    db_schema: &DatabaseSchema,
    out: &mut BTreeSet<QualifiedTable>,
) {
    for field in fields {
        if let Some(relation) = field.relation() {
            out.insert(QualifiedTable::new(
                db_schema.clone(),
                relation.table().clone(),
            ));
            if let RelationKey::Through(through) = relation.key() {
                out.insert(QualifiedTable::new(
                    db_schema.clone(),
                    through.table.clone(),
                ));
            }
        }
        collect_relation_tables(field.children(), db_schema, out);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;

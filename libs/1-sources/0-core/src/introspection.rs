//! Enumerating a source's relational shape — the catalog a designer browses.
//!
//! [`Catalog`](crate::Catalog) answers "what is the type of *this one named
//! column*", and [`CaptureProvisioning`](crate::CaptureProvisioning) answers "is
//! this index's table set coverable". Neither *enumerates* the store. A visual,
//! DB-aware schema designer needs the opposite: discover the whole relational
//! shape — every table, its columns and types, primary keys, foreign keys — so a
//! user can pick from what's really there instead of typing column names from
//! memory.
//!
//! That is this module's job. [`SchemaIntrospection`] is the one method a source
//! implements ([`introspect`](SchemaIntrospection::introspect)); everything it
//! returns is mechanism-neutral vocabulary ([`RelationalCatalog`]/[`TableShape`]/
//! [`ColumnShape`]/[`ForeignKey`]), so any future source backend gets a visual
//! designer for free. The shape is reusable beyond the UI — any tool that wants
//! to reason about the live schema can consume it.
//!
//! Junction detection is **not** on the trait: which tables look like
//! many-to-many junctions is pure logic over the catalog, so it lives in the
//! free function [`junction_candidates`].
//!
//! ```no_run
//! # use sources_core::{SchemaIntrospection, junction_candidates, Result};
//! # async fn demo(source: &dyn SchemaIntrospection) -> Result<()> {
//! let catalog = source.introspect().await?;
//! for table in &catalog.tables {
//!     println!("{}.{} ({} columns)", table.schema, table.name, table.columns.len());
//! }
//! for junction in junction_candidates(&catalog) {
//!     println!("{} looks like a m2m junction", junction.table);
//! }
//! # Ok(())
//! # }
//! ```

use async_trait::async_trait;
use schema_core::common::ColumnName;
use schema_core::{DatabaseSchema, FlussoType, TableName};
use serde::{Deserialize, Serialize};

use crate::{QualifiedTable, Result};

/// One column as the source's catalog describes it, plus a suggested flusso
/// field type when the native type maps cleanly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnShape {
    /// The column name.
    pub name: ColumnName,
    /// The native type, spelled as the store spells it (e.g. Postgres
    /// `character varying(255)`).
    pub sql_type: String,
    /// Whether the column admits null.
    pub nullable: bool,
    /// Whether the column participates in its table's primary key.
    pub is_primary_key: bool,
    /// A flusso field type that maps cleanly from `sql_type`, when one does.
    /// `None` means the type has no obvious default and the designer should ask
    /// (e.g. a `keyword`-vs-`text` choice, or an unknown user type).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_type: Option<FlussoType>,
}

/// A foreign key from one table to another, columns in catalog order (a
/// composite key keeps its column order aligned between the two sides).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignKey {
    /// The referencing columns on the owning table.
    pub columns: Vec<ColumnName>,
    /// Schema of the referenced table.
    pub references_schema: DatabaseSchema,
    /// The referenced table.
    pub references_table: TableName,
    /// The referenced columns, positionally aligned with `columns`.
    pub references_columns: Vec<ColumnName>,
}

/// One table's shape: its columns, primary key, and outgoing foreign keys.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableShape {
    /// Schema the table lives in.
    pub schema: DatabaseSchema,
    /// The table name.
    pub name: TableName,
    /// Every column, in catalog (ordinal) order.
    pub columns: Vec<ColumnShape>,
    /// The primary-key columns, in key order. Empty when the table has none.
    pub primary_key: Vec<ColumnName>,
    /// Foreign keys this table declares (its outgoing references).
    pub foreign_keys: Vec<ForeignKey>,
}

/// A snapshot of a source's relational catalog — every table it can stream
/// from, with enough shape to drive schema authoring.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RelationalCatalog {
    /// Every introspected table, ordered by `(schema, table)`.
    pub tables: Vec<TableShape>,
}

/// A source's ability to enumerate its relational catalog for discovery and
/// design tooling. Implemented per mechanism (Postgres reads `pg_catalog`);
/// consumed by anything that needs to reason about the live schema without
/// knowing the backend.
#[async_trait]
pub trait SchemaIntrospection: Send + Sync {
    /// Read-only: enumerate every streamable table with its columns, primary
    /// key, and foreign keys. Never mutates anything.
    async fn introspect(&self) -> Result<RelationalCatalog>;
}

/// A table that looks like a many-to-many junction: exactly two outgoing
/// foreign keys, so the two FKs are the candidate `through` keys for a
/// `many_to_many` join (`left_key` → first FK, `right_key` → second).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JunctionCandidate {
    /// The junction table itself.
    pub table: QualifiedTable,
    /// The first foreign key (its `columns` are the junction's `left_key`).
    pub left: ForeignKey,
    /// The second foreign key (its `columns` are the junction's `right_key`).
    pub right: ForeignKey,
}

/// Tables in `catalog` that look like many-to-many junctions: pure logic over
/// the catalog, so it's a free function rather than a trait method.
///
/// A table qualifies when it has **exactly two** outgoing foreign keys (each
/// single-column — composite-key junctions are out of v1 scope). The two FKs
/// are returned in catalog order; a designer offers them as a `many_to_many`
/// `through` pair.
pub fn junction_candidates(catalog: &RelationalCatalog) -> Vec<JunctionCandidate> {
    catalog
        .tables
        .iter()
        .filter_map(|table| {
            let [left, right] = table.foreign_keys.as_slice() else {
                return None;
            };
            if left.columns.len() != 1 || right.columns.len() != 1 {
                return None;
            }
            Some(JunctionCandidate {
                table: QualifiedTable::new(table.schema.clone(), table.name.clone()),
                left: left.clone(),
                right: right.clone(),
            })
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn fk(col: &str, table: &str, ref_col: &str) -> ForeignKey {
        ForeignKey {
            columns: vec![ColumnName::try_new(col).unwrap()],
            references_schema: DatabaseSchema::try_new("public").unwrap(),
            references_table: TableName::try_new(table).unwrap(),
            references_columns: vec![ColumnName::try_new(ref_col).unwrap()],
        }
    }

    fn table(name: &str, foreign_keys: Vec<ForeignKey>) -> TableShape {
        TableShape {
            schema: DatabaseSchema::try_new("public").unwrap(),
            name: TableName::try_new(name).unwrap(),
            columns: Vec::new(),
            primary_key: Vec::new(),
            foreign_keys,
        }
    }

    #[test]
    fn two_single_column_fks_is_a_junction() {
        let catalog = RelationalCatalog {
            tables: vec![table(
                "product_tags",
                vec![
                    fk("product_id", "products", "id"),
                    fk("tag_id", "tags", "id"),
                ],
            )],
        };
        let candidates = junction_candidates(&catalog);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].left.references_table.as_ref(), "products");
        assert_eq!(candidates[0].right.references_table.as_ref(), "tags");
    }

    #[test]
    fn one_or_three_fks_is_not_a_junction() {
        let catalog = RelationalCatalog {
            tables: vec![
                table("orders", vec![fk("user_id", "users", "id")]),
                table(
                    "noise",
                    vec![fk("a", "x", "id"), fk("b", "y", "id"), fk("c", "z", "id")],
                ),
            ],
        };
        assert!(junction_candidates(&catalog).is_empty());
    }
}

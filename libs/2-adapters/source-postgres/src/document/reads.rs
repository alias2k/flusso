//! The columns every index reads, per part, derived once when the builder is
//! built so resolving a change only looks them up.
//!
//! For each index: the [`ColumnUse`] of its root row, and, per table a relation
//! reads, one [`ColumnUse`] per path back to the root, in the order
//! [`find_paths`] yields those paths (the last step of each path decides, see
//! [`source::relation_columns`] and [`source::via_columns`]). A path with no
//! entry counts as touched.

use std::collections::HashMap;

use kernel::{Field, IndexName, IndexSchema, RelationKey, TableName};
use source::{ColumnUse, SourceSpec, relation_columns, root_columns, via_columns};

use super::fields::{Step, find_paths};

#[derive(Debug, Default)]
pub(super) struct ReadColumns {
    indexes: HashMap<IndexName, IndexReads>,
}

#[derive(Debug)]
struct IndexReads {
    root: ColumnUse,
    paths: HashMap<TableName, Vec<ColumnUse>>,
}

impl ReadColumns {
    pub(super) fn new(spec: &SourceSpec) -> Self {
        let indexes = spec
            .indexes()
            .map(|(name, schema)| (name.clone(), IndexReads::new(schema)))
            .collect();
        Self { indexes }
    }

    /// What `index` reads from its root row; [`ColumnUse::All`] if unknown.
    pub(super) fn root(&self, index: &IndexName) -> &ColumnUse {
        self.indexes
            .get(index)
            .map_or(&ColumnUse::All, |reads| &reads.root)
    }

    /// What the `n`th path from `table` back to `index`'s root reads from
    /// `table`; [`ColumnUse::All`] if unknown.
    pub(super) fn path(&self, index: &IndexName, table: &TableName, n: usize) -> &ColumnUse {
        self.indexes
            .get(index)
            .and_then(|reads| reads.paths.get(table))
            .and_then(|paths| paths.get(n))
            .unwrap_or(&ColumnUse::All)
    }
}

impl IndexReads {
    fn new(schema: &IndexSchema) -> Self {
        let mut tables = Vec::new();
        related_tables(&schema.fields, &mut tables);
        let paths = tables
            .into_iter()
            .map(|table| {
                let mut found = Vec::new();
                find_paths(&schema.fields, table, &mut Vec::new(), &mut found);
                let uses = found
                    .iter()
                    .map(|path| match path.last() {
                        Some(Step::Relation(relation)) => relation_columns(schema, relation, table),
                        Some(Step::Via(hop)) => via_columns(schema, hop),
                        None => ColumnUse::All,
                    })
                    .collect();
                (table.clone(), uses)
            })
            .collect();
        Self {
            root: root_columns(schema),
            paths,
        }
    }
}

/// Every table a relation reads at any depth: targets, junctions and `via`
/// tables.
fn related_tables<'a>(fields: &'a [Field], out: &mut Vec<&'a TableName>) {
    for field in fields {
        if let Some(relation) = field.relation() {
            out.push(relation.table());
            if let RelationKey::Through(through) = relation.key() {
                out.push(&through.table);
            }
            out.extend(relation.via().iter().map(|hop| &hop.table));
        }
        related_tables(field.children(), out);
    }
}

//! What each index reads, derived once when the builder is built, so
//! resolving a change only looks it up.
//!
//! For each index: the [`ColumnUse`] of its root row, and, per table a relation
//! reads, one [`PathReads`] per path back to the root, in the order
//! [`find_paths`] yields those paths: where the path is, what it reads from
//! the table (the last step of each path decides, see
//! [`source::relation_columns`] and [`source::via_columns`]), and, once a
//! change has asked, whether its root keys are read back from a link.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use kernel::{Field, IndexName, IndexSchema, RelationKey, TableName};
use source::{ColumnUse, SourceSpec, relation_columns, root_columns, via_columns};

use super::fields::{PathAddress, Step, find_paths};
use super::resolve::Link;

#[derive(Debug, Default)]
pub(super) struct ReadColumns {
    indexes: HashMap<IndexName, IndexReads>,
}

#[derive(Debug)]
struct IndexReads {
    root: ColumnUse,
    paths: HashMap<TableName, Vec<PathReads>>,
}

/// One path from a table back to an index's root.
#[derive(Debug)]
pub(super) struct PathReads {
    /// Where the path is in the index's field tree.
    pub(super) address: PathAddress,
    /// What the path reads from the table it starts at.
    pub(super) columns: ColumnUse,
    /// Whether its root keys are read back from a link, with the column-type
    /// generation the answer was derived at.
    root_link: Mutex<Option<(u64, Option<Link>)>>,
}

impl PathReads {
    /// The link this path's root keys are read back from, as known at
    /// `generation`; `None` when not known then.
    pub(super) fn root_link(&self, generation: u64) -> Option<Option<Link>> {
        self.root_link
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .filter(|(known, _)| *known == generation)
            .map(|(_, link)| link.clone())
    }

    /// Remember `link` as this path's answer, derived at `generation`.
    pub(super) fn know_root_link(&self, generation: u64, link: Option<Link>) {
        *self
            .root_link
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some((generation, link));
    }
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

    /// Every path from `table` back to `index`'s root; none when no relation
    /// of the index reads `table`.
    pub(super) fn paths(&self, index: &IndexName, table: &TableName) -> &[PathReads] {
        self.indexes
            .get(index)
            .and_then(|reads| reads.paths.get(table))
            .map_or(&[], Vec::as_slice)
    }
}

impl IndexReads {
    fn new(schema: &IndexSchema) -> Self {
        let mut tables = Vec::new();
        related_tables(&schema.fields, &mut tables);
        let paths = tables
            .into_iter()
            .map(|table| {
                let paths = find_paths(&schema.fields, table)
                    .into_iter()
                    .map(|address| {
                        let steps = address.steps(&schema.fields).unwrap_or_default();
                        let columns = match steps.last() {
                            Some(Step::Relation(relation)) => {
                                relation_columns(schema, relation, table)
                            }
                            Some(Step::Via { hop, next_on }) => via_columns(schema, hop, next_on),
                            None => ColumnUse::All,
                        };
                        PathReads {
                            address,
                            columns,
                            root_link: Mutex::new(None),
                        }
                    })
                    .collect();
                (table.clone(), paths)
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

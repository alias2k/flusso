//! Pure traversal of an index's field tree: finding relation paths (for reverse
//! resolution), collecting relation tables (to pre-resolve their primary keys)
//! and the columns a document renders as values (to find its `bytea` ones).
//! No I/O.

use std::collections::HashMap;

use kernel::{
    AggregateOp, ColumnName, ColumnPair, Expression, Field, FieldSource, Filter, Relation,
    RelationKey, TableName, ViaHop,
};

/// One hop of a path from the root down to a changed table: a relation's own
/// hop, or one table of an aggregate's `via` on the way to it.
#[derive(Debug, Clone, Copy)]
pub(super) enum Step<'a> {
    Relation(&'a Relation),
    /// A downward foreign-key hop: `foreign_key` on `table` points at the
    /// table before it. `next_on` are the `on` pairs of the level after the
    /// hop, whose `local` columns the hop's rows hold.
    Via {
        hop: &'a ViaHop,
        next_on: &'a [ColumnPair],
    },
}

impl<'a> Step<'a> {
    /// The table this hop reaches.
    pub(super) fn table(self) -> &'a TableName {
        match self {
            Step::Relation(relation) => relation.table(),
            Step::Via { hop, .. } => &hop.table,
        }
    }

    /// The key tying this hop's rows to the table before it.
    pub(super) fn key(self) -> RelationKey<'a> {
        match self {
            Step::Relation(relation) => relation.key(),
            Step::Via { hop, .. } => RelationKey::Direct(&hop.foreign_key),
        }
    }

    /// The table whose rows hold the link to the table before, and that
    /// column; `None` for a `belongs_to`, whose link is on the parent.
    pub(super) fn row_link(self) -> Option<(&'a TableName, &'a ColumnName)> {
        match self {
            Step::Relation(relation) => source::row_link(relation),
            Step::Via { hop, .. } => Some((&hop.table, &hop.foreign_key)),
        }
    }
}

/// Collect every `(table, column)` whose value a document renders, at any
/// depth, `table` being the table `fields` read: each column field, a
/// `min`/`max` over a plain column, and the key an `ids` aggregate collects
/// (from `pks`, by table name).
pub(super) fn collect_value_columns(
    fields: &[Field],
    table: &TableName,
    pks: &HashMap<String, ColumnName>,
    out: &mut Vec<(TableName, ColumnName)>,
) {
    for field in fields {
        match &field.source {
            FieldSource::Column(column) => out.push((table.clone(), column.column.clone())),
            FieldSource::Group(nested) => collect_value_columns(nested, table, pks, out),
            FieldSource::Relation(Relation::Join(join)) => {
                collect_value_columns(&join.fields, &join.table, pks, out);
            }
            FieldSource::Relation(Relation::Aggregate(aggregate)) => match &aggregate.op {
                AggregateOp::Min(Expression::Column(column))
                | AggregateOp::Max(Expression::Column(column)) => {
                    out.push((aggregate.table.clone(), column.clone()));
                }
                AggregateOp::Ids { .. } => {
                    if let Some(pk) = pks.get(&aggregate.table.to_string()) {
                        out.push((aggregate.table.clone(), pk.clone()));
                    }
                }
                _ => {}
            },
            FieldSource::Geo(_) | FieldSource::Constant(_) | FieldSource::Script(_) => {}
        }
    }
}

/// Collect every `(table, column)` a value filter compares against, at any
/// depth. A relation's filters run against its own target table and a
/// junction's filters against the junction, so each
/// [`ValueOpFilter`](kernel::ValueOpFilter)'s column is paired with the table
/// it runs on — the document query later casts the operand to that column's
/// real type. Null-check and raw filters carry no typed operand and are
/// skipped.
pub(super) fn collect_filter_columns<'a>(
    fields: &'a [Field],
    out: &mut Vec<(&'a TableName, &'a ColumnName)>,
) {
    for field in fields {
        if let Some(relation) = field.relation() {
            push_value_ops(relation.table(), relation.filters(), out);
            if let RelationKey::Through(through) = relation.key() {
                push_value_ops(&through.table, through.filters.as_deref(), out);
            }
            for hop in relation.via() {
                push_value_ops(&hop.table, hop.filters.as_deref(), out);
            }
        }
        collect_filter_columns(field.children(), out);
    }
}

fn push_value_ops<'a>(
    table: &'a TableName,
    filters: Option<&'a [Filter]>,
    out: &mut Vec<(&'a TableName, &'a ColumnName)>,
) {
    for filter in filters.unwrap_or_default() {
        if let Filter::ValueOp(value_op) = filter {
            out.push((table, &value_op.column));
        }
    }
}

/// Collect every path from the root down to `table`, at any depth. `prefix`
/// is the chain of steps to the current point; a same-row group adds no step,
/// a relation adds one, plus one per table of an aggregate's `via` before it.
/// A path ends at the step that reaches `table` (as its table or junction).
pub(super) fn find_paths<'a>(
    fields: &'a [Field],
    table: &TableName,
    prefix: &mut Vec<Step<'a>>,
    out: &mut Vec<Vec<Step<'a>>>,
) {
    for field in fields {
        match field.relation() {
            Some(relation) => {
                let depth = prefix.len();
                let steps = source::via_hops(relation)
                    .map(|(hop, next_on)| Step::Via { hop, next_on })
                    .chain([Step::Relation(relation)]);
                for step in steps {
                    prefix.push(step);
                    let hit = step.table() == table
                        || matches!(step.key(), RelationKey::Through(through) if through.table == *table);
                    if hit {
                        out.push(prefix.clone());
                    }
                }
                find_paths(field.children(), table, prefix, out);
                prefix.truncate(depth);
            }
            None => find_paths(field.children(), table, prefix, out),
        }
    }
}

/// Collect the target table of every relation, and every `via` table, at any
/// depth — the tables whose primary keys the document query needs (to
/// correlate and join through).
pub(super) fn collect_relation_tables(fields: &[Field], out: &mut Vec<TableName>) {
    for field in fields {
        if let Some(relation) = field.relation() {
            out.push(relation.table().clone());
            out.extend(relation.via().iter().map(|hop| hop.table.clone()));
        }
        collect_relation_tables(field.children(), out);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;

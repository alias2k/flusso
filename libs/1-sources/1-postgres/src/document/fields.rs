//! Pure traversal of an index's field tree: finding relation paths (for reverse
//! resolution), collecting relation tables (to pre-resolve their primary keys),
//! and resolving a field name back to its column. No I/O.

use schema_core::{ColumnName, Field, FieldName, Filter, Relation, RelationKey, TableName};

pub(super) fn relation_target(relation: &Relation) -> (&TableName, RelationKey<'_>) {
    (relation.table(), relation.key())
}

/// Collect every `(table, column)` a value filter compares against, at any
/// depth. A relation's filters run against its own target table, so each
/// [`ValueOpFilter`](schema_core::ValueOpFilter)'s column is paired with the
/// relation's table — the document query later casts the operand to that
/// column's real type. Null-check and raw filters carry no typed operand and
/// are skipped.
pub(super) fn collect_filter_columns<'a>(
    fields: &'a [Field],
    out: &mut Vec<(&'a TableName, &'a ColumnName)>,
) {
    for field in fields {
        if let Some(relation) = field.relation() {
            for filter in relation.filters().unwrap_or_default() {
                if let Filter::ValueOp(value_op) = filter {
                    out.push((relation.table(), &value_op.column));
                }
            }
        }
        collect_filter_columns(field.children(), out);
    }
}

/// Collect every relation path from the root down to `table`, at any depth.
/// `prefix` is the chain of relations to the current point; a same-row group
/// adds no hop, a relation does.
pub(super) fn find_paths<'a>(
    fields: &'a [Field],
    table: &TableName,
    prefix: &mut Vec<&'a Relation>,
    out: &mut Vec<Vec<&'a Relation>>,
) {
    for field in fields {
        match field.relation() {
            Some(relation) => {
                prefix.push(relation);
                let (target, key) = relation_target(relation);
                let hit = target == table
                    || matches!(key, RelationKey::Through(through) if through.table == *table);
                if hit {
                    out.push(prefix.clone());
                }
                find_paths(field.children(), table, prefix, out);
                prefix.pop();
            }
            None => find_paths(field.children(), table, prefix, out),
        }
    }
}

/// Collect the target table of every relation at any depth — the tables whose
/// primary keys the document query needs (to correlate and join through).
pub(super) fn collect_relation_tables(fields: &[Field], out: &mut Vec<TableName>) {
    for field in fields {
        if let Some(relation) = field.relation() {
            out.push(relation.table().clone());
        }
        collect_relation_tables(field.children(), out);
    }
}

pub(super) fn field_column<'a>(fields: &'a [Field], name: &FieldName) -> Option<&'a ColumnName> {
    for field in fields {
        if &field.field == name {
            return field.column();
        }
        if let Some(column) = field_column(field.children(), name) {
            return Some(column);
        }
    }
    None
}

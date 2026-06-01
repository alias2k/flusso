//! Pure traversal of an index's field tree: finding relation paths (for reverse
//! resolution), collecting relation tables (to pre-resolve their primary keys),
//! and resolving a field name back to its column. No I/O.

use schema_core::{ColumnName, Field, FieldName, FieldRelation, JoinKey, TableName};

/// The target table and key of a relation.
pub(super) fn relation_target(relation: &FieldRelation) -> (&TableName, &JoinKey) {
    match relation {
        FieldRelation::Join(join) => (&join.table, &join.key),
        FieldRelation::Aggregate(aggregate) => (&aggregate.table, &aggregate.key),
    }
}

/// Collect every relation path from the root down to `table`, at any depth.
/// `prefix` is the chain of relations to the current point; a same-row group
/// adds no hop, a relation does.
pub(super) fn find_paths<'a>(
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

/// Collect the target table of every relation at any depth — the tables whose
/// primary keys the document query needs (to correlate and join through).
pub(super) fn collect_relation_tables(fields: &[Field], out: &mut Vec<TableName>) {
    for field in fields {
        if let Some(relation) = &field.relation {
            out.push(relation_target(relation).0.clone());
        }
        if let Some(nested) = &field.fields {
            collect_relation_tables(nested, out);
        }
    }
}

/// Resolve a document field name back to the column it reads from.
pub(super) fn field_column<'a>(fields: &'a [Field], name: &FieldName) -> Option<&'a ColumnName> {
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

//! Pure traversal of an index's field tree: finding relation paths, the columns
//! to select, and resolving a field name back to its column. No I/O.

use schema_core::{
    ColumnName, Field, FieldName, FieldRelation, IndexSchema, JoinKey, SoftDelete, TableName,
};

use super::push_unique;

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

/// Whether any relation appears among these fields (or their same-row groups).
pub(super) fn contains_relation(fields: &[Field]) -> bool {
    fields.iter().any(|field| {
        field.relation.is_some()
            || matches!(&field.fields, Some(nested) if field.relation.is_none() && contains_relation(nested))
    })
}

/// The root-table columns the document reads: primary key, doc id, soft-delete
/// column, and every column-backed field (including same-row nested groups).
pub(super) fn root_columns(schema: &IndexSchema) -> Vec<ColumnName> {
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
pub(super) fn collect_column_fields(fields: &[Field], out: &mut Vec<ColumnName>) {
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

//! Which columns of a table a document reads — the one place that derives it
//! from the schema.
//!
//! An update that changes none of the columns a document reads can't change
//! that document, so resolution skips it. This module answers "which columns"
//! per part of the document; [`ColumnUse::touched_by`] answers "did this update
//! change any of them".
//!
//! - [`root_columns`]: what an index reads from its root row — root-level
//!   fields, the root `filters`, the `soft_delete` marker and its `when`.
//! - [`relation_columns`]: what a relation reads from one of its tables — the
//!   projected fields, `filters`, `order_by`, the aggregate input, and the
//!   link (a child's `foreign_key`, a junction's `left_key`/`right_key`).
//! - [`row_link`]: the link column a related row holds, which a deleted or
//!   re-parented row's pre-image must carry (see
//!   [`SourceSpec::pre_image_columns`](crate::SourceSpec::pre_image_columns)).
//!
//! A raw SQL filter can read any column, so it makes its table [`ColumnUse::All`].
//! A table's primary key is always read (it correlates the table's own
//! relations), so `touched_by` checks the changed row's key columns too.
//!
//! ```
//! use kernel::{ColumnName, GenericValue};
//! use source::{ColumnUse, RowImage, RowKey};
//!
//! let column = |name: &str| ColumnName::try_new(name).unwrap();
//! let key = RowKey(vec![(column("id"), GenericValue::Int(1))]);
//! let image = |seen: i64| {
//!     RowImage(vec![
//!         (column("id"), GenericValue::Int(1)),
//!         (column("name"), GenericValue::String("ada".into())),
//!         (column("last_seen"), GenericValue::BigInt(seen)),
//!     ])
//! };
//! let reads_name = ColumnUse::Columns([column("name")].into());
//!
//! assert!(!reads_name.touched_by(&key, Some(&image(1)), Some(&image(2))));
//! assert!(reads_name.touched_by(&key, None, Some(&image(2))));
//! ```

use std::collections::HashSet;

use kernel::{
    ColumnName, Field, FieldSource, Filter, IndexSchema, Relation, RelationKey, SoftDelete,
    TableName,
};

use crate::{RowImage, RowKey};

/// The columns of one table a part of a document reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnUse {
    /// Any column may matter: a raw SQL filter reads the table.
    All,
    /// Exactly these columns, plus the table's primary key.
    Columns(HashSet<ColumnName>),
}

impl ColumnUse {
    /// Whether an update from `before` to `after` may change what is read.
    ///
    /// `true` unless both images are present and carry every used column (and
    /// every column of `key`) with equal values. A missing image or column
    /// counts as changed: the skip never rests on missing information.
    pub fn touched_by(
        &self,
        key: &RowKey,
        before: Option<&RowImage>,
        after: Option<&RowImage>,
    ) -> bool {
        let (Some(before), Some(after), ColumnUse::Columns(columns)) = (before, after, self) else {
            return true;
        };
        key.0
            .iter()
            .map(|(column, _)| column)
            .chain(columns)
            .any(|column| match (before.get(column), after.get(column)) {
                (Some(old), Some(new)) => old != new,
                _ => true,
            })
    }
}

/// Collects columns until a raw filter makes the whole table relevant.
#[derive(Default)]
struct Collector {
    columns: HashSet<ColumnName>,
    all: bool,
}

impl Collector {
    fn column(&mut self, column: &ColumnName) {
        self.columns.insert(column.clone());
    }

    fn filters(&mut self, filters: Option<&[Filter]>) {
        for filter in filters.unwrap_or_default() {
            match filter {
                Filter::Raw(_) => self.all = true,
                Filter::NullCheck(check) => self.column(&check.column),
                Filter::ValueOp(value_op) => self.column(&value_op.column),
            }
        }
    }

    /// The columns one row level reads: its fields, same-row groups, and the
    /// `belongs_to` columns pointing out of it. Other relations correlate on the
    /// row's primary key and read their own tables.
    fn level(&mut self, fields: &[Field]) {
        for field in fields {
            match &field.source {
                FieldSource::Column(column) => self.column(&column.column),
                FieldSource::Geo(geo) => {
                    self.column(&geo.lat);
                    self.column(&geo.lon);
                }
                FieldSource::Group(fields) => self.level(fields),
                FieldSource::Relation(relation) => {
                    if let RelationKey::Local(column) = relation.key() {
                        self.column(column);
                    }
                }
                FieldSource::Constant(_) => {}
            }
        }
    }

    fn finish(self) -> ColumnUse {
        if self.all {
            ColumnUse::All
        } else {
            ColumnUse::Columns(self.columns)
        }
    }
}

/// The columns an index reads from its root row.
pub fn root_columns(schema: &IndexSchema) -> ColumnUse {
    let mut used = Collector::default();
    used.level(&schema.fields);
    used.filters(schema.filters.as_deref());
    match &schema.soft_delete {
        Some(SoftDelete::Column(marker)) => {
            used.column(&marker.column);
            used.filters(marker.when.as_deref());
        }
        Some(SoftDelete::Field(marker)) => {
            match root_level_column(&schema.fields, &marker.field) {
                Some(column) => used.column(column),
                None => used.all = true,
            }
            used.filters(marker.when.as_deref());
        }
        None => {}
    }
    used.finish()
}

/// The columns of `table` that `relation` reads: as its target (fields,
/// `filters`, `order_by`, aggregate input, a child's `foreign_key`) and as its
/// junction (`left_key`, `right_key`, the junction's `filters`).
pub fn relation_columns(relation: &Relation, table: &TableName) -> ColumnUse {
    let mut used = Collector::default();
    if relation.table() == table {
        match relation {
            Relation::Join(join) => {
                used.level(&join.fields);
                for order in join.order_by.as_deref().unwrap_or_default() {
                    used.column(&order.column);
                }
            }
            Relation::Aggregate(aggregate) => {
                if let Some(input) = aggregate_input(&aggregate.op) {
                    used.column(input);
                }
            }
        }
        used.filters(relation.filters());
        if let RelationKey::Direct(foreign_key) = relation.key() {
            used.column(foreign_key);
        }
    }
    if let RelationKey::Through(through) = relation.key()
        && through.table == *table
    {
        used.column(&through.left_key);
        used.column(&through.right_key);
        used.filters(through.filters.as_deref());
    }
    used.finish()
}

/// The table whose rows hold `relation`'s link to the parent, and that column:
/// a `has_one`/`has_many` (or direct aggregate) child's `foreign_key`, a
/// junction's `left_key`. `None` for a `belongs_to`, whose link is on the parent.
pub fn row_link(relation: &Relation) -> Option<(&TableName, &ColumnName)> {
    match relation.key() {
        RelationKey::Direct(foreign_key) => Some((relation.table(), foreign_key)),
        RelationKey::Through(through) => Some((&through.table, &through.left_key)),
        RelationKey::Local(_) => None,
    }
}

fn aggregate_input(op: &kernel::AggregateOp) -> Option<&ColumnName> {
    use kernel::AggregateOp;
    match op {
        AggregateOp::Sum(column)
        | AggregateOp::Avg(column)
        | AggregateOp::Min(column)
        | AggregateOp::Max(column) => Some(column),
        AggregateOp::Count | AggregateOp::Ids { .. } => None,
    }
}

/// The column behind a root-level field (groups included), if it is a column.
fn root_level_column<'a>(fields: &'a [Field], name: &kernel::FieldName) -> Option<&'a ColumnName> {
    fields.iter().find_map(|field| match &field.source {
        FieldSource::Column(column) if field.field == *name => Some(&column.column),
        FieldSource::Group(fields) => root_level_column(fields, name),
        _ => None,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;

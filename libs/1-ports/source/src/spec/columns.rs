//! Which columns of a table a document reads — the one place that derives it
//! from the schema.
//!
//! An update that changes none of the columns a document reads can't change
//! that document, so resolution skips it. This module answers "which columns"
//! per part of the document; [`ColumnUse::touched_by`] answers "did this update
//! change any of them", against a [`RowDiff`] built once per change.
//!
//! - [`root_columns`]: what an index reads from its root row — root-level
//!   fields, the root `filters`, the `soft_delete` marker and its `when`.
//! - [`relation_columns`]: what a relation reads from one of its tables — the
//!   projected fields, `filters`, `order_by`, every column of the aggregate
//!   input (one column or an arithmetic expression), and the
//!   link (a child's `foreign_key`, a junction's `left_key`/`right_key`, the
//!   `related` side of its `on` pairs).
//! - [`via_columns`]: what an aggregate reads from one of its `via` tables —
//!   the hop's `foreign_key`, `filters`, the `related` side of its `on` pairs
//!   and the `local` side of the next level's.
//! - [`soft_delete_column`]: the column a `soft_delete` marker reads on the
//!   root row; the document query reads the same one.
//! - [`row_link`]: the link column a related row holds, which a deleted or
//!   re-parented row's pre-image must carry (see
//!   [`SourceSpec::pre_image_columns`](crate::SourceSpec::pre_image_columns)).
//!
//! An `on` pair reads a column on each side of its hop: `related` on the hop's
//! own table, `local` on the table before it, so a parent level (the root row,
//! a join's rows, a `via` hop) counts the `local` columns of the relations it
//! correlates.
//!
//! A raw SQL filter is pasted into a correlated subquery, so it can read any
//! column it can name: its own table's, the root row's (`root.…`), a
//! junction's, an enclosing level's. One raw filter anywhere in an index makes
//! every set of that index [`ColumnUse::All`]. A table's primary key is always
//! read (it correlates the table's own relations): the declared `primary_key`
//! is in every set, and [`RowDiff::new`] checks the changed row's key columns.
//!
//! ```
//! use kernel::{ColumnName, GenericValue};
//! use source::{ColumnUse, RowDiff, RowImage, RowKey};
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
//! let (before, after) = (image(1), image(2));
//!
//! assert!(!reads_name.touched_by(&RowDiff::new(&key, Some(&before), Some(&after))));
//! assert!(reads_name.touched_by(&RowDiff::new(&key, None, Some(&after))));
//! ```

use std::collections::{HashMap, HashSet};

use kernel::{
    ColumnName, ColumnPair, Field, FieldName, FieldSource, Filter, GenericValue, IndexSchema,
    Relation, RelationKey, SoftDelete, TableName, ViaHop,
};

use crate::{RowImage, RowKey};

/// The columns of one table a part of a document reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnUse {
    /// Any column may matter: the index has a raw SQL filter.
    All,
    /// Exactly these columns, plus the table's primary key.
    Columns(HashSet<ColumnName>),
}

impl ColumnUse {
    /// Whether the update `diff` describes may change what is read.
    ///
    /// `true` unless the diff is known and both images carry every used column
    /// with equal values. A missing column counts as changed: the skip never
    /// rests on missing information.
    pub fn touched_by(&self, diff: &RowDiff<'_>) -> bool {
        let (Some(images), ColumnUse::Columns(columns)) = (&diff.images, self) else {
            return true;
        };
        columns.iter().any(|column| images.changed(column))
    }
}

/// One update's pre- and post-image, indexed once so every
/// [`ColumnUse::touched_by`] check is a lookup per used column.
///
/// Unknown — every part touched — when either image is missing or the changed
/// row's key columns are not carried unchanged by both.
#[derive(Debug)]
pub struct RowDiff<'a> {
    images: Option<Images<'a>>,
}

#[derive(Debug)]
struct Images<'a> {
    before: HashMap<&'a ColumnName, &'a GenericValue>,
    after: HashMap<&'a ColumnName, &'a GenericValue>,
}

impl Images<'_> {
    fn changed(&self, column: &ColumnName) -> bool {
        match (self.before.get(column), self.after.get(column)) {
            (Some(old), Some(new)) => old != new,
            _ => true,
        }
    }
}

impl<'a> RowDiff<'a> {
    /// Compare an update's images for the row `key` addresses.
    pub fn new(key: &RowKey, before: Option<&'a RowImage>, after: Option<&'a RowImage>) -> Self {
        let images = before.zip(after).map(|(before, after)| Images {
            before: before
                .0
                .iter()
                .map(|(column, value)| (column, value))
                .collect(),
            after: after
                .0
                .iter()
                .map(|(column, value)| (column, value))
                .collect(),
        });
        let images =
            images.filter(|images| !key.0.iter().any(|(column, _)| images.changed(column)));
        Self { images }
    }

    /// A diff that knows nothing, so every part counts as touched.
    pub fn unknown() -> Self {
        Self { images: None }
    }
}

/// Collects the columns one part reads. A raw filter never reaches it:
/// [`reads_raw`] turns the whole index to [`ColumnUse::All`] first; it still
/// counts as "all" here so the two can't disagree.
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

    fn locals(&mut self, on: &[ColumnPair]) {
        for pair in on {
            self.column(&pair.local);
        }
    }

    fn related(&mut self, on: &[ColumnPair]) {
        for pair in on {
            self.column(&pair.related);
        }
    }

    /// The columns one row level reads: its fields, same-row groups, the
    /// `belongs_to` columns pointing out of it, and the `local` side of every
    /// relation's first `on` pairs. Otherwise relations correlate on the row's
    /// primary key and read their own tables.
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
                    self.locals(relation.parent_on());
                }
                FieldSource::Constant(_) | FieldSource::Script(_) => {}
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
    if reads_raw(schema) {
        return ColumnUse::All;
    }
    let mut used = Collector::default();
    if let Some(primary_key) = &schema.primary_key {
        used.column(primary_key);
    }
    used.level(&schema.fields);
    used.filters(schema.filters.as_deref());
    if let Some(soft_delete) = &schema.soft_delete {
        match soft_delete_column(schema) {
            Some(column) => used.column(column),
            None => used.all = true,
        }
        used.filters(soft_delete_when(soft_delete));
    }
    used.finish()
}

/// The columns of `table` that `relation` (a relation of `schema`) reads: as
/// its target (fields, `filters`, `order_by`, aggregate input, a child's
/// `foreign_key`, the `related` side of its `on` pairs) and as its junction (`left_key`, `right_key`, the
/// junction's `filters`).
pub fn relation_columns(schema: &IndexSchema, relation: &Relation, table: &TableName) -> ColumnUse {
    if reads_raw(schema) {
        return ColumnUse::All;
    }
    let mut used = Collector::default();
    if relation.table() == table {
        match relation {
            Relation::Join(join) => {
                used.column(&join.primary_key);
                used.level(&join.fields);
                for order in join.order_by.as_deref().unwrap_or_default() {
                    used.column(&order.column);
                }
            }
            Relation::Aggregate(aggregate) => {
                if let Some(input) = aggregate.op.input() {
                    for column in input.columns() {
                        used.column(column);
                    }
                }
            }
        }
        used.filters(relation.filters());
        used.related(relation.on());
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

/// The columns an aggregate (of `schema`) reads from one of its `via` tables:
/// the hop's `foreign_key`, its `filters`, the `related` side of its `on`
/// pairs, and the `local` side of `next_on` — the pairs of the level after the
/// hop (the next hop's, or the aggregate's own after the last). The hop's
/// primary key, which the next level points at, is checked by
/// [`RowDiff::new`].
pub fn via_columns(schema: &IndexSchema, hop: &ViaHop, next_on: &[ColumnPair]) -> ColumnUse {
    if reads_raw(schema) {
        return ColumnUse::All;
    }
    let mut used = Collector::default();
    used.column(&hop.foreign_key);
    used.filters(hop.filters.as_deref());
    used.related(&hop.on);
    used.locals(next_on);
    used.finish()
}

/// Each `via` hop of `relation` with the `on` pairs of the level after it: the
/// next hop's, or the relation's own after the last hop. What
/// [`via_columns`] takes.
pub fn via_hops(relation: &Relation) -> impl Iterator<Item = (&ViaHop, &[ColumnPair])> {
    let via = relation.via();
    let next = via
        .iter()
        .skip(1)
        .map(|hop| hop.on.as_slice())
        .chain([relation.on()]);
    via.iter().zip(next)
}

/// The column a `soft_delete` marker reads on the root row: its `column`, or
/// the column behind the first field named `field`, searched depth-first in
/// declaration order through groups **and** joins. A name found inside a join
/// is still read on the root row, as the document query does.
/// `None` without a marker, or when the first field of that name is not a
/// column (or there is none).
pub fn soft_delete_column(schema: &IndexSchema) -> Option<&ColumnName> {
    match schema.soft_delete.as_ref()? {
        SoftDelete::Column(marker) => Some(&marker.column),
        SoftDelete::Field(marker) => field_column(&schema.fields, &marker.field),
    }
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

fn field_column<'a>(fields: &'a [Field], name: &FieldName) -> Option<&'a ColumnName> {
    first_named(fields, name).and_then(|(field, _)| field.column())
}

/// The first field named `name`, depth-first in declaration order through
/// groups and joins, and whether it was found inside a relation.
pub(crate) fn first_named<'a>(fields: &'a [Field], name: &FieldName) -> Option<(&'a Field, bool)> {
    for field in fields {
        if field.field == *name {
            return Some((field, false));
        }
        let in_relation = matches!(field.source, FieldSource::Relation(_));
        if let Some((found, nested)) = first_named(field.children(), name) {
            return Some((found, nested || in_relation));
        }
    }
    None
}

fn soft_delete_when(soft_delete: &SoftDelete) -> Option<&[Filter]> {
    match soft_delete {
        SoftDelete::Column(marker) => marker.when.as_deref(),
        SoftDelete::Field(marker) => marker.when.as_deref(),
    }
}

/// Whether a raw SQL filter appears anywhere in the index: root `filters`,
/// the `soft_delete` `when`, or any relation's, junction's or `via` hop's
/// `filters`.
fn reads_raw(schema: &IndexSchema) -> bool {
    fn any_raw(filters: Option<&[Filter]>) -> bool {
        filters
            .unwrap_or_default()
            .iter()
            .any(|filter| matches!(filter, Filter::Raw(_)))
    }
    fn in_fields(fields: &[Field]) -> bool {
        fields.iter().any(|field| {
            field.relation().is_some_and(|relation| {
                any_raw(relation.filters())
                    || matches!(relation.key(), RelationKey::Through(through) if any_raw(through.filters.as_deref()))
                    || relation.via().iter().any(|hop| any_raw(hop.filters.as_deref()))
            }) || in_fields(field.children())
        })
    }
    any_raw(schema.filters.as_deref())
        || schema
            .soft_delete
            .as_ref()
            .is_some_and(|soft_delete| any_raw(soft_delete_when(soft_delete)))
        || in_fields(&schema.fields)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;

//! Validating a self-describing schema against a live store.
//!
//! The schema now states every field's type and nullability itself, so the
//! mapping is derived without a database (see
//! [`SourceSpec::index_mappings`](crate::SourceSpec::index_mappings)). A
//! database, *when reachable*, is still useful as a check: does each column
//! exist, and does its real type and nullability agree with what the schema
//! declares? That is this module's job — it reports disagreements as
//! [`Diagnostic`]s rather than filling anything in. With no database, it is
//! simply skipped.
//!
//! Relations are checked too, at any depth: the related table, the key
//! column on the side that holds it, a junction's table and both keys, each
//! `via` hop's table and foreign key, both sides of every `on:` pair, the
//! columns value and null-check filters name (raw filters are opaque), every
//! `order_by` column, and a join's `primary_key`. A missing table is reported
//! once per relation, and the columns on it are not looked up.
//!
//! Every [`Diagnostic`] carries a [`DiagnosticKind`], which fixes its
//! [`Severity`]. An index that starting flusso does not make work is an
//! error, whatever the cause: an unknown table or column, or a `soft_delete`
//! marker that reads nothing (no document builds as written), and an index
//! that does not hold what its schema says, such as a `[]` column without
//! `array: true` or a `required` field over a nullable column. Whether an
//! error stops `flusso run` is the caller's decision, by kind. A join's
//! `primary_key` that names no column is only a warning
//! ([`DiagnosticKind::IgnoredPrimaryKey`]): relations are keyed by the table's
//! own primary key, which every relation table must have, on one column
//! ([`DiagnosticKind::RelationPrimaryKey`]). A column field's `transforms` must
//! apply to its column's type ([`DiagnosticKind::TransformNotApplicable`]).
//! Every link column must compare with the key it names
//! ([`DiagnosticKind::LinkTypeMismatch`]): a foreign key with its parent's key,
//! a `belongs_to` `column` with the target's, a junction's keys with both
//! sides', a `via` hop's foreign key with the level before, and both sides of
//! each `on:` pair. An `ids` aggregate's `element_type` is checked against the
//! related table's key type ([`DiagnosticKind::IdsElementMismatch`],
//! [`DiagnosticKind::IdsElementLossy`]).
//!
//! The transform and link checks ask the store to try the operation. A store
//! that refuses to (a role without a privilege the check needs) answers
//! [`SourceError::Unchecked`], and the check is reported as skipped
//! ([`DiagnosticKind::CheckSkipped`], a warning) with the store's reason.
//!
//! The root's own keys are checked on the root table: that it has a
//! single-column primary key ([`DiagnosticKind::RootPrimaryKey`]), that the
//! declared `primary_key` exists and is that key
//! ([`DiagnosticKind::RootPrimaryKeyMismatch`]) and of a type the source reads
//! back as a document key ([`DiagnosticKind::UnsupportedKeyType`]), the
//! columns its `filters` name, and the `soft_delete` marker — its
//! `column` (or the column behind its `field`) and its `when` filters. A
//! diagnostic on one of these is anchored at the key's name (`table`,
//! `primary_key`, `filters`, `soft_delete`) instead of a field, and its
//! [`Anchor`] says so: a field may carry the same name.
//! Two checks need no database at all: a `soft_delete` `field` that names no
//! column field, and two fields sharing a name at one level
//! ([`DiagnosticKind::DuplicateFieldName`], a warning, since such a schema
//! has always loaded, while a document still holds what the index maps;
//! [`DiagnosticKind::DuplicateFieldNameWrong`], an error, where it doesn't,
//! see [`duplicate_is_wrong`](crate::duplicate_is_wrong); an error of its
//! own kind for a pair that never gives an indexed document, see
//! [`never_indexed`](crate::never_indexed)).
//! [`validate_spec`] reports them on their own (`flusso check --offline`) and
//! [`validate_indexes`] reports them with the rest.
//!
//! A `time` or `timetz` column under a `timestamp` or `date` field is
//! [`DiagnosticKind::TimeOfDayAsDate`]: OpenSearch's `date` mapping refuses
//! every non-null value. Only where every document carries the field, mapped
//! as declared, is it reported: at the root or in plain objects (emitted, no
//! `options`), with no `options` on the field and no namesake. A field in a
//! join or a `min`/`max` is refused only where a related row holds a time,
//! so neither is reported.
//!
//! A date mapping whose `format` reads a time of day only (`HH:mm:ss`,
//! `hour_minute_second`, …) over a value that is no time of day is
//! [`DiagnosticKind::TimeOfDayFormat`]: OpenSearch refuses every document
//! holding one. The mapping keeps such a format as written (see
//! [`kernel::with_iso`]), since a `timestamp` field may sit over a `time`
//! column, the documented way to index a time of day: so a `timestamp`
//! column, a `min`/`max` or `ids` of one is reported only once the database
//! names the column's type; a `date` field, a `custom` over date or
//! timestamp types, a constant or a script with no database.
//!
//! The same rule decides where a duplicate pair that never indexes gets its
//! own kind: under a join, an input-only or unindexed ancestor, or an object
//! with `options`, it does not stop the start. Its severity is then decided
//! by what holds the pair: a document's `_source` under an unindexed parent
//! (stored unchecked), a script under an input-only one (one at that field's
//! level or above reads it through `this`, one anywhere that names `root`
//! through `root`), or nothing at all, the only place a wrong pair is the
//! warning.
//!
//! The one store-specific piece is the [`Catalog`] trait: how a store reports a
//! table and a column, and, when it can, a table's primary key, whether a
//! column's type takes its `transforms`, and whether two columns compare.
//! Everything else — the field walk, which table each name
//! lives on — is shared here.
//!
//! ```
//! # use std::collections::BTreeMap;
//! # use async_trait::async_trait;
//! # use kernel::{ColumnName, DatabaseSchema, IndexName, IndexSchema, TableName};
//! use source::{Catalog, ColumnInfo, SourceError, SourceSpec, validate_indexes};
//!
//! /// A database with one table, `users(id)`.
//! struct Users;
//!
//! #[async_trait]
//! impl Catalog for Users {
//!     async fn column(
//!         &self,
//!         schema: &DatabaseSchema,
//!         table: &TableName,
//!         column: &ColumnName,
//!     ) -> source::Result<ColumnInfo> {
//!         match (table.as_ref(), column.as_ref()) {
//!             ("users", "id") => Ok(ColumnInfo {
//!                 sql_type: "integer".into(),
//!                 base_type: None,
//!                 nullable: false,
//!             }),
//!             ("users", _) => Err(SourceError::UnknownColumn(format!("{schema}.{table}.{column}"))),
//!             _ => Err(SourceError::UnknownTable(format!("{schema}.{table}"))),
//!         }
//!     }
//!
//!     async fn has_table(&self, _: &DatabaseSchema, table: &TableName) -> source::Result<bool> {
//!         Ok(table.as_ref() == "users")
//!     }
//! }
//!
//! let schema: IndexSchema = serde_json::from_value(serde_json::json!({
//!     "version": 1, "table": "users", "db_schema": "public", "primary_key": "id",
//!     "fields": [{ "field": "orderCount", "source": { "relation": { "aggregate": {
//!         "table": "order", "op": "count", "key": { "direct": "user_id" },
//!     } } } }],
//! }))?;
//! let spec = SourceSpec::new(BTreeMap::from([(IndexName::try_new("users")?, schema)]));
//!
//! let diagnostics = futures::executor::block_on(validate_indexes(&spec, &Users))?;
//! assert_eq!(diagnostics.len(), 1);
//! assert_eq!(diagnostics[0].message, "`table` references unknown table `public.order`");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use async_trait::async_trait;
use futures::{StreamExt, TryStreamExt};
use kernel::common::{ColumnName, FieldName, IndexName};
use kernel::{
    Aggregate, AggregateKey, AggregateOp, Column, ColumnPair, DatabaseSchema, Field, FieldSource,
    Filter, FlussoType, Geo, IndexSchema, Join, JoinKind, Relation, SoftDelete, TableName, Through,
    Transform,
};

use crate::spec::first_named;
use crate::{Result, SourceError, SourceSpec};

mod element;

use element::Fit;

/// How a store reports one base-table column: its native type name (as the
/// store spells it, e.g. Postgres `character varying(255)`) and whether it
/// admits null.
#[derive(Debug, Clone)]
pub struct ColumnInfo {
    /// The column's type as the store names it: a domain by its own name.
    pub sql_type: String,
    /// What a domain column's type resolves to through every domain (`text[]`
    /// for a domain over `text[]`), which decides whether it holds an array or
    /// JSON; `None` when the type is no domain.
    pub base_type: Option<String>,
    /// Whether the column admits null.
    pub nullable: bool,
}

/// A source's view of its own catalog: whether a table exists, and the type
/// and nullability of a base-table column. This is all [`validate_indexes`]
/// needs from a source to check a declared schema against the live store.
#[async_trait]
pub trait Catalog: Send + Sync {
    /// The type and nullability of `column` in `table` (within `schema`), as the
    /// store defines it. A missing column is [`SourceError::UnknownColumn`], a
    /// missing table [`SourceError::UnknownTable`].
    async fn column(
        &self,
        schema: &DatabaseSchema,
        table: &TableName,
        column: &ColumnName,
    ) -> Result<ColumnInfo>;

    /// Whether `table` exists in `schema`.
    async fn has_table(&self, schema: &DatabaseSchema, table: &TableName) -> Result<bool>;

    /// The columns of `table`'s primary key, in key order: `Some(vec![])`
    /// when it has none, `None` when the store can't say. Only asked of a
    /// table that exists.
    ///
    /// The default is `None`: every primary-key check is skipped.
    async fn primary_key(
        &self,
        _schema: &DatabaseSchema,
        _table: &TableName,
    ) -> Result<Option<Vec<ColumnName>>> {
        Ok(None)
    }

    /// Whether `transforms`, applied in order, accept the type of `column` in
    /// `table`. Only asked of a column that exists. With `element`, the column
    /// holds an array and the transforms apply to each element, so the
    /// element's type is the one asked about (`text` for a `json`/`jsonb`
    /// column, whose elements are unknown).
    ///
    /// The default is `true`: the check is skipped. A store that refuses to
    /// run the check answers [`SourceError::Unchecked`], which is reported as
    /// [`DiagnosticKind::CheckSkipped`].
    async fn accepts_transforms(
        &self,
        _schema: &DatabaseSchema,
        _table: &TableName,
        _column: &ColumnName,
        _transforms: &[Transform],
        _element: bool,
    ) -> Result<bool> {
        Ok(true)
    }

    /// Whether the source reads a value of `column` in `table` back as a
    /// document key, so a row keyed by it addresses its document. Only asked
    /// of a root table's own single-column primary key, which exists.
    ///
    /// The default is `true`: the check is skipped.
    async fn reads_key(
        &self,
        _schema: &DatabaseSchema,
        _table: &TableName,
        _column: &ColumnName,
    ) -> Result<bool> {
        Ok(true)
    }

    /// Whether a value of `link` and a value of `key` (each a table and a
    /// column) compare for equality, as the document query joins a link
    /// column to the key it names. Only asked of two columns that exist.
    ///
    /// The default is `true`: the check is skipped. A store that refuses to
    /// run the check answers [`SourceError::Unchecked`], which is reported as
    /// [`DiagnosticKind::CheckSkipped`].
    async fn compares(
        &self,
        _schema: &DatabaseSchema,
        _link: (&TableName, &ColumnName),
        _key: (&TableName, &ColumnName),
    ) -> Result<bool> {
        Ok(true)
    }

    /// Whether the store records which of `table`'s columns admit null: a
    /// table does; a view's columns read as nullable whatever they hold. Only
    /// asked of a table with a column a field declares non-null that the
    /// store reports nullable.
    ///
    /// The default is `true`: the column is judged as a table's.
    async fn tracks_nullability(
        &self,
        _schema: &DatabaseSchema,
        _table: &TableName,
    ) -> Result<bool> {
        Ok(true)
    }
}

/// How serious a [`Diagnostic`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Starting flusso does not give a working index: documents fail to
    /// build or are rejected, or the index does not hold what its schema
    /// says (a list where it says one value, a null where it says never).
    Error,
    /// Worth knowing, but the index is correct as far as is known (e.g. a
    /// join `primary_key` that is ignored, or a check the store refused to
    /// run).
    Warning,
}

/// What a [`Diagnostic`] found. Each kind has one [`Severity`]; a caller that
/// acts on diagnostics (e.g. refusing to start) decides on the kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticKind {
    /// A table the schema names does not exist.
    UnknownTable,
    /// A column the document query reads does not exist.
    UnknownColumn,
    /// A `soft_delete` `field` that reads no column: the marker would be
    /// dropped and soft-deleted rows would stay indexed.
    UnreadSoftDeleteMarker,
    /// A declared type (field or aggregate `value_type`) does not accept the
    /// column's database type.
    TypeMismatch,
    /// A column an aggregate expression or a geo point reads is not numeric.
    NotNumeric,
    /// A field declared non-null (`required`) over a column that allows null:
    /// a document can hold null where the schema says it never does.
    Nullability,
    /// A field declared non-null (`required`) over a column of a relation
    /// that records no nullability (a view, say), which reads as nullable
    /// whatever it holds: nothing is known either way, so the field must
    /// really never be null.
    NullabilityUntracked,
    /// A join's `primary_key` names no column. Nothing builds on it: the join
    /// is keyed by the table's own primary key.
    IgnoredPrimaryKey,
    /// A relation table (a join's, an aggregate's or a `via` hop's) has no
    /// single-column primary key: none, or a composite one. Every document
    /// build of the index needs it, so every build fails.
    RelationPrimaryKey,
    /// A column field's `transforms` do not apply to the column's type (e.g.
    /// `lowercase` over an `integer`): every document build fails.
    TransformNotApplicable,
    /// The index's root table has no single-column primary key: none, or a
    /// composite one. The backfill skips the table; live changes are still
    /// indexed. Also reported, instead of [`UnknownColumn`](Self::UnknownColumn),
    /// for a declared `primary_key` that names no column on such a table.
    RootPrimaryKey,
    /// The declared `primary_key` names a column of the root table other than
    /// its single-column primary key. Changes and the backfill carry the
    /// table's own key while the build matches on the declared column, so no
    /// document is ever written.
    RootPrimaryKeyMismatch,
    /// The root table's primary key is of a type the source can't read back
    /// as a document key (e.g. a Postgres `inet` or enum): every row's key
    /// reads as null, so no document is ever written.
    UnsupportedKeyType,
    /// A field declares `array: true` over a column that holds no array:
    /// neither a `[]` type nor `json`/`jsonb`.
    NotAnArrayColumn,
    /// A field over a `[]` column does not declare `array: true`: the schema
    /// says one value while the index holds a list, so a consumer reading by
    /// the schema fails on every document holding one.
    UndeclaredArrayColumn,
    /// A link column and the key it names are of types the store can't
    /// compare (e.g. a `text` foreign key against a `uuid` key): the document
    /// query can't join them, so every document build fails.
    LinkTypeMismatch,
    /// An `ids` aggregate's `element_type` can hold no value of the related
    /// table's key type (e.g. `long` over a `uuid` key, or `json`): the sink
    /// rejects every document that collects a key.
    IdsElementMismatch,
    /// An `ids` aggregate's `element_type` holds only some values of the
    /// related table's key type (e.g. `integer` over a `bigint` key: another
    /// is rejected, or stored altered), or changes them (e.g. `date` over an
    /// integer key, read as epoch milliseconds): the index holds other ids
    /// than the table.
    IdsElementLossy,
    /// Two fields at one level of the document share a name (as
    /// [`FieldName`] reads it: trimmed, case kept), and a document still holds
    /// a value the index's mapping takes: the last one's, read as the first
    /// one declares it, or a script's output; or nothing is known either way.
    /// Every other pair is
    /// [`DuplicateFieldNameWrong`](Self::DuplicateFieldNameWrong) (issue #485).
    /// A schema with one still loads, as earlier releases did.
    DuplicateFieldName,
    /// Two fields at one level share a name, and a document then holds
    /// something other than the index maps (see
    /// [`duplicate_is_wrong`](crate::duplicate_is_wrong)): a list where it
    /// maps one value, an object cut to other child fields, raw JSON, text,
    /// `null`, no value at all, a value stored unchecked where another shape
    /// is declared, or a document that fails to build or is refused. Under an
    /// input-only field, what a script reading it gets. Where that holds for every document whatever the data, it is
    /// [`DuplicateFieldNameFailsToBuild`](Self::DuplicateFieldNameFailsToBuild)
    /// or [`DuplicateFieldNameRefused`](Self::DuplicateFieldNameRefused)
    /// instead.
    DuplicateFieldNameWrong,
    /// Two fields at one level share a name, and every document fails to
    /// build whatever the data: an `array: true` column of scalars before an
    /// `object` (see [`never_indexed`](crate::never_indexed)), at the root or
    /// in plain objects only.
    DuplicateFieldNameFailsToBuild,
    /// Two fields at one level share a name, and OpenSearch refuses every
    /// document whatever the data, `ignore_malformed` or not: a scalar or list
    /// constant before an indexed join, `object`, `json` or `map` with no
    /// `options` (see [`never_indexed`](crate::never_indexed)), at the root or
    /// in plain objects only. A sink that does not map the index (`stdout`)
    /// takes it.
    DuplicateFieldNameRefused,
    /// A `timestamp` or `date` field over a `time` or `timetz` column, with no
    /// `options`, at the root or in plain objects: OpenSearch refuses every
    /// document holding a non-null value.
    TimeOfDayAsDate,
    /// A field mapped as a date whose `format` reads a time of day only, over
    /// a value that is no time of day (a date, a timestamp): OpenSearch
    /// refuses every document whose value is not null.
    TimeOfDayFormat,
    /// A check the store refused to run (e.g. the transform or link check,
    /// under a role without USAGE on a type's schema): the message names what
    /// was not checked and the store's reason. Nothing is known either way.
    CheckSkipped,
}

impl DiagnosticKind {
    /// How serious this kind is.
    pub fn severity(self) -> Severity {
        match self {
            Self::UnknownTable
            | Self::UnknownColumn
            | Self::UnreadSoftDeleteMarker
            | Self::TypeMismatch
            | Self::NotNumeric
            | Self::RelationPrimaryKey
            | Self::TransformNotApplicable
            | Self::RootPrimaryKey
            | Self::RootPrimaryKeyMismatch
            | Self::UnsupportedKeyType
            | Self::NotAnArrayColumn
            | Self::LinkTypeMismatch
            | Self::IdsElementMismatch
            | Self::DuplicateFieldNameFailsToBuild
            | Self::DuplicateFieldNameRefused
            | Self::TimeOfDayAsDate
            | Self::TimeOfDayFormat
            | Self::Nullability
            | Self::UndeclaredArrayColumn
            | Self::IdsElementLossy
            | Self::DuplicateFieldNameWrong => Severity::Error,
            Self::IgnoredPrimaryKey
            | Self::DuplicateFieldName
            | Self::NullabilityUntracked
            | Self::CheckSkipped => Severity::Warning,
        }
    }

    /// The kind as a stable `snake_case` token, e.g. `unknown_column`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UnknownTable => "unknown_table",
            Self::UnknownColumn => "unknown_column",
            Self::UnreadSoftDeleteMarker => "unread_soft_delete_marker",
            Self::TypeMismatch => "type_mismatch",
            Self::NotNumeric => "not_numeric",
            Self::Nullability => "nullability",
            Self::NullabilityUntracked => "nullability_untracked",
            Self::IgnoredPrimaryKey => "ignored_primary_key",
            Self::RelationPrimaryKey => "relation_primary_key",
            Self::TransformNotApplicable => "transform_not_applicable",
            Self::RootPrimaryKey => "root_primary_key",
            Self::RootPrimaryKeyMismatch => "root_primary_key_mismatch",
            Self::UnsupportedKeyType => "unsupported_key_type",
            Self::NotAnArrayColumn => "not_an_array_column",
            Self::UndeclaredArrayColumn => "undeclared_array_column",
            Self::LinkTypeMismatch => "link_type_mismatch",
            Self::IdsElementMismatch => "ids_element_mismatch",
            Self::IdsElementLossy => "ids_element_lossy",
            Self::DuplicateFieldName => "duplicate_field_name",
            Self::DuplicateFieldNameWrong => "duplicate_field_name_wrong",
            Self::DuplicateFieldNameFailsToBuild => "duplicate_field_name_fails_to_build",
            Self::DuplicateFieldNameRefused => "duplicate_field_name_refused",
            Self::TimeOfDayAsDate => "time_of_day_as_date",
            Self::TimeOfDayFormat => "time_of_day_format",
            Self::CheckSkipped => "check_skipped",
        }
    }
}

/// What a [`Diagnostic`]'s [`path`](Diagnostic::path) names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Anchor {
    /// A field of the document, by its path from the root.
    Field,
    /// An index-level schema key (`table`, `primary_key`, `filters`,
    /// `soft_delete`), alone. A root field may have the same name.
    IndexKey,
}

impl Anchor {
    /// The anchor as a stable `snake_case` token: `field` or `index_key`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Field => "field",
            Self::IndexKey => "index_key",
        }
    }
}

/// One disagreement between a declared schema and the live database.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Diagnostic {
    /// The index whose schema disagrees.
    pub index: IndexName,
    /// The field the disagreement is on: the last segment of [`path`](Self::path).
    pub field: FieldName,
    /// The field's path from the document root, e.g. `orders`, `items`, `sku`.
    /// A disagreement on an index-level key is anchored at that key alone
    /// (`table`, `primary_key`, `filters` or `soft_delete`), and
    /// [`anchor`](Self::anchor) is [`Anchor::IndexKey`].
    pub path: Vec<FieldName>,
    /// For each segment of [`path`](Self::path), its 1-based position among
    /// the fields of its level when another field there has the same name
    /// (see [`DiagnosticKind::DuplicateFieldName`]), else `None`: which of
    /// the namesakes the finding is on.
    pub positions: Vec<Option<usize>>,
    /// Whether [`path`](Self::path) names a field or an index-level key.
    pub anchor: Anchor,
    /// What was found.
    pub kind: DiagnosticKind,
    /// The kind's [`severity`](DiagnosticKind::severity).
    pub severity: Severity,
    pub message: String,
}

impl Diagnostic {
    /// A diagnostic on a top-level `field` (or, with [`Anchor::IndexKey`], an
    /// index-level key) of `index`; the severity follows from `kind`.
    pub fn new(
        index: IndexName,
        field: FieldName,
        anchor: Anchor,
        kind: DiagnosticKind,
        message: impl Into<String>,
    ) -> Self {
        Self {
            index,
            path: vec![field.clone()],
            positions: vec![None],
            field,
            anchor,
            kind,
            severity: kind.severity(),
            message: message.into(),
        }
    }

    /// The field path joined with `.`, e.g. `orders.items.sku`. A segment
    /// another field at its level shares carries its position, as the
    /// duplicate warning names it: `orders#2.items.sku`.
    pub fn dotted_path(&self) -> String {
        self.path
            .iter()
            .enumerate()
            .map(|(i, name)| match self.positions.get(i).copied().flatten() {
                Some(at) => format!("{name}#{at}"),
                None => name.to_string(),
            })
            .collect::<Vec<String>>()
            .join(".")
    }
}

/// How an [`DiagnosticKind::IdsElementLossy`] message ends: what is wrong,
/// and the fix.
const LOSSY_FIX: &str = "; the index holds other ids than the table: declare an `element_type` \
                         that holds every key (`keyword` takes any key as its text)";

/// How many indexes [`validate_indexes`] checks at once. Each check is a run
/// of small catalog lookups one after another; overlapping a few indexes hides
/// their round trips without taking more than a few of the store's
/// connections at start.
const VALIDATE_CONCURRENCY: usize = 4;

/// Validate every index in `spec` against the store behind `catalog`, returning
/// the disagreements found. An empty result means the declared schema matches
/// the database. The spec already holds only enabled indexes.
///
/// A few indexes are checked at once; the diagnostics come back in the spec's
/// index order, as one index after another would give them.
pub async fn validate_indexes(spec: &SourceSpec, catalog: &dyn Catalog) -> Result<Vec<Diagnostic>> {
    // Built up front, so the buffered stream holds no closure over borrows.
    let checks: Vec<_> = spec
        .indexes()
        .map(|(name, schema)| validate_index(name, schema, catalog))
        .collect();
    let per_index: Vec<Vec<Diagnostic>> = futures::stream::iter(checks)
        .buffered(VALIDATE_CONCURRENCY)
        .try_collect()
        .await?;
    Ok(per_index.concat())
}

/// [`validate_indexes`] for one index.
async fn validate_index(
    name: &IndexName,
    schema: &IndexSchema,
    catalog: &dyn Catalog,
) -> Result<Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();
    let mut checker = Checker {
        index: name,
        db_schema: &schema.db_schema,
        catalog,
        tables: HashMap::new(),
        primary_keys: HashMap::new(),
        nullability: HashMap::new(),
        path: Vec::new(),
        positions: Vec::new(),
        anchor: Anchor::Field,
        uncarried: 0,
        date_mapped: false,
        unmapped: 0,
        date_format: None,
        out: &mut diagnostics,
    };
    checker
        .fields(&schema.table, &schema.fields, schema.primary_key.as_ref())
        .await?;
    checker.root(schema).await?;
    time_of_day_formats(name, &schema.fields, &mut Vec::new(), &mut diagnostics);
    duplicate_names(
        name,
        &schema.fields,
        Level::root(&schema.fields),
        &mut Vec::new(),
        &mut diagnostics,
    );
    Ok(diagnostics)
}

/// The checks that need no database: a `soft_delete` `field` that names no
/// field, or whose first field of that name is not a column (such a marker
/// reads nothing, so soft-deleted rows would stay indexed), and two fields of
/// one name at one level ([`DiagnosticKind::DuplicateFieldName`]).
/// [`validate_indexes`] reports these too.
pub fn validate_spec(spec: &SourceSpec) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for (name, schema) in spec.indexes() {
        if let Some(message) = marker_field_problem(schema)
            && let Some(key) = schema_key(SOFT_DELETE)
        {
            diagnostics.push(Diagnostic::new(
                name.clone(),
                key,
                Anchor::IndexKey,
                DiagnosticKind::UnreadSoftDeleteMarker,
                message,
            ));
        }
        time_of_day_formats(name, &schema.fields, &mut Vec::new(), &mut diagnostics);
        duplicate_names(
            name,
            &schema.fields,
            Level::root(&schema.fields),
            &mut Vec::new(),
            &mut diagnostics,
        );
    }
    diagnostics
}

/// One [`DiagnosticKind::TimeOfDayFormat`] per mapped field whose `format`
/// reads a time of day only where the schema alone says flusso writes a date
/// or a timestamp there. A `timestamp` column, and a `min`/`max` or `ids` of
/// one, may hold a time of day: the database-backed walk decides those.
fn time_of_day_formats(
    index: &IndexName,
    fields: &[Field],
    path: &mut Vec<(FieldName, Option<usize>)>,
    out: &mut Vec<Diagnostic>,
) {
    for (at, field) in fields.iter().enumerate() {
        if !field.emit {
            continue;
        }
        path.push((field.field.clone(), position(fields, at)));
        if let Some(format) =
            kernel::date_format(field).filter(|f| kernel::reads_time_of_day_only(f))
            && !decided_by_its_column(field)
            && let Some(values) = kernel::date_values(field)
        {
            let what = match values {
                kernel::DateValues::Dates => WRITES_A_DATE,
                kernel::DateValues::Timestamps => WRITES_A_TIMESTAMP,
                kernel::DateValues::Both => WRITES_EITHER,
            };
            out.push(Diagnostic {
                index: index.clone(),
                field: field.field.clone(),
                path: path.iter().map(|(name, _)| name.clone()).collect(),
                positions: path.iter().map(|(_, at)| *at).collect(),
                anchor: Anchor::Field,
                kind: DiagnosticKind::TimeOfDayFormat,
                severity: DiagnosticKind::TimeOfDayFormat.severity(),
                message: time_of_day_format_message(format, what, None),
            });
        }
        if kernel::maps_children(field) {
            time_of_day_formats(index, field.children(), path, out);
        }
        path.pop();
    }
}

/// Whether what flusso writes into `field` is a column's value that may be a
/// time of day: a `timestamp` column, or a `min`/`max` or `ids` typed so.
fn decided_by_its_column(field: &Field) -> bool {
    match &field.source {
        FieldSource::Column(column) => column.ty == FlussoType::Timestamp,
        FieldSource::Relation(Relation::Aggregate(aggregate)) => match &aggregate.op {
            AggregateOp::Min(_) | AggregateOp::Max(_) => {
                aggregate.value_type == Some(FlussoType::Timestamp)
            }
            AggregateOp::Ids { element_type } => *element_type == FlussoType::Timestamp,
            _ => false,
        },
        _ => false,
    }
}

const WRITES_A_DATE: &str = "a date (`2024-01-02`)";
const WRITES_A_TIMESTAMP: &str = "a timestamp (`2024-01-02T03:04:05`)";
const WRITES_EITHER: &str = "a date or a timestamp (`2024-01-02`, `2024-01-02T03:04:05`)";

/// What [`DiagnosticKind::TimeOfDayFormat`] says: `format` refuses `what`
/// flusso writes, read from a column of `sql_type` when known.
fn time_of_day_format_message(format: &str, what: &str, sql_type: Option<&str>) -> String {
    let from = sql_type
        .map(|ty| format!(", from the column's `{ty}`"))
        .unwrap_or_default();
    format!(
        "the `format` `{format}` reads a time of day only, but flusso writes {what} here{from}, \
         so OpenSearch refuses every document whose value is not null. Drop the `format`, or \
         use one that reads it: `strict_date_optional_time`, or yours followed by \
         `||strict_date_optional_time`"
    )
}

/// Where a level of fields sits, for [`duplicate_names`].
#[derive(Clone, Copy)]
struct Level {
    /// Every document carries the level (see [`carries`]).
    carried: bool,
    /// How far it reaches: indexed, stored, or under an input-only field.
    reach: crate::Reach,
    /// A running script reads it through `this`: one at its level or above,
    /// none of them under an input-only field (that subtree runs no script).
    read_by_this: bool,
    /// A running script of the index names `root`, which holds every level.
    read_by_root: bool,
}

impl Level {
    fn root(fields: &[Field]) -> Self {
        Self {
            carried: true,
            reach: crate::Reach::Indexed,
            read_by_this: has_script(fields),
            read_by_root: names_root(fields),
        }
    }

    /// The level of `field`'s children.
    fn under(self, field: &Field) -> Self {
        let reach = self.reach.max(reach_of(field));
        Self {
            carried: self.carried && carries(field),
            reach,
            read_by_this: self.read_by_this
                || (reach != crate::Reach::Absent && has_script(field.children())),
            read_by_root: self.read_by_root,
        }
    }

    /// The reach the namesakes `same` at this level are judged at: when
    /// they are, or are under, an input-only field, whether a script reads
    /// them.
    fn judged(self, same: &[&Field]) -> crate::Reach {
        let read = self.read_by_this || self.read_by_root;
        let input_only = same.iter().all(|field| !field.emit);
        match self.reach {
            crate::Reach::Absent if read => crate::Reach::Scripts,
            crate::Reach::Indexed | crate::Reach::Stored if input_only && read => {
                crate::Reach::Scripts
            }
            reach => reach,
        }
    }
}

/// A script that runs among `fields`: an emitted one (an input-only script
/// is stripped unrun).
fn has_script(fields: &[Field]) -> bool {
    fields
        .iter()
        .any(|field| field.emit && matches!(field.source, FieldSource::Script(_)))
}

/// Whether a script that runs, anywhere in `fields`, names `root`: the
/// scripts crate builds `root` for such a script. Nothing under an input-only
/// field runs.
fn names_root(fields: &[Field]) -> bool {
    fields.iter().filter(|field| field.emit).any(|field| {
        matches!(&field.source, FieldSource::Script(script) if script.source.contains("root"))
            || names_root(field.children())
    })
}

/// One [`DiagnosticKind::DuplicateFieldName`] per name declared more than
/// once at a level of `fields`, at any depth, anchored at that name. A pair
/// that never gives an indexed document gets its own kind, but only where
/// every document carries the level; one whose documents (or, under an
/// input-only field, the scripts reading it) hold something other than the
/// schema declares is [`DiagnosticKind::DuplicateFieldNameWrong`].
fn duplicate_names(
    index: &IndexName,
    fields: &[Field],
    level: Level,
    path: &mut Vec<(FieldName, Option<usize>)>,
    out: &mut Vec<Diagnostic>,
) {
    let mut reported: Vec<&FieldName> = Vec::new();
    for field in fields {
        let name = &field.field;
        if reported.contains(&name) {
            continue;
        }
        let same: Vec<(usize, &Field)> = fields
            .iter()
            .enumerate()
            .filter(|(_, other)| other.field == *name)
            .collect();
        if same.len() < 2 {
            continue;
        }
        reported.push(name);
        path.push((name.clone(), None));
        let fields: Vec<&Field> = same.iter().map(|(_, field)| *field).collect();
        let reach = level.judged(&fields);
        let never = level
            .carried
            .then(|| crate::never_indexed(&fields))
            .flatten();
        let kind = match never {
            Some(crate::NeverIndexed::FailsToBuild) => {
                DiagnosticKind::DuplicateFieldNameFailsToBuild
            }
            Some(crate::NeverIndexed::Refused) => DiagnosticKind::DuplicateFieldNameRefused,
            None if crate::duplicate_is_wrong(&fields, reach) => {
                DiagnosticKind::DuplicateFieldNameWrong
            }
            None => DiagnosticKind::DuplicateFieldName,
        };
        out.push(Diagnostic {
            index: index.clone(),
            field: name.clone(),
            path: path.iter().map(|(name, _)| name.clone()).collect(),
            positions: path.iter().map(|(_, at)| *at).collect(),
            anchor: Anchor::Field,
            kind,
            severity: kind.severity(),
            message: crate::duplicates::duplicate_message(name, &same, reach),
        });
        path.pop();
    }
    for (at, field) in fields.iter().enumerate() {
        path.push((field.field.clone(), position(fields, at)));
        duplicate_names(index, field.children(), level.under(field), path, out);
        path.pop();
    }
}

/// Whether every document that carries `field`'s level carries its children
/// too, as mapped: a plain object (emitted, no `options`). A join may have no
/// related row; an input-only or unindexed field, or one with `options` (an
/// object with `enabled: false`, say), is not mapped as its children declare.
fn carries(field: &Field) -> bool {
    matches!(field.source, FieldSource::Group(_)) && field.emit && field.options.is_empty()
}

/// How far the fields under `field` reach on its account: nowhere under an
/// input-only field, stored only under an unindexed join or an object with
/// `enabled: false`.
fn reach_of(field: &Field) -> crate::Reach {
    if !field.emit {
        crate::Reach::Absent
    } else if crate::duplicates::unindexed(field) {
        crate::Reach::Stored
    } else {
        crate::Reach::Indexed
    }
}

/// The 1-based position of `fields[at]` when another field of `fields` has
/// its name.
fn position(fields: &[Field], at: usize) -> Option<usize> {
    let name = &fields.get(at)?.field;
    let shared = fields.iter().filter(|other| other.field == *name).count() > 1;
    shared.then_some(at + 1)
}

const TABLE: &str = "table";
const PRIMARY_KEY: &str = "primary_key";
const FILTERS: &str = "filters";
const SOFT_DELETE: &str = "soft_delete";

/// An index-level schema key as a diagnostic's anchor. Every key is a valid
/// field name, so this is never `None`.
fn schema_key(key: &str) -> Option<FieldName> {
    FieldName::try_new(key).ok()
}

/// Why a `soft_delete` `field` reads no column, when it doesn't.
fn marker_field_problem(schema: &IndexSchema) -> Option<String> {
    let Some(SoftDelete::Field(marker)) = &schema.soft_delete else {
        return None;
    };
    let name = &marker.field;
    match first_named(&schema.fields, name) {
        None => Some(format!(
            "`field` names `{name}`, but no field has that name"
        )),
        Some((field, _)) if field.column().is_none() => Some(format!(
            "`field` names `{name}`, but the first field of that name is not a column"
        )),
        Some(_) => None,
    }
}

/// One index's walk: where it is (`path`), what it has found (`out`), and the
/// tables it already looked up.
struct Checker<'a> {
    index: &'a IndexName,
    db_schema: &'a DatabaseSchema,
    catalog: &'a dyn Catalog,
    tables: HashMap<TableName, bool>,
    primary_keys: HashMap<TableName, Option<Vec<ColumnName>>>,
    /// Whether each table records nullability, asked once per table.
    nullability: HashMap<TableName, bool>,
    path: Vec<FieldName>,
    /// The [`position`] of each field segment of `path` (index-level keys,
    /// pushed after the fields, have none).
    positions: Vec<Option<usize>>,
    anchor: Anchor,
    /// How many enclosing fields some document lacks the subtree of, as
    /// mapped: any field but a plain object (see [`carries`]).
    uncarried: usize,
    /// Whether the field being checked is mapped as its own declaration says,
    /// so a time of day under a `date` mapping is refused
    /// ([`DiagnosticKind::TimeOfDayAsDate`]).
    date_mapped: bool,
    /// How many enclosing fields OpenSearch doesn't map the children of: an
    /// input-only field, an `indexed: false` join, an `enabled: false` object.
    unmapped: usize,
    /// The `format` of the field being checked, when OpenSearch maps it as a
    /// date ([`kernel::date_format`]).
    date_format: Option<String>,
    out: &'a mut Vec<Diagnostic>,
}

const NUMERIC: &[FlussoType] = &[
    FlussoType::Double,
    FlussoType::Float,
    FlussoType::Decimal,
    FlussoType::Integer,
    FlussoType::Long,
    FlussoType::Short,
];

fn is_numeric(sql_type: &str) -> bool {
    NUMERIC.iter().any(|ty| ty.accepts_pg(sql_type))
}

/// A `date` format that reads every `time` value (any number of fraction
/// digits), led by the default pattern so the ISO form still reads.
const TIME_FORMAT: &str =
    "strict_date_optional_time||HH:mm:ss[.SSSSSS][.SSSSS][.SSSS][.SSS][.SS][.S]";

/// A `date` format that reads every `timetz` value Postgres writes: any
/// number of fraction digits, an offset with or without minutes.
const TIMETZ_FORMAT: &str = "strict_date_optional_time||\
                             HH:mm:ss[.SSSSSS][.SSSSS][.SSSS][.SSS][.SS][.S]XXX||\
                             HH:mm:ss[.SSSSSS][.SSSSS][.SSSS][.SSS][.SS][.S]X";

/// Whether a Postgres type (array and precision ignored) is a time of day:
/// `Some(true)` with a zone (`timetz`), `Some(false)` without, else `None`.
fn time_of_day_type(sql_type: &str) -> Option<bool> {
    let base = sql_type.trim().trim_end_matches("[]").to_ascii_lowercase();
    let bare = match base.split_once('(') {
        Some((head, rest)) => match rest.split_once(')') {
            Some((_, tail)) => format!("{head}{tail}"),
            None => base.clone(),
        },
        None => base.clone(),
    };
    match bare.trim() {
        "time" | "time without time zone" => Some(false),
        "timetz" | "time with time zone" => Some(true),
        _ => None,
    }
}

impl Checker<'_> {
    /// Report a disagreement on the field at the current path.
    fn report(&mut self, kind: DiagnosticKind, message: String) {
        let Some(field) = self.path.last() else {
            return;
        };
        self.out.push(Diagnostic {
            index: self.index.clone(),
            field: field.clone(),
            path: self.path.clone(),
            positions: (0..self.path.len())
                .map(|i| self.positions.get(i).copied().flatten())
                .collect(),
            anchor: self.anchor,
            kind,
            severity: kind.severity(),
            message,
        });
    }

    /// The root's own keys, on the root table: its primary key and the
    /// declared `primary_key` column, the `filters` columns, then the
    /// `soft_delete` marker's column and `when` columns. A missing root table
    /// is already reported on the fields.
    async fn root(&mut self, schema: &IndexSchema) -> Result<()> {
        self.anchor = Anchor::IndexKey;
        if self.has(&schema.table).await? {
            self.root_key(
                &schema.table,
                schema.primary_key.as_ref(),
                has_relation(&schema.fields),
            )
            .await?;
        }
        if let Some(key) = schema_key(FILTERS) {
            self.path.push(key);
            self.filters(&schema.table, schema.filters.as_deref(), FILTERS)
                .await?;
            self.path.pop();
        }
        let Some(marker) = &schema.soft_delete else {
            return Ok(());
        };
        let Some(key) = schema_key(SOFT_DELETE) else {
            return Ok(());
        };
        self.path.push(key);
        let when = match marker {
            SoftDelete::Column(marker) => {
                self.exists(&schema.table, &marker.column, "column").await?;
                marker.when.as_deref()
            }
            SoftDelete::Field(marker) => {
                if let Some(message) = marker_field_problem(schema) {
                    self.report(DiagnosticKind::UnreadSoftDeleteMarker, message);
                }
                // A root-level field's column was checked with the field. One
                // found inside a relation is still read on the root row.
                if let Some((field, true)) = first_named(&schema.fields, &marker.field)
                    && let Some(column) = field.column()
                {
                    self.exists(&schema.table, column, "field").await?;
                }
                marker.when.as_deref()
            }
        };
        self.filters(&schema.table, when, "when").await?;
        self.path.pop();
        Ok(())
    }

    /// The root table's own primary key, and the declared `primary_key`
    /// against it. Changes and the backfill key a root row by the table's
    /// primary key; the batched build matches it on the declared column.
    ///
    /// - A single-column key: the declared column must exist
    ///   ([`DiagnosticKind::UnknownColumn`]), be that key
    ///   ([`DiagnosticKind::RootPrimaryKeyMismatch`]), and be of a type the
    ///   source reads back ([`DiagnosticKind::UnsupportedKeyType`]).
    /// - None or a composite one: [`DiagnosticKind::RootPrimaryKey`] at
    ///   `table`. A root change is then built by its whole key, so a declared
    ///   column that doesn't exist is only read by a relation (which keys on
    ///   the root, or traces a change back to it): with none, it is reported
    ///   under the same kind; with one, as an unknown column.
    /// - Unknown to the catalog: only the declared column's existence.
    async fn root_key(
        &mut self,
        table: &TableName,
        declared: Option<&ColumnName>,
        related: bool,
    ) -> Result<()> {
        let key = self.primary_key(table).await?;
        let single = match key.as_deref() {
            Some([single]) => Some(single.clone()),
            Some(columns) => {
                let problem = match columns {
                    [] => "has no primary key".to_owned(),
                    composite => format!("has a composite primary key ({})", quoted(composite)),
                };
                self.at(TABLE, |checker| {
                    checker.report(
                        DiagnosticKind::RootPrimaryKey,
                        format!(
                            "`{}.{table}` {problem}: the backfill skips it; live changes are \
                             still indexed",
                            checker.db_schema
                        ),
                    );
                });
                None
            }
            None => None,
        };
        let Some(declared) = declared else {
            return Ok(());
        };
        let Some(field) = schema_key(PRIMARY_KEY) else {
            return Ok(());
        };
        self.path.push(field);
        if key.is_some() && single.is_none() && !related {
            match self.catalog.column(self.db_schema, table, declared).await {
                Ok(_) => {}
                Err(SourceError::UnknownColumn(what)) => self.report(
                    DiagnosticKind::RootPrimaryKey,
                    format!(
                        "`primary_key` references unknown column `{what}`; a root change \
                         built by its whole key does not read it"
                    ),
                ),
                Err(e) => return Err(e),
            }
        } else if let Some(info) = self.column(table, declared, Some(PRIMARY_KEY)).await? {
            match single {
                Some(single) if single != *declared => self.report(
                    DiagnosticKind::RootPrimaryKeyMismatch,
                    format!(
                        "`primary_key` is `{declared}`, but `{}.{table}` is keyed by \
                         `{single}`: the backfill and every root change would match the wrong \
                         row or none",
                        self.db_schema
                    ),
                ),
                Some(_)
                    if !self
                        .catalog
                        .reads_key(self.db_schema, table, declared)
                        .await? =>
                {
                    self.report(
                        DiagnosticKind::UnsupportedKeyType,
                        format!(
                            "`primary_key` `{declared}` is `{}`, a type that can't be read back \
                             as a document key: the backfill would index no row, and no change \
                             on a related table would reach a document",
                            info.sql_type
                        ),
                    );
                }
                _ => {}
            }
        }
        self.path.pop();
        Ok(())
    }

    /// Run `check` with the path at the index-level `key`.
    fn at(&mut self, key: &str, check: impl FnOnce(&mut Self)) {
        if let Some(key) = schema_key(key) {
            self.path.push(key);
            check(self);
            self.path.pop();
        }
    }

    /// Validate a list of fields under `table`. Boxed because the recursion is
    /// through an `async fn`.
    fn fields<'s>(
        &'s mut self,
        table: &'s TableName,
        fields: &'s [Field],
        primary_key: Option<&'s ColumnName>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 's>> {
        Box::pin(async move {
            for (at, field) in fields.iter().enumerate() {
                self.path.push(field.field.clone());
                self.positions.push(position(fields, at));
                self.field(table, field, primary_key).await?;
                self.positions.pop();
                self.path.pop();
            }
            Ok(())
        })
    }

    async fn field(
        &mut self,
        table: &TableName,
        field: &Field,
        primary_key: Option<&ColumnName>,
    ) -> Result<()> {
        // A namesake's value is mapped by the last one of the name, so only an
        // unshared field is known to be mapped as it declares.
        let shared = self.positions.last().copied().flatten().is_some();
        self.date_mapped = self.uncarried == 0 && field.emit && !shared && field.options.is_empty();
        self.date_format = (self.unmapped == 0)
            .then(|| kernel::date_format(field))
            .flatten()
            .map(str::to_owned);
        let hides = !carries(field);
        if hides {
            self.uncarried += 1;
        }
        let unmaps = !field.emit || !kernel::maps_children(field);
        if unmaps {
            self.unmapped += 1;
        }
        let checked = match &field.source {
            FieldSource::Column(column) => self.column_field(table, column, primary_key).await,
            FieldSource::Relation(Relation::Aggregate(aggregate)) => {
                self.aggregate(table, aggregate).await
            }
            FieldSource::Relation(Relation::Join(join)) => self.join(table, join).await,
            FieldSource::Group(fields) => self.fields(table, fields, primary_key).await,
            FieldSource::Geo(geo) => self.geo(table, geo).await,
            FieldSource::Constant(_) | FieldSource::Script(_) => Ok(()),
        };
        if hides {
            self.uncarried -= 1;
        }
        if unmaps {
            self.unmapped -= 1;
        }
        checked
    }

    /// For a `date` value read from a timestamp column, what its `format`
    /// does with the timestamp flusso writes there when it refuses it (a
    /// `TypeMismatch` suffix); empty otherwise. The append rule follows the
    /// declared type, so no ISO pattern reads it.
    fn date_format_refuses(&self, ty: &FlussoType, info: &ColumnInfo) -> String {
        let base = info.base_type.as_deref().unwrap_or(&info.sql_type);
        let Some(format) = self.date_format.as_deref() else {
            return String::new();
        };
        if *ty != FlussoType::Date
            || !FlussoType::Timestamp.accepts_pg(base)
            || time_of_day_type(base).is_some()
            || kernel::reads(format, kernel::DateValues::Timestamps)
            || kernel::reads_time_of_day_only(format)
        {
            return String::new();
        }
        format!(
            ": flusso writes the column's value, {WRITES_A_TIMESTAMP}, which the `format` \
             `{format}` refuses, so OpenSearch refuses every document whose value is not null. \
             Declare the field `timestamp`, or drop the `format`"
        )
    }

    /// Report a `timestamp`-typed value read from a column that holds no time
    /// of day under a time-of-day `format` ([`DiagnosticKind::TimeOfDayFormat`]).
    fn time_of_day_format(&mut self, ty: &FlussoType, info: &ColumnInfo) {
        if *ty != FlussoType::Timestamp {
            return;
        }
        let base = info.base_type.as_deref().unwrap_or(&info.sql_type);
        if time_of_day_type(base).is_some() {
            return;
        }
        let Some(format) = self
            .date_format
            .clone()
            .filter(|format| kernel::reads_time_of_day_only(format))
        else {
            return;
        };
        let what = if FlussoType::Date.accepts_pg(base) {
            WRITES_A_DATE
        } else {
            WRITES_A_TIMESTAMP
        };
        self.report(
            DiagnosticKind::TimeOfDayFormat,
            time_of_day_format_message(&format, what, Some(&info.sql_type)),
        );
    }

    /// Report a `timestamp` or `date` field over a `time`/`timetz` column
    /// ([`DiagnosticKind::TimeOfDayAsDate`]) when every document carries it,
    /// mapped as declared (`date_mapped`); `true` when reported.
    fn time_of_day(&mut self, ty: &FlussoType, info: &ColumnInfo) -> bool {
        let base = info.base_type.as_deref().unwrap_or(&info.sql_type);
        let Some(zoned) = time_of_day_type(base) else {
            return false;
        };
        let declared = match ty {
            FlussoType::Timestamp => "timestamp",
            FlussoType::Date => "date",
            _ => return false,
        };
        if !self.date_mapped {
            return false;
        }
        let (postgres, format) = if zoned {
            ("time with time zone", TIMETZ_FORMAT)
        } else {
            ("time without time zone", TIME_FORMAT)
        };
        self.report(
            DiagnosticKind::TimeOfDayAsDate,
            format!(
                "a `{declared}` field over the column's database type `{}`: a time of day is no \
                 date to OpenSearch, so it refuses every document whose value is not null. \
                 Index it as text with a `custom` field (`postgres: [{postgres}]`, `opensearch: \
                 keyword`), or as a `timestamp` field (a `date` field takes only a `date` \
                 column) with `options: {{ format: \"{format}\" }}`",
                info.sql_type
            ),
        );
        true
    }

    /// Whether `table` exists (cached).
    async fn has(&mut self, table: &TableName) -> Result<bool> {
        if let Some(exists) = self.tables.get(table) {
            return Ok(*exists);
        }
        let exists = self.catalog.has_table(self.db_schema, table).await?;
        self.tables.insert(table.clone(), exists);
        Ok(exists)
    }

    /// The primary key of an existing `table` (cached); `None` when the
    /// catalog can't say.
    async fn primary_key(&mut self, table: &TableName) -> Result<Option<Vec<ColumnName>>> {
        if let Some(columns) = self.primary_keys.get(table) {
            return Ok(columns.clone());
        }
        let columns = self.catalog.primary_key(self.db_schema, table).await?;
        self.primary_keys.insert(table.clone(), columns.clone());
        Ok(columns)
    }

    /// A relation's existing `table`, named by `role`, must have a
    /// single-column primary key: relations are joined and traced through it.
    async fn relation_key(&mut self, table: &TableName, role: &str) -> Result<()> {
        let Some(columns) = self.primary_key(table).await? else {
            return Ok(());
        };
        let problem = match columns.as_slice() {
            [_] => return Ok(()),
            [] => "has no primary key".to_owned(),
            composite => format!("has a composite primary key ({})", quoted(composite)),
        };
        self.report(
            DiagnosticKind::RelationPrimaryKey,
            format!(
                "`{role}` `{}.{table}` {problem}; a relation table needs a single-column one",
                self.db_schema
            ),
        );
        Ok(())
    }

    /// Whether `table` exists, reporting it under `role` when it doesn't.
    async fn table(&mut self, table: &TableName, role: &str) -> Result<bool> {
        let exists = self.has(table).await?;
        if !exists {
            self.report(
                DiagnosticKind::UnknownTable,
                format!(
                    "`{role}` references unknown table `{}.{table}`",
                    self.db_schema
                ),
            );
        }
        Ok(exists)
    }

    /// Look up a column, turning a missing column or table into an Error
    /// [`Diagnostic`] on the field (prefixed with the schema key that names it,
    /// when given) instead of aborting. A real transport failure (the store
    /// became unreachable) still propagates. `None` when it was missing.
    async fn column(
        &mut self,
        table: &TableName,
        column: &ColumnName,
        role: Option<&str>,
    ) -> Result<Option<ColumnInfo>> {
        let prefix = role.map(|role| format!("`{role}` ")).unwrap_or_default();
        match self.catalog.column(self.db_schema, table, column).await {
            Ok(info) => Ok(Some(info)),
            Err(SourceError::UnknownColumn(what)) => {
                self.report(
                    DiagnosticKind::UnknownColumn,
                    format!("{prefix}references unknown column `{what}`"),
                );
                Ok(None)
            }
            Err(SourceError::UnknownTable(what)) => {
                self.report(
                    DiagnosticKind::UnknownTable,
                    format!("{prefix}references unknown table `{what}`"),
                );
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    async fn exists(&mut self, table: &TableName, column: &ColumnName, role: &str) -> Result<()> {
        self.column(table, column, Some(role)).await.map(drop)
    }

    /// A join's declared `primary_key`. Relations are keyed by the table's own
    /// primary key from the catalog, so a name that misses is ignored: a
    /// warning, not an unknown column.
    async fn join_primary_key(&mut self, table: &TableName, column: &ColumnName) -> Result<()> {
        match self.catalog.column(self.db_schema, table, column).await {
            Ok(_) => Ok(()),
            Err(SourceError::UnknownColumn(what)) => {
                self.report(
                    DiagnosticKind::IgnoredPrimaryKey,
                    format!(
                        "`primary_key` references unknown column `{what}`; ignored: \
                         the join is keyed by the table's own primary key"
                    ),
                );
                Ok(())
            }
            Err(SourceError::UnknownTable(what)) => {
                self.report(
                    DiagnosticKind::UnknownTable,
                    format!("`primary_key` references unknown table `{what}`"),
                );
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// The columns value and null-check filters name, on `table`.
    async fn filters(
        &mut self,
        table: &TableName,
        filters: Option<&[Filter]>,
        role: &str,
    ) -> Result<()> {
        for filter in filters.unwrap_or_default() {
            let column = match filter {
                Filter::ValueOp(filter) => &filter.column,
                Filter::NullCheck(filter) => &filter.column,
                Filter::Raw(_) => continue,
            };
            self.exists(table, column, role).await?;
        }
        Ok(())
    }

    /// Both sides of each `on:` pair: `local` on `before` (skipped when that
    /// table is missing), `related` on `related` (likewise), and that the two
    /// compare.
    async fn on_pairs(
        &mut self,
        on: &[ColumnPair],
        before: Option<&TableName>,
        related: Option<&TableName>,
        role: &str,
    ) -> Result<()> {
        for pair in on {
            if let Some(before) = before {
                self.exists(before, &pair.local, &format!("{role}.local"))
                    .await?;
            }
            if let Some(related) = related {
                self.exists(related, &pair.related, &format!("{role}.related"))
                    .await?;
            }
            if let (Some(before), Some(related)) = (before, related) {
                self.link(
                    &format!("{role}.related"),
                    (related, &pair.related),
                    (before, &pair.local),
                )
                .await?;
            }
        }
        Ok(())
    }

    /// The single-column primary key of an existing `table`, when it has one
    /// and the catalog can say.
    async fn single_key(&mut self, table: &TableName) -> Result<Option<ColumnName>> {
        Ok(match self.primary_key(table).await?.as_deref() {
            Some([single]) => Some(single.clone()),
            _ => None,
        })
    }

    /// A link column, named by `role`, against the key column it names on
    /// `table`'s single-column primary key: reported when the two don't
    /// compare. Skipped when the table has no such key, or either column is
    /// missing (reported where it is named).
    async fn link_to_key(
        &mut self,
        role: &str,
        link: (&TableName, &ColumnName),
        table: &TableName,
    ) -> Result<()> {
        match self.single_key(table).await? {
            Some(key) => self.link(role, link, (table, &key)).await,
            None => Ok(()),
        }
    }

    /// A link column, named by `role`, against the column it is joined to:
    /// reported when the catalog says the two don't compare. Skipped when
    /// either column is missing.
    async fn link(
        &mut self,
        role: &str,
        link: (&TableName, &ColumnName),
        key: (&TableName, &ColumnName),
    ) -> Result<()> {
        let (Some(link_info), Some(key_info)) = (
            self.quiet_column(link).await?,
            self.quiet_column(key).await?,
        ) else {
            return Ok(());
        };
        match self.catalog.compares(self.db_schema, link, key).await {
            Ok(true) => return Ok(()),
            Ok(false) => {}
            Err(SourceError::Unchecked(reason)) => {
                self.report(
                    DiagnosticKind::CheckSkipped,
                    format!(
                        "could not check `{role}` `{}.{}` (`{}`) against `{}.{}` (`{}`): {reason}",
                        link.0, link.1, link_info.sql_type, key.0, key.1, key_info.sql_type,
                    ),
                );
                return Ok(());
            }
            Err(e) => return Err(e),
        }
        self.report(
            DiagnosticKind::LinkTypeMismatch,
            format!(
                "`{role}` `{}.{}` (`{}`) can't be compared with `{}.{}` (`{}`): every document \
                 build fails",
                link.0, link.1, link_info.sql_type, key.0, key.1, key_info.sql_type,
            ),
        );
        Ok(())
    }

    /// A column's info, or `None` when it or its table is missing, without
    /// reporting it: the check that names the column reports that.
    async fn quiet_column(
        &mut self,
        (table, column): (&TableName, &ColumnName),
    ) -> Result<Option<ColumnInfo>> {
        match self.catalog.column(self.db_schema, table, column).await {
            Ok(info) => Ok(Some(info)),
            Err(SourceError::UnknownColumn(_) | SourceError::UnknownTable(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// An `ids` aggregate's `element_type` against the type of the key it
    /// collects: the primary key of `table`.
    async fn ids_element(&mut self, table: &TableName, element: &FlussoType) -> Result<()> {
        let Some(key) = self.single_key(table).await? else {
            return Ok(());
        };
        let Some(info) = self.quiet_column((table, &key)).await? else {
            return Ok(());
        };
        self.time_of_day_format(element, &info);
        let key_type = info.base_type.as_deref().unwrap_or(&info.sql_type);
        let element_name = element::name(element);
        let sql_type = &info.sql_type;
        match element::fit(element, key_type) {
            Fit::Holds => {}
            Fit::Rejects => self.report(
                DiagnosticKind::IdsElementMismatch,
                format!(
                    "`element_type` `{element_name}` can't hold the `{sql_type}` keys of \
                     `{table}`: OpenSearch rejects every document that collects one"
                ),
            ),
            Fit::Some => self.report(
                DiagnosticKind::IdsElementLossy,
                format!(
                    "`element_type` `{element_name}` holds only some `{sql_type}` keys of \
                     `{table}`: OpenSearch rejects a document that collects another, or stores \
                     the key altered (a fraction truncated), so two keys can become \
                     equal{LOSSY_FIX}"
                ),
            ),
            Fit::Changes(how) => self.report(
                DiagnosticKind::IdsElementLossy,
                format!(
                    "`element_type` `{element_name}` changes the `{sql_type}` keys of \
                     `{table}`: {how}{LOSSY_FIX}"
                ),
            ),
        }
        Ok(())
    }

    /// A junction's table, both keys, and its filters.
    async fn through(&mut self, through: &Through, role: &str) -> Result<()> {
        if !self.table(&through.table, &format!("{role}.table")).await? {
            return Ok(());
        }
        self.exists(
            &through.table,
            &through.left_key,
            &format!("{role}.left_key"),
        )
        .await?;
        self.exists(
            &through.table,
            &through.right_key,
            &format!("{role}.right_key"),
        )
        .await?;
        self.filters(
            &through.table,
            through.filters.as_deref(),
            &format!("{role}.filters"),
        )
        .await
    }

    /// A junction's keys against the keys they name: `left_key` against
    /// `parent`'s, `right_key` against the far table's (when it exists).
    async fn through_links(
        &mut self,
        through: &Through,
        parent: &TableName,
        far: Option<&TableName>,
    ) -> Result<()> {
        self.link_to_key(
            "through.left_key",
            (&through.table, &through.left_key),
            parent,
        )
        .await?;
        if let Some(far) = far {
            self.link_to_key(
                "through.right_key",
                (&through.table, &through.right_key),
                far,
            )
            .await?;
        }
        Ok(())
    }

    /// A join under `parent`: its table, key, `on:` pairs, primary key,
    /// filters and `order_by`, then its fields.
    async fn join(&mut self, parent: &TableName, join: &Join) -> Result<()> {
        let related = self.table(&join.table, "table").await?;
        match &join.kind {
            JoinKind::BelongsTo { column } => {
                self.exists(parent, column, "column").await?;
                if related {
                    self.link_to_key("column", (parent, column), &join.table)
                        .await?;
                }
            }
            JoinKind::HasOne { foreign_key } | JoinKind::HasMany { foreign_key } => {
                if related {
                    self.exists(&join.table, foreign_key, "foreign_key").await?;
                    self.link_to_key("foreign_key", (&join.table, foreign_key), parent)
                        .await?;
                }
            }
            JoinKind::ManyToMany { through } => {
                self.through(through, "through").await?;
                self.through_links(through, parent, related.then_some(&join.table))
                    .await?;
            }
        }
        self.on_pairs(&join.on, Some(parent), related.then_some(&join.table), "on")
            .await?;
        if !related {
            return Ok(());
        }
        self.relation_key(&join.table, "table").await?;
        self.join_primary_key(&join.table, &join.primary_key)
            .await?;
        self.filters(&join.table, join.filters.as_deref(), "filters")
            .await?;
        for order in join.order_by.as_deref().unwrap_or_default() {
            self.exists(&join.table, &order.column, "order_by").await?;
        }
        self.fields(&join.table, &join.fields, Some(&join.primary_key))
            .await
    }

    /// An aggregate under `parent`: each `via` hop, then the aggregated table,
    /// its key, `on:` pairs, filters and input.
    async fn aggregate(&mut self, parent: &TableName, aggregate: &Aggregate) -> Result<()> {
        let mut before = Some(parent);
        for hop in &aggregate.via {
            let exists = self.table(&hop.table, "via.table").await?;
            let here = exists.then_some(&hop.table);
            if let Some(table) = here {
                self.relation_key(table, "via.table").await?;
                self.exists(table, &hop.foreign_key, "via.foreign_key")
                    .await?;
                if let Some(before) = before {
                    self.link_to_key("via.foreign_key", (table, &hop.foreign_key), before)
                        .await?;
                }
            }
            self.on_pairs(&hop.on, before, here, "via.on").await?;
            if let Some(table) = here {
                self.filters(table, hop.filters.as_deref(), "via.filters")
                    .await?;
            }
            before = here;
        }
        let related = self.table(&aggregate.table, "table").await?;
        if related {
            self.relation_key(&aggregate.table, "table").await?;
        }
        match &aggregate.key {
            AggregateKey::Direct(foreign_key) => {
                if related {
                    self.exists(&aggregate.table, foreign_key, "foreign_key")
                        .await?;
                    if let Some(before) = before {
                        self.link_to_key("foreign_key", (&aggregate.table, foreign_key), before)
                            .await?;
                    }
                }
            }
            AggregateKey::Through(through) => {
                self.through(through, "through").await?;
                if let Some(before) = before {
                    self.through_links(through, before, related.then_some(&aggregate.table))
                        .await?;
                }
            }
        }
        self.on_pairs(
            &aggregate.on,
            before,
            related.then_some(&aggregate.table),
            "on",
        )
        .await?;
        if !related {
            return Ok(());
        }
        self.filters(&aggregate.table, aggregate.filters.as_deref(), "filters")
            .await?;
        if let AggregateOp::Ids { element_type } = &aggregate.op {
            self.ids_element(&aggregate.table, element_type).await?;
        }
        self.aggregate_input(aggregate).await
    }

    /// Check the input a `sum`/`avg`/`min`/`max` folds. A plain column keeps the
    /// one check it always had: `sum`/`min`/`max` declare a `value_type` that must
    /// accept the column's type. An expression's columns must each exist on the
    /// aggregated table and be numeric.
    async fn aggregate_input(&mut self, aggregate: &Aggregate) -> Result<()> {
        let Some(input) = aggregate.op.input() else {
            return Ok(());
        };
        if let Some(column) = input.as_column() {
            if let (false, Some(value_type)) = (
                matches!(aggregate.op, AggregateOp::Avg(_)),
                &aggregate.value_type,
            ) {
                let Some(info) = self.column(&aggregate.table, column, None).await? else {
                    return Ok(());
                };
                if matches!(aggregate.op, AggregateOp::Min(_) | AggregateOp::Max(_)) {
                    self.time_of_day_format(value_type, &info);
                }
                if !value_type.accepts_pg(&info.sql_type) {
                    let refused = self.date_format_refuses(value_type, &info);
                    self.report(
                        DiagnosticKind::TypeMismatch,
                        format!(
                            "declared aggregate type does not accept the column's database type \
                             `{}`{refused}",
                            info.sql_type
                        ),
                    );
                }
            }
            return Ok(());
        }
        for column in input.columns() {
            let Some(info) = self.column(&aggregate.table, column, None).await? else {
                continue;
            };
            if !is_numeric(&info.sql_type) {
                self.report(
                    DiagnosticKind::NotNumeric,
                    format!(
                        "aggregate expression column `{column}` must be numeric, found `{}`",
                        info.sql_type
                    ),
                );
            }
        }
        Ok(())
    }

    /// Confirm both coordinate columns of a geo point exist and hold a numeric type.
    async fn geo(&mut self, table: &TableName, geo: &Geo) -> Result<()> {
        for column in [&geo.lat, &geo.lon] {
            let Some(info) = self.column(table, column, None).await? else {
                continue;
            };
            if !is_numeric(&info.sql_type) {
                self.report(
                    DiagnosticKind::NotNumeric,
                    format!(
                        "geo_point coordinate column `{column}` must be numeric, found `{}`",
                        info.sql_type
                    ),
                );
            }
        }
        Ok(())
    }

    async fn column_field(
        &mut self,
        table: &TableName,
        column: &Column,
        primary_key: Option<&ColumnName>,
    ) -> Result<()> {
        let Some(info) = self.column(table, &column.column, None).await? else {
            return Ok(());
        };

        let structure = info.base_type.as_deref().unwrap_or(&info.sql_type);
        let holds_array = is_array_type(structure);
        let holds_json = is_json_type(structure);
        let database_type = match &info.base_type {
            Some(base) => format!("`{}` (a domain over `{base}`)", info.sql_type),
            None => format!("`{}`", info.sql_type),
        };
        if column.array && !holds_array && !holds_json {
            self.report(
                DiagnosticKind::NotAnArrayColumn,
                format!(
                    "declares `array: true`, but the column's database type {database_type} holds \
                     no array (a `[]` type or `json`/`jsonb` does)"
                ),
            );
        }
        if !column.array && holds_array {
            let advice = match &column.ty {
                FlussoType::Map { .. } | FlussoType::GeoPoint => format!(
                    "a `{}` field takes no `array: true`; read the column with a field type that \
                     does (a `json` field, say)",
                    if matches!(column.ty, FlussoType::GeoPoint) {
                        "geo"
                    } else {
                        "map"
                    }
                ),
                _ => "declare `array: true` (that changes the index's content hash, so the \
                      index is rebuilt, and turns on the array clean-up: null elements dropped, \
                      a lone value wrapped)"
                    .to_owned(),
            };
            self.report(
                DiagnosticKind::UndeclaredArrayColumn,
                format!(
                    "the column's database type {database_type} is an array, but the field does \
                     not declare `array: true`: the schema says one value while the index holds \
                     a list, so a consumer reading by the schema fails on every document holding \
                     one; {advice}"
                ),
            );
        }

        // The elements of a `json`/`jsonb` array have no database type to
        // check a scalar declaration against.
        let unknown_elements = column.array
            && holds_json
            && !matches!(column.ty, FlussoType::Json | FlussoType::Custom { .. });
        let time_of_day = self.time_of_day(&column.ty, &info);
        self.time_of_day_format(&column.ty, &info);
        if !time_of_day && !unknown_elements && !column.ty.accepts_pg(&info.sql_type) {
            let refused = self.date_format_refuses(&column.ty, &info);
            self.report(
                DiagnosticKind::TypeMismatch,
                format!(
                    "declared type does not accept the column's database type `{}`{refused}",
                    info.sql_type
                ),
            );
        }

        if !column.transforms.is_empty() {
            let names = column
                .transforms
                .iter()
                .map(|transform| format!("`{}`", transform_name(*transform)))
                .collect::<Vec<_>>()
                .join(", ");
            let accepted = self
                .catalog
                .accepts_transforms(
                    self.db_schema,
                    table,
                    &column.column,
                    &column.transforms,
                    column.array,
                )
                .await;
            match accepted {
                Ok(true) => {}
                Ok(false) => self.report(
                    DiagnosticKind::TransformNotApplicable,
                    format!(
                        "`transforms` {names} do not apply to the column's database type `{}`",
                        info.sql_type
                    ),
                ),
                Err(SourceError::Unchecked(reason)) => self.report(
                    DiagnosticKind::CheckSkipped,
                    format!(
                        "could not check `transforms` {names} against the column's database \
                         type `{}`: {reason}",
                        info.sql_type
                    ),
                ),
                Err(e) => return Err(e),
            }
        }

        // A primary key or a `default` makes the field non-null regardless, so only
        // a plain non-null declaration over a nullable column is worth flagging.
        let forced_non_null = primary_key == Some(&column.column) || column.default.is_some();
        if !column.nullable && info.nullable && !forced_non_null {
            if !self.tracks_nullability(table).await? {
                self.report(
                    DiagnosticKind::NullabilityUntracked,
                    format!(
                        "declared non-null (`required: true`), but `{table}` records no \
                         nullability (a view, a materialized view or a foreign table reads every \
                         column as nullable): flusso cannot check it. The column must really \
                         never hold null; if it can, declare `required: false` or give the \
                         field a `default`"
                    ),
                );
                return Ok(());
            }
            self.report(
                DiagnosticKind::Nullability,
                "declared non-null (`required: true`) but the database column allows null: a \
                 document can hold null where the schema says it never does, so a consumer \
                 reading by the schema fails on it; declare `required: false`, or make the \
                 column `NOT NULL`, or give the field a `default`"
                    .to_owned(),
            );
        }
        Ok(())
    }
}

impl Checker<'_> {
    async fn tracks_nullability(&mut self, table: &TableName) -> Result<bool> {
        if let Some(tracked) = self.nullability.get(table) {
            return Ok(*tracked);
        }
        let tracked = self
            .catalog
            .tracks_nullability(self.db_schema, table)
            .await?;
        self.nullability.insert(table.clone(), tracked);
        Ok(tracked)
    }
}

/// Whether a `format_type` name is an array type (`text[]`, `integer[]`).
fn is_array_type(sql_type: &str) -> bool {
    sql_type.trim().ends_with("[]")
}

/// Whether a `format_type` name is `json` or `jsonb` (not an array of them).
fn is_json_type(sql_type: &str) -> bool {
    matches!(
        sql_type.trim().to_ascii_lowercase().as_str(),
        "json" | "jsonb"
    )
}

/// Whether any field, at any depth, is a relation.
fn has_relation(fields: &[Field]) -> bool {
    fields.iter().any(|field| match &field.source {
        FieldSource::Relation(_) => true,
        FieldSource::Group(nested) => has_relation(nested),
        FieldSource::Column(_)
        | FieldSource::Geo(_)
        | FieldSource::Constant(_)
        | FieldSource::Script(_) => false,
    })
}

/// Columns as `` `a`, `b` ``.
fn quoted(columns: &[ColumnName]) -> String {
    columns
        .iter()
        .map(|column| format!("`{column}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A transform as the schema spells it.
fn transform_name(transform: Transform) -> &'static str {
    match transform {
        Transform::Lowercase => "lowercase",
        Transform::Trim => "trim",
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeMap;

    use kernel::IndexSchema;
    use kernel::common::IndexName;
    use serde_json::json;

    use super::*;

    /// A catalog whose every column lookup fails as "unknown" — the post-stale-DB
    /// case from the designer.
    struct NoColumns;

    #[async_trait]
    impl Catalog for NoColumns {
        async fn column(
            &self,
            schema: &DatabaseSchema,
            table: &TableName,
            column: &ColumnName,
        ) -> Result<ColumnInfo> {
            Err(SourceError::UnknownColumn(format!(
                "{schema}.{table}.{column}"
            )))
        }

        async fn has_table(&self, _: &DatabaseSchema, _: &TableName) -> Result<bool> {
            Ok(true)
        }
    }

    /// An unknown column is a per-field Error diagnostic, not a fatal error that
    /// aborts validation (and gets mislabelled "database not reachable").
    #[test]
    fn unknown_column_is_a_diagnostic_not_a_fatal_error() {
        let schema: IndexSchema = serde_json::from_value(json!({
            "version": 1,
            "table": "products",
            "db_schema": "public",
            "fields": [{
                "field": "title",
                "source": { "column": { "column": "title", "ty": "keyword", "nullable": true } },
            }],
        }))
        .unwrap();
        let mut indexes = BTreeMap::new();
        indexes.insert(IndexName::try_new("products").unwrap(), schema);
        let spec = SourceSpec::new(indexes);

        let diagnostics = futures::executor::block_on(validate_indexes(&spec, &NoColumns)).unwrap();

        assert_eq!(
            diagnostics.len(),
            1,
            "the unknown column should produce one diagnostic"
        );
        assert_eq!(diagnostics[0].kind, DiagnosticKind::UnknownColumn);
        assert_eq!(diagnostics[0].severity, Severity::Error);
        assert!(
            diagnostics[0].message.contains("unknown column"),
            "got: {}",
            diagnostics[0].message,
        );
    }

    /// Every column unknown; each lookup yields `yields(table)` times first,
    /// counting how many lookups are in flight at once.
    #[derive(Default)]
    struct Yielding {
        in_flight: std::sync::atomic::AtomicUsize,
        peak: std::sync::atomic::AtomicUsize,
    }

    impl Yielding {
        /// Earlier tables answer later, so an ordered result is not an
        /// accident of finishing order.
        fn yields(table: &TableName) -> usize {
            let n: usize = table.as_ref().trim_start_matches('t').parse().unwrap();
            20 - n
        }
    }

    #[async_trait]
    impl Catalog for Yielding {
        async fn column(
            &self,
            schema: &DatabaseSchema,
            table: &TableName,
            column: &ColumnName,
        ) -> Result<ColumnInfo> {
            use std::sync::atomic::Ordering;
            let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak.fetch_max(now, Ordering::SeqCst);
            for _ in 0..Self::yields(table) {
                let mut yielded = false;
                std::future::poll_fn(|cx| {
                    if yielded {
                        std::task::Poll::Ready(())
                    } else {
                        yielded = true;
                        cx.waker().wake_by_ref();
                        std::task::Poll::Pending
                    }
                })
                .await;
            }
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
            Err(SourceError::UnknownColumn(format!(
                "{schema}.{table}.{column}"
            )))
        }

        async fn has_table(&self, _: &DatabaseSchema, _: &TableName) -> Result<bool> {
            Ok(true)
        }
    }

    /// Indexes are checked a few at a time (issue #398), and their findings
    /// still come back in the spec's index order.
    #[test]
    fn indexes_are_checked_concurrently_and_reported_in_order() {
        let mut indexes = BTreeMap::new();
        for i in 0..10 {
            let schema: IndexSchema = serde_json::from_value(json!({
                "version": 1,
                "table": format!("t{i}"),
                "db_schema": "public",
                "fields": [{
                    "field": "title",
                    "source": { "column": { "column": "title", "ty": "keyword", "nullable": true } },
                }],
            }))
            .unwrap();
            indexes.insert(IndexName::try_new(format!("index_{i}")).unwrap(), schema);
        }
        let spec = SourceSpec::new(indexes);
        let catalog = Yielding::default();

        let diagnostics = futures::executor::block_on(validate_indexes(&spec, &catalog)).unwrap();

        let named: Vec<&str> = diagnostics.iter().map(|d| d.index.as_ref()).collect();
        let expected: Vec<String> = (0..10).map(|i| format!("index_{i}")).collect();
        assert_eq!(named, expected);
        assert_eq!(
            catalog.peak.load(std::sync::atomic::Ordering::SeqCst),
            VALIDATE_CONCURRENCY
        );
    }

    /// `order_lines(order_id integer, quantity integer, unit_price numeric, note text)`.
    struct OrderLines;

    #[async_trait]
    impl Catalog for OrderLines {
        async fn column(
            &self,
            schema: &DatabaseSchema,
            table: &TableName,
            column: &ColumnName,
        ) -> Result<ColumnInfo> {
            let sql_type = match column.as_ref() {
                "order_id" | "quantity" => "integer",
                "unit_price" => "numeric",
                "note" => "text",
                _ => {
                    return Err(SourceError::UnknownColumn(format!(
                        "{schema}.{table}.{column}"
                    )));
                }
            };
            Ok(ColumnInfo {
                sql_type: sql_type.to_owned(),
                base_type: None,
                nullable: true,
            })
        }

        async fn has_table(&self, _: &DatabaseSchema, _: &TableName) -> Result<bool> {
            Ok(true)
        }
    }

    fn aggregate_messages(
        op: serde_json::Value,
        value_type: Option<&str>,
    ) -> Vec<(DiagnosticKind, String)> {
        let schema: IndexSchema = serde_json::from_value(json!({
            "version": 1,
            "table": "orders",
            "db_schema": "public",
            "fields": [{
                "field": "value",
                "source": { "relation": { "aggregate": {
                    "table": "order_lines",
                    "op": op,
                    "key": { "direct": "order_id" },
                    "value_type": value_type,
                } } },
            }],
        }))
        .unwrap();
        let mut indexes = BTreeMap::new();
        indexes.insert(IndexName::try_new("orders").unwrap(), schema);
        let spec = SourceSpec::new(indexes);
        futures::executor::block_on(validate_indexes(&spec, &OrderLines))
            .unwrap()
            .into_iter()
            .map(|d| (d.kind, d.message))
            .collect()
    }

    #[test]
    fn every_expression_column_must_exist_and_be_numeric() {
        let messages = aggregate_messages(
            json!({ "sum": "quantity * unit_price + note - missing" }),
            Some("double"),
        );
        assert_eq!(
            messages,
            vec![
                (
                    DiagnosticKind::NotNumeric,
                    "aggregate expression column `note` must be numeric, found `text`".to_owned()
                ),
                (
                    DiagnosticKind::UnknownColumn,
                    "references unknown column `public.order_lines.missing`".to_owned()
                ),
            ]
        );
        for op in ["avg", "min", "max"] {
            let messages = aggregate_messages(json!({ op: "quantity * unit_price" }), None);
            assert!(messages.is_empty(), "{op}: {messages:?}");
        }
    }

    #[test]
    fn a_plain_column_keeps_its_value_type_check() {
        assert_eq!(
            aggregate_messages(json!({ "sum": "note" }), Some("double")),
            vec![(
                DiagnosticKind::TypeMismatch,
                "declared aggregate type does not accept the column's database type `text`"
                    .to_owned()
            )]
        );
        assert!(aggregate_messages(json!({ "sum": "quantity" }), Some("integer")).is_empty());
    }

    /// `places(id integer not null, label mood, email email_address, lat text,
    /// lon double precision)`, where `mood` is an enum type and `email_address`
    /// a domain: a catalog reports both by their own names.
    struct Places;

    #[async_trait]
    impl Catalog for Places {
        async fn column(
            &self,
            schema: &DatabaseSchema,
            table: &TableName,
            column: &ColumnName,
        ) -> Result<ColumnInfo> {
            let sql_type = match column.as_ref() {
                "id" => "integer",
                "label" => "mood",
                "email" => "email_address",
                "lat" => "text",
                "lon" => "double precision",
                _ => {
                    return Err(SourceError::UnknownColumn(format!(
                        "{schema}.{table}.{column}"
                    )));
                }
            };
            Ok(ColumnInfo {
                sql_type: sql_type.to_owned(),
                base_type: None,
                nullable: column.as_ref() != "id",
            })
        }

        async fn has_table(&self, _: &DatabaseSchema, _: &TableName) -> Result<bool> {
            Ok(true)
        }
    }

    /// `lists(id integer, tags text[], codes integer[], doc jsonb, note text,
    /// tagged tags_domain, attrs json_domain)`, where `tags_domain` is a domain
    /// over `text[]` and `json_domain` one over `jsonb`. Transforms take a
    /// `text` element; the probe records whether each ask was about the
    /// element.
    struct Lists(std::sync::Mutex<Vec<(String, bool)>>);

    #[async_trait]
    impl Catalog for Lists {
        async fn column(
            &self,
            schema: &DatabaseSchema,
            table: &TableName,
            column: &ColumnName,
        ) -> Result<ColumnInfo> {
            let (sql_type, base_type) = match column.as_ref() {
                "id" => ("integer", None),
                "tags" => ("text[]", None),
                "codes" => ("integer[]", None),
                "doc" => ("jsonb", None),
                "note" => ("text", None),
                "tagged" => ("tags_domain", Some("text[]")),
                "attrs" => ("json_domain", Some("jsonb")),
                _ => {
                    return Err(SourceError::UnknownColumn(format!(
                        "{schema}.{table}.{column}"
                    )));
                }
            };
            Ok(ColumnInfo {
                sql_type: sql_type.to_owned(),
                base_type: base_type.map(str::to_owned),
                nullable: column.as_ref() != "id",
            })
        }

        async fn has_table(&self, _: &DatabaseSchema, _: &TableName) -> Result<bool> {
            Ok(true)
        }

        async fn accepts_transforms(
            &self,
            _: &DatabaseSchema,
            _: &TableName,
            column: &ColumnName,
            _: &[Transform],
            element: bool,
        ) -> Result<bool> {
            self.0.lock().unwrap().push((column.to_string(), element));
            Ok(matches!(
                column.as_ref(),
                "tags" | "doc" | "note" | "tagged" | "attrs"
            ))
        }
    }

    fn lists_findings(
        fields: serde_json::Value,
    ) -> (Vec<(String, DiagnosticKind)>, Vec<(String, bool)>) {
        let schema: IndexSchema = serde_json::from_value(json!({
            "version": 1, "table": "lists", "db_schema": "public", "primary_key": "id",
            "fields": fields,
        }))
        .unwrap();
        let spec = SourceSpec::new(BTreeMap::from([(
            IndexName::try_new("lists").unwrap(),
            schema,
        )]));
        let catalog = Lists(Default::default());
        let findings = futures::executor::block_on(validate_indexes(&spec, &catalog))
            .unwrap()
            .into_iter()
            .map(|d| (d.field.to_string(), d.kind))
            .collect();
        (findings, catalog.0.into_inner().unwrap())
    }

    fn list_column(column: &str, ty: &str, array: bool) -> serde_json::Value {
        json!({ "field": column, "source": { "column": {
            "column": column, "ty": ty, "nullable": true, "array": array,
            "transforms": ["lowercase"],
        } } })
    }

    #[test]
    fn an_array_flag_must_match_a_column_that_holds_an_array() {
        let (findings, _) = lists_findings(json!([
            { "field": "id", "source": { "column": { "column": "id", "ty": "integer", "nullable": false } } },
            list_column("tags", "keyword", true),
            list_column("doc", "keyword", true),
            list_column("note", "keyword", true),
            { "field": "codes", "source": { "column": { "column": "codes", "ty": "integer", "nullable": true } } },
        ]));
        assert_eq!(
            findings,
            vec![
                ("note".to_owned(), DiagnosticKind::NotAnArrayColumn),
                ("codes".to_owned(), DiagnosticKind::UndeclaredArrayColumn),
            ]
        );
        assert_eq!(DiagnosticKind::NotAnArrayColumn.severity(), Severity::Error);
        assert_eq!(
            DiagnosticKind::UndeclaredArrayColumn.severity(),
            Severity::Error
        );
    }

    /// An index that does not hold what its schema says is an error (issue
    /// #485): a list where the schema says one value, a null where it says
    /// never, other ids than the table's. A finding that leaves the index
    /// correct, or knows nothing either way, stays a warning.
    #[test]
    fn an_index_that_differs_from_its_schema_is_an_error() {
        for kind in [
            DiagnosticKind::UndeclaredArrayColumn,
            DiagnosticKind::Nullability,
            DiagnosticKind::IdsElementLossy,
        ] {
            assert_eq!(kind.severity(), Severity::Error, "{kind:?}");
        }
        for kind in [
            DiagnosticKind::IgnoredPrimaryKey,
            DiagnosticKind::CheckSkipped,
        ] {
            assert_eq!(kind.severity(), Severity::Warning, "{kind:?}");
        }
    }

    /// A domain is judged by the type it resolves to: one over `text[]` or
    /// `jsonb` holds an array, and one over `text[]` without the flag is
    /// reported by its name and its base. The declared type is still checked
    /// against the domain's own name, as for any domain: a `TypeMismatch`,
    /// which does not stop a start, where the `json` domain's unknown
    /// elements skip that check.
    #[test]
    fn a_domain_over_an_array_or_jsonb_holds_an_array() {
        let (findings, _) = lists_findings(json!([
            { "field": "id", "source": { "column": { "column": "id", "ty": "integer", "nullable": false } } },
            list_column("tagged", "keyword", true),
            list_column("attrs", "keyword", true),
        ]));
        assert_eq!(
            findings,
            vec![("tagged".to_owned(), DiagnosticKind::TypeMismatch)]
        );

        let schema: IndexSchema = serde_json::from_value(json!({
            "version": 1, "table": "lists", "db_schema": "public", "primary_key": "id",
            "fields": [
                { "field": "id", "source": { "column": { "column": "id", "ty": "integer", "nullable": false } } },
                { "field": "tagged", "source": { "column": { "column": "tagged", "ty": "keyword", "nullable": true } } },
            ],
        }))
        .unwrap();
        let spec = SourceSpec::new(BTreeMap::from([(
            IndexName::try_new("lists").unwrap(),
            schema,
        )]));
        let found =
            futures::executor::block_on(validate_indexes(&spec, &Lists(Default::default())))
                .unwrap();
        let undeclared: Vec<_> = found
            .iter()
            .filter(|d| d.kind == DiagnosticKind::UndeclaredArrayColumn)
            .collect();
        let [finding] = undeclared.as_slice() else {
            panic!("one undeclared array: {found:?}");
        };

        assert!(
            finding.message.contains(
                "`tags_domain` (a domain over `text[]`) is an array, but the field does not \
                 declare `array: true`: the schema says one value while the index holds a list"
            ),
            "{}",
            finding.message
        );
        assert!(
            finding
                .message
                .contains("declare `array: true` (that changes"),
            "{}",
            finding.message
        );
        assert!(
            finding.message.contains("content hash"),
            "{}",
            finding.message
        );
    }

    #[test]
    fn an_array_column_asks_about_its_elements_transforms() {
        let (findings, asked) = lists_findings(json!([
            { "field": "id", "source": { "column": { "column": "id", "ty": "integer", "nullable": false } } },
            list_column("tags", "keyword", true),
            list_column("codes", "integer", true),
            list_column("doc", "keyword", true),
        ]));
        assert_eq!(
            findings,
            vec![("codes".to_owned(), DiagnosticKind::TransformNotApplicable)]
        );
        assert_eq!(
            asked,
            vec![
                ("tags".to_owned(), true),
                ("codes".to_owned(), true),
                ("doc".to_owned(), true),
            ]
        );
    }

    /// The type, numeric and nullability checks each carry their own kind, so a
    /// caller can tell them from a name that does not exist.
    #[test]
    fn type_numeric_and_nullability_findings_carry_their_kind() {
        let schema: IndexSchema = serde_json::from_value(json!({
            "version": 1, "table": "places", "db_schema": "public", "primary_key": "id",
            "fields": [
                { "field": "id", "source": { "column": { "column": "id", "ty": "integer", "nullable": false } } },
                { "field": "label", "source": { "column": { "column": "label", "ty": "enum", "nullable": false } } },
                { "field": "email", "source": { "column": { "column": "email", "ty": "keyword", "nullable": true } } },
                { "field": "at", "source": { "geo": { "lat": "lat", "lon": "lon", "nullable": true } } },
            ],
        }))
        .unwrap();
        let spec = SourceSpec::new(BTreeMap::from([(
            IndexName::try_new("places").unwrap(),
            schema,
        )]));
        let diagnostics = futures::executor::block_on(validate_indexes(&spec, &Places)).unwrap();
        let nullability = diagnostics
            .iter()
            .find(|d| d.kind == DiagnosticKind::Nullability)
            .unwrap();
        assert!(
            nullability
                .message
                .contains("a document can hold null where the schema says it never does"),
            "{}",
            nullability.message
        );
        assert!(
            nullability.message.contains("declare `required: false`"),
            "{}",
            nullability.message
        );
        let found: Vec<_> = diagnostics
            .into_iter()
            .map(|d| (d.dotted_path(), d.kind, d.severity))
            .collect();
        let path = |p: &str| p.to_owned();
        assert_eq!(
            found,
            vec![
                (path("label"), DiagnosticKind::TypeMismatch, Severity::Error),
                (path("label"), DiagnosticKind::Nullability, Severity::Error),
                (path("email"), DiagnosticKind::TypeMismatch, Severity::Error),
                (path("at"), DiagnosticKind::NotNumeric, Severity::Error),
            ]
        );
    }

    /// `Places` read as a view: its columns record no nullability. Counts
    /// how often it is asked.
    #[derive(Default)]
    struct PlacesView {
        asked: std::sync::atomic::AtomicUsize,
    }

    #[async_trait]
    impl Catalog for PlacesView {
        async fn column(
            &self,
            schema: &DatabaseSchema,
            table: &TableName,
            column: &ColumnName,
        ) -> Result<ColumnInfo> {
            Places.column(schema, table, column).await
        }

        async fn has_table(&self, _: &DatabaseSchema, _: &TableName) -> Result<bool> {
            Ok(true)
        }

        async fn tracks_nullability(&self, _: &DatabaseSchema, _: &TableName) -> Result<bool> {
            self.asked
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(false)
        }
    }

    /// Whether a table records nullability is asked once per table, however
    /// many of its columns are declared `required`.
    #[test]
    fn nullability_tracking_is_asked_once_per_table() {
        let required = |name: &str| {
            json!({ "field": name, "source": { "column": {
                "column": "label", "ty": "keyword", "nullable": false,
            } } })
        };
        let schema: IndexSchema = serde_json::from_value(json!({
            "version": 1, "table": "places", "db_schema": "public", "primary_key": "id",
            "fields": [int_field("id"), required("a"), required("b"), required("c")],
        }))
        .unwrap();
        let spec = SourceSpec::new(BTreeMap::from([(
            IndexName::try_new("places").unwrap(),
            schema,
        )]));
        let view = PlacesView::default();
        let found = futures::executor::block_on(validate_indexes(&spec, &view)).unwrap();
        assert_eq!(
            found
                .iter()
                .filter(|d| d.kind == DiagnosticKind::NullabilityUntracked)
                .count(),
            3,
            "{found:?}"
        );
        assert_eq!(view.asked.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    /// `required` over a view's column is only a warning (issue #485): the
    /// store reports it nullable whatever it holds, so nothing is known. On a
    /// table it stays the error.
    #[test]
    fn required_over_a_views_column_is_a_warning_that_says_why() {
        let schema: IndexSchema = serde_json::from_value(json!({
            "version": 1, "table": "places", "db_schema": "public", "primary_key": "id",
            "fields": [
                { "field": "id", "source": { "column": { "column": "id", "ty": "integer", "nullable": false } } },
                { "field": "label", "source": { "column": { "column": "label", "ty": "keyword", "nullable": false } } },
            ],
        }))
        .unwrap();
        let spec = SourceSpec::new(BTreeMap::from([(
            IndexName::try_new("places").unwrap(),
            schema,
        )]));
        let kinds = |catalog: &dyn Catalog| {
            futures::executor::block_on(validate_indexes(&spec, catalog))
                .unwrap()
                .into_iter()
                .filter(|d| d.dotted_path() == "label")
                .filter(|d| d.kind != DiagnosticKind::TypeMismatch)
                .collect::<Vec<_>>()
        };
        let on_a_table = kinds(&Places);
        assert_eq!(
            on_a_table.iter().map(|d| d.kind).collect::<Vec<_>>(),
            vec![DiagnosticKind::Nullability]
        );
        let on_a_view = kinds(&PlacesView::default());
        let [finding] = on_a_view.as_slice() else {
            panic!("one finding: {on_a_view:?}");
        };
        assert_eq!(finding.kind, DiagnosticKind::NullabilityUntracked);
        assert_eq!(finding.severity, Severity::Warning);
        assert!(
            finding.message.contains("flusso cannot check it")
                && finding.message.contains("must really never hold null"),
            "{}",
            finding.message
        );
    }

    /// `users(id, name, team_id)`, `teams(id, name)`, `orders(id, user_id,
    /// status, region)`, `items(id, order_id, qty, region)`, `tags(id, label)`,
    /// `user_tags(user_id, tag_id, kind)`, `notes(user_id, body)`, `pairs(a, b,
    /// user_id)`; every column an `integer`. Each table's key is `id`, except the
    /// junction `user_tags` and `notes` (none) and `pairs` (`a`, `b`). Only a
    /// `name` column takes `transforms`.
    struct Shop;

    const SHOP: &[(&str, &[&str])] = &[
        ("users", &["id", "name", "team_id", "label"]),
        ("teams", &["id", "name"]),
        (
            "orders",
            &[
                "id",
                "user_id",
                "status",
                "region",
                "user_ref",
                "user_label",
            ],
        ),
        ("items", &["id", "order_id", "qty", "region"]),
        ("tags", &["id", "label"]),
        ("user_tags", &["user_id", "tag_id", "kind"]),
        ("notes", &["user_id", "body"]),
        ("pairs", &["a", "b", "user_id"]),
    ];

    #[async_trait]
    impl Catalog for Shop {
        async fn column(
            &self,
            schema: &DatabaseSchema,
            table: &TableName,
            column: &ColumnName,
        ) -> Result<ColumnInfo> {
            let Some((_, columns)) = SHOP.iter().find(|(name, _)| *name == table.as_ref()) else {
                return Err(SourceError::UnknownTable(format!("{schema}.{table}")));
            };
            if !columns.contains(&column.as_ref()) {
                return Err(SourceError::UnknownColumn(format!(
                    "{schema}.{table}.{column}"
                )));
            }
            Ok(ColumnInfo {
                sql_type: "integer".to_owned(),
                base_type: None,
                nullable: true,
            })
        }

        async fn has_table(&self, _: &DatabaseSchema, table: &TableName) -> Result<bool> {
            Ok(SHOP.iter().any(|(name, _)| *name == table.as_ref()))
        }

        async fn primary_key(
            &self,
            _: &DatabaseSchema,
            table: &TableName,
        ) -> Result<Option<Vec<ColumnName>>> {
            let key: &[&str] = match table.as_ref() {
                "user_tags" | "notes" => &[],
                "pairs" => &["a", "b"],
                _ => &["id"],
            };
            Ok(Some(
                key.iter()
                    .map(|c| ColumnName::try_new(*c).unwrap())
                    .collect(),
            ))
        }

        async fn accepts_transforms(
            &self,
            _: &DatabaseSchema,
            _: &TableName,
            column: &ColumnName,
            _: &[Transform],
            _: bool,
        ) -> Result<bool> {
            if column.as_ref() == "label" {
                return Err(SourceError::Unchecked(HIDDEN.to_owned()));
            }
            Ok(column.as_ref() == "name")
        }

        async fn compares(
            &self,
            _: &DatabaseSchema,
            (_, link): (&TableName, &ColumnName),
            _: (&TableName, &ColumnName),
        ) -> Result<bool> {
            if link.as_ref() == "user_label" {
                return Err(SourceError::Unchecked(HIDDEN.to_owned()));
            }
            Ok(link.as_ref() != "user_ref")
        }
    }

    /// Why [`Shop`] refuses to check a `label` column's transforms and a
    /// `user_label` link.
    const HIDDEN: &str = "permission denied for schema hidden";

    fn value_filter(column: &str) -> serde_json::Value {
        json!({ "value_op": { "column": column, "op": "eq", "value": { "single": "1" } } })
    }

    fn null_filter(column: &str) -> serde_json::Value {
        json!({ "null_check": { "column": column, "op": "is_null" } })
    }

    fn int_field(name: &str) -> serde_json::Value {
        json!({
            "field": name,
            "source": { "column": { "column": name, "ty": "integer", "nullable": true } },
        })
    }

    /// `(path, kind, message)` for every diagnostic of a `users` index with `fields`.
    fn shop_problems(fields: serde_json::Value) -> Vec<Found> {
        root_problems(json!({ "fields": fields }))
    }

    /// A `users` index: `id` and `name`, plus the index-level `keys`.
    fn users_spec(keys: serde_json::Value) -> SourceSpec {
        let mut schema = json!({
            "version": 1,
            "table": "users",
            "db_schema": "public",
            "primary_key": "id",
            "fields": [int_field("id"), int_field("name")],
        });
        for (key, value) in keys.as_object().unwrap() {
            schema[key] = value.clone();
        }
        let schema: IndexSchema = serde_json::from_value(schema).unwrap();
        SourceSpec::new(BTreeMap::from([(
            IndexName::try_new("users").unwrap(),
            schema,
        )]))
    }

    /// `(path, kind, message)` of one diagnostic.
    type Found = (String, DiagnosticKind, String);

    fn located(diagnostics: Vec<Diagnostic>) -> Vec<Found> {
        diagnostics
            .into_iter()
            .map(|d| {
                assert_eq!(d.severity, d.kind.severity());
                assert_eq!(d.path.last(), Some(&d.field));
                let at = match d.anchor {
                    Anchor::Field => d.dotted_path(),
                    Anchor::IndexKey => format!("@{}", d.dotted_path()),
                };
                (at, d.kind, d.message)
            })
            .collect()
    }

    /// `(path, kind, message)` for every diagnostic of a `users` index with `keys`.
    fn root_problems(keys: serde_json::Value) -> Vec<Found> {
        located(futures::executor::block_on(validate_indexes(&users_spec(keys), &Shop)).unwrap())
    }

    fn problems(expected: &[(&str, DiagnosticKind, &str)]) -> Vec<Found> {
        expected
            .iter()
            .map(|(path, kind, message)| ((*path).to_owned(), *kind, (*message).to_owned()))
            .collect()
    }

    #[test]
    fn a_schema_that_names_only_what_exists_is_clean() {
        let problems = shop_problems(json!([
            { "field": "team", "source": { "relation": { "join": {
                "table": "teams", "kind": { "belongs_to": { "column": "team_id" } },
                "primary_key": "id", "nullable": true,
                "fields": [int_field("name")],
            } } } },
            { "field": "orders", "source": { "relation": { "join": {
                "table": "orders", "kind": { "has_many": { "foreign_key": "user_id" } },
                "primary_key": "id",
                "filters": [value_filter("status"), null_filter("region"), { "raw": { "raw": "nope > 1" } }],
                "order_by": [{ "column": "id" }],
                "on": [{ "local": "name", "related": "region" }],
                "fields": [{ "field": "items", "source": { "relation": { "join": {
                    "table": "items", "kind": { "has_many": { "foreign_key": "order_id" } },
                    "primary_key": "id", "fields": [int_field("qty")],
                } } } }],
            } } } },
            { "field": "tags", "source": { "relation": { "join": {
                "table": "tags", "primary_key": "id",
                "kind": { "many_to_many": { "through": {
                    "table": "user_tags", "left_key": "user_id", "right_key": "tag_id",
                    "filters": [value_filter("kind")],
                } } },
                "fields": [int_field("label")],
            } } } },
            { "field": "itemCount", "source": { "relation": { "aggregate": {
                "table": "items", "op": "count", "key": { "direct": "order_id" },
                "via": [{ "table": "orders", "foreign_key": "user_id",
                          "filters": [value_filter("status")],
                          "on": [{ "local": "name", "related": "region" }] }],
                "on": [{ "local": "region", "related": "region" }],
                "filters": [null_filter("qty")],
            } } } },
        ]));
        assert_eq!(problems, Vec::new());
    }

    #[test]
    fn a_join_reports_every_name_it_misses() {
        let found = shop_problems(json!([
            { "field": "team", "source": { "relation": { "join": {
                "table": "teams", "kind": { "belongs_to": { "column": "teamid" } },
                "primary_key": "uid", "nullable": true,
                "filters": [value_filter("stat"), null_filter("nam")],
                "order_by": [{ "column": "rank" }],
                "on": [{ "local": "nmae", "related": "nmae" }],
                "fields": [int_field("name")],
            } } } },
            { "field": "orders", "source": { "relation": { "join": {
                "table": "orders", "kind": { "has_many": { "foreign_key": "userid" } },
                "primary_key": "id",
                "fields": [{ "field": "items", "source": { "relation": { "join": {
                    "table": "item", "kind": { "has_many": { "foreign_key": "order_id" } },
                    "primary_key": "id", "fields": [int_field("qty")],
                } } } }],
            } } } },
        ]));
        assert_eq!(
            found,
            problems(&[
                (
                    "team",
                    DiagnosticKind::UnknownColumn,
                    "`column` references unknown column `public.users.teamid`"
                ),
                (
                    "team",
                    DiagnosticKind::UnknownColumn,
                    "`on.local` references unknown column `public.users.nmae`"
                ),
                (
                    "team",
                    DiagnosticKind::UnknownColumn,
                    "`on.related` references unknown column `public.teams.nmae`"
                ),
                (
                    "team",
                    DiagnosticKind::IgnoredPrimaryKey,
                    "`primary_key` references unknown column `public.teams.uid`; ignored: the join is keyed by the table's own primary key"
                ),
                (
                    "team",
                    DiagnosticKind::UnknownColumn,
                    "`filters` references unknown column `public.teams.stat`"
                ),
                (
                    "team",
                    DiagnosticKind::UnknownColumn,
                    "`filters` references unknown column `public.teams.nam`"
                ),
                (
                    "team",
                    DiagnosticKind::UnknownColumn,
                    "`order_by` references unknown column `public.teams.rank`"
                ),
                (
                    "orders",
                    DiagnosticKind::UnknownColumn,
                    "`foreign_key` references unknown column `public.orders.userid`"
                ),
                (
                    "orders.items",
                    DiagnosticKind::UnknownTable,
                    "`table` references unknown table `public.item`"
                ),
            ]),
        );
    }

    #[test]
    fn a_junction_reports_its_table_keys_and_filters() {
        let join = |through: serde_json::Value| {
            json!([{ "field": "tags", "source": { "relation": { "join": {
                "table": "tags", "primary_key": "id",
                "kind": { "many_to_many": { "through": through } },
                "fields": [int_field("label")],
            } } } }])
        };
        assert_eq!(
            shop_problems(join(json!({
                "table": "user_tag", "left_key": "user_id", "right_key": "tag_id",
            }))),
            problems(&[(
                "tags",
                DiagnosticKind::UnknownTable,
                "`through.table` references unknown table `public.user_tag`"
            )]),
        );
        assert_eq!(
            shop_problems(join(json!({
                "table": "user_tags", "left_key": "user", "right_key": "tag",
                "filters": [value_filter("knd")],
            }))),
            problems(&[
                (
                    "tags",
                    DiagnosticKind::UnknownColumn,
                    "`through.left_key` references unknown column `public.user_tags.user`"
                ),
                (
                    "tags",
                    DiagnosticKind::UnknownColumn,
                    "`through.right_key` references unknown column `public.user_tags.tag`"
                ),
                (
                    "tags",
                    DiagnosticKind::UnknownColumn,
                    "`through.filters` references unknown column `public.user_tags.knd`"
                ),
            ]),
        );
    }

    #[test]
    fn an_aggregate_reports_its_hops_key_pairs_and_filters() {
        let found = shop_problems(json!([
            { "field": "itemCount", "source": { "relation": { "aggregate": {
                "table": "items", "op": "count", "key": { "direct": "orderid" },
                "via": [{ "table": "orders", "foreign_key": "userid",
                          "filters": [value_filter("state")],
                          "on": [{ "local": "nme", "related": "regio" }] }],
                "on": [{ "local": "regn", "related": "rgn" }],
                "filters": [null_filter("qt")],
            } } } },
            { "field": "lostHop", "source": { "relation": { "aggregate": {
                "table": "items", "op": "count", "key": { "direct": "order_id" },
                "via": [{ "table": "order", "foreign_key": "user_id",
                          "on": [{ "local": "name", "related": "region" }] }],
                "on": [{ "local": "region", "related": "region" }],
            } } } },
            { "field": "lostTable", "source": { "relation": { "aggregate": {
                "table": "item", "op": { "sum": "qty" }, "value_type": "long",
                "key": { "through": { "table": "user_tags", "left_key": "user_id", "right_key": "tagid" } },
            } } } },
        ]));
        assert_eq!(
            found,
            problems(&[
                (
                    "itemCount",
                    DiagnosticKind::UnknownColumn,
                    "`via.foreign_key` references unknown column `public.orders.userid`"
                ),
                (
                    "itemCount",
                    DiagnosticKind::UnknownColumn,
                    "`via.on.local` references unknown column `public.users.nme`"
                ),
                (
                    "itemCount",
                    DiagnosticKind::UnknownColumn,
                    "`via.on.related` references unknown column `public.orders.regio`"
                ),
                (
                    "itemCount",
                    DiagnosticKind::UnknownColumn,
                    "`via.filters` references unknown column `public.orders.state`"
                ),
                (
                    "itemCount",
                    DiagnosticKind::UnknownColumn,
                    "`foreign_key` references unknown column `public.items.orderid`"
                ),
                (
                    "itemCount",
                    DiagnosticKind::UnknownColumn,
                    "`on.local` references unknown column `public.orders.regn`"
                ),
                (
                    "itemCount",
                    DiagnosticKind::UnknownColumn,
                    "`on.related` references unknown column `public.items.rgn`"
                ),
                (
                    "itemCount",
                    DiagnosticKind::UnknownColumn,
                    "`filters` references unknown column `public.items.qt`"
                ),
                (
                    "lostHop",
                    DiagnosticKind::UnknownTable,
                    "`via.table` references unknown table `public.order`"
                ),
                (
                    "lostTable",
                    DiagnosticKind::UnknownTable,
                    "`table` references unknown table `public.item`"
                ),
                (
                    "lostTable",
                    DiagnosticKind::UnknownColumn,
                    "`through.right_key` references unknown column `public.user_tags.tagid`"
                ),
            ]),
        );
    }

    #[test]
    fn root_filters_are_checked_on_the_root_table() {
        assert_eq!(
            root_problems(json!({
                "filters": [value_filter("nme"), null_filter("team"), { "raw": { "raw": "nope" } }],
            })),
            problems(&[
                (
                    "@filters",
                    DiagnosticKind::UnknownColumn,
                    "`filters` references unknown column `public.users.nme`"
                ),
                (
                    "@filters",
                    DiagnosticKind::UnknownColumn,
                    "`filters` references unknown column `public.users.team`"
                ),
            ]),
        );
        assert_eq!(
            root_problems(json!({ "filters": [value_filter("name"), null_filter("team_id")] })),
            Vec::new()
        );
    }

    #[test]
    fn a_soft_delete_column_and_its_when_are_checked_on_the_root_table() {
        assert_eq!(
            root_problems(json!({ "soft_delete": { "column": {
                "column": "deleted_at",
                "when": [value_filter("stat"), null_filter("team")],
            } } })),
            problems(&[
                (
                    "@soft_delete",
                    DiagnosticKind::UnknownColumn,
                    "`column` references unknown column `public.users.deleted_at`"
                ),
                (
                    "@soft_delete",
                    DiagnosticKind::UnknownColumn,
                    "`when` references unknown column `public.users.stat`"
                ),
                (
                    "@soft_delete",
                    DiagnosticKind::UnknownColumn,
                    "`when` references unknown column `public.users.team`"
                ),
            ]),
        );
        assert_eq!(
            root_problems(json!({ "soft_delete": { "column": {
                "column": "team_id", "when": [null_filter("name")],
            } } })),
            Vec::new()
        );
    }

    #[test]
    fn a_soft_delete_field_must_name_a_column_field() {
        assert_eq!(
            root_problems(json!({ "soft_delete": { "field": {
                "field": "deleted", "when": [value_filter("nme")],
            } } })),
            problems(&[
                (
                    "@soft_delete",
                    DiagnosticKind::UnreadSoftDeleteMarker,
                    "`field` names `deleted`, but no field has that name"
                ),
                (
                    "@soft_delete",
                    DiagnosticKind::UnknownColumn,
                    "`when` references unknown column `public.users.nme`"
                ),
            ]),
        );
        assert_eq!(
            root_problems(json!({ "soft_delete": { "field": { "field": "name" } } })),
            Vec::new()
        );
    }

    /// The search is depth-first: a join named like the marker, declared before
    /// a column field of that name, is the match, and it is not a column.
    #[test]
    fn a_soft_delete_field_whose_first_match_is_a_join_is_an_error() {
        let team = json!({ "field": "team", "source": { "relation": { "join": {
            "table": "teams", "kind": { "belongs_to": { "column": "team_id" } },
            "primary_key": "id", "nullable": true, "fields": [int_field("name")],
        } } } });
        assert_eq!(
            root_problems(json!({
                "fields": [team, int_field("team")],
                "soft_delete": { "field": { "field": "team" } },
            })),
            problems(&[
                (
                    "team#2",
                    DiagnosticKind::UnknownColumn,
                    "references unknown column `public.users.team`"
                ),
                (
                    "@soft_delete",
                    DiagnosticKind::UnreadSoftDeleteMarker,
                    "`field` names `team`, but the first field of that name is not a column"
                ),
                (
                    "team",
                    DiagnosticKind::DuplicateFieldNameWrong,
                    "`team` is declared 2 times at this level: #1 (join on `teams`), #2 (column \
                     `team`); the index maps the last one, and a document holds the last one's \
                     value as raw JSON, not converted to the declared types, so its values are \
                     not what the schema declares. Rename all but one"
                ),
            ]),
        );
    }

    /// A field found inside a relation is read on the root row, so its column
    /// must exist there.
    #[test]
    fn a_soft_delete_field_inside_a_relation_is_checked_on_the_root_table() {
        let orders = |field: &str| {
            json!({ "field": "orders", "source": { "relation": { "join": {
                "table": "orders", "kind": { "has_many": { "foreign_key": "user_id" } },
                "primary_key": "id", "fields": [int_field(field)],
            } } } })
        };
        assert_eq!(
            root_problems(json!({
                "fields": [orders("status")],
                "soft_delete": { "field": { "field": "status" } },
            })),
            problems(&[(
                "@soft_delete",
                DiagnosticKind::UnknownColumn,
                "`field` references unknown column `public.users.status`"
            )]),
        );
        assert_eq!(
            root_problems(json!({
                "fields": [orders("name"), int_field("id")],
                "soft_delete": { "field": { "field": "name" } },
            })),
            problems(&[(
                "orders.name",
                DiagnosticKind::UnknownColumn,
                "references unknown column `public.orders.name`"
            )]),
        );
    }

    #[test]
    fn validate_spec_reports_a_dangling_soft_delete_field_with_no_database() {
        assert_eq!(
            located(validate_spec(&users_spec(json!({
                "soft_delete": { "field": { "field": "deleted" } },
            })))),
            problems(&[(
                "@soft_delete",
                DiagnosticKind::UnreadSoftDeleteMarker,
                "`field` names `deleted`, but no field has that name"
            )]),
        );
        assert!(
            validate_spec(&users_spec(json!({
                "filters": [value_filter("nme")],
                "soft_delete": { "column": { "column": "deleted_at" } },
            })))
            .is_empty()
        );
    }

    fn timestamp_field(name: &str, format: Option<&str>) -> serde_json::Value {
        let mut field = json!({
            "field": name,
            "source": { "column": { "column": name, "ty": "timestamp", "nullable": true } },
        });
        if let Some(format) = format {
            field["options"] = json!({ "format": { "String": format } });
        }
        field
    }

    fn with_format(mut field: serde_json::Value, format: &str) -> serde_json::Value {
        field["options"] = json!({ "format": { "String": format } });
        field
    }

    fn offline_findings(fields: serde_json::Value) -> Vec<Found> {
        located(validate_spec(&users_spec(json!({ "fields": fields }))))
    }

    const SPACE_ONLY: &str = "yyyy-MM-dd HH:mm:ss.SSSSSS||yyyy-MM-dd HH:mm:ss";

    /// A date `format` that can't read what flusso writes is no finding: the
    /// mapping it resolves to gets `strict_date_optional_time` appended
    /// (`kernel::iso_date_format`), so its documents index.
    #[test]
    fn a_date_format_that_cannot_read_the_iso_form_is_no_finding() {
        let script = with_format(
            json!({ "field": "computed", "source": { "script": {
                "source": "\"s\"", "ty": "timestamp", "nullable": true,
            } } }),
            "epoch_millis",
        );
        let latest = with_format(
            json!({ "field": "latest", "source": { "relation": { "aggregate": {
                "table": "orders", "op": { "max": "placed_at" },
                "key": { "direct": "user_id" }, "value_type": "timestamp",
            } } } }),
            "yyyy-MM-dd HH:mm:ss",
        );
        assert_eq!(
            offline_findings(json!([
                int_field("id"),
                timestamp_field("seen", Some(SPACE_ONLY)),
                script,
                latest,
                orders_join(
                    "orders",
                    json!([timestamp_field("placed", Some(SPACE_ONLY))])
                ),
            ])),
            problems(&[]),
        );
    }

    /// `clocks(id integer, stamped timestamp, opens time, closes timetz, day
    /// date)`, `events(id integer, clock_id integer, at timestamp, opens_at
    /// time)` and `stamps(id timestamp, clock_id integer)`, each keyed by `id`.
    struct Clocks;

    #[async_trait]
    impl Catalog for Clocks {
        async fn column(
            &self,
            schema: &DatabaseSchema,
            table: &TableName,
            column: &ColumnName,
        ) -> Result<ColumnInfo> {
            let sql_type = match (table.as_ref(), column.as_ref()) {
                ("stamps", "id") | (_, "stamped" | "at") => "timestamp without time zone",
                (_, "id" | "clock_id") => "integer",
                (_, "opens" | "opens_at") => "time without time zone",
                (_, "closes") => "time with time zone",
                (_, "day") => "date",
                _ => {
                    return Err(SourceError::UnknownColumn(format!(
                        "{schema}.{table}.{column}"
                    )));
                }
            };
            Ok(ColumnInfo {
                sql_type: sql_type.to_owned(),
                base_type: None,
                nullable: true,
            })
        }

        async fn has_table(&self, _: &DatabaseSchema, _: &TableName) -> Result<bool> {
            Ok(true)
        }

        async fn primary_key(
            &self,
            _: &DatabaseSchema,
            _: &TableName,
        ) -> Result<Option<Vec<ColumnName>>> {
            Ok(Some(vec![ColumnName::try_new("id").unwrap()]))
        }
    }

    fn clocks_findings(fields: serde_json::Value) -> Vec<Found> {
        let schema: IndexSchema = serde_json::from_value(json!({
            "version": 1, "table": "clocks", "db_schema": "public", "primary_key": "id",
            "fields": fields,
        }))
        .unwrap();
        let spec = SourceSpec::new(BTreeMap::from([(
            IndexName::try_new("clocks").unwrap(),
            schema,
        )]));
        located(futures::executor::block_on(validate_indexes(&spec, &Clocks)).unwrap())
    }

    fn events_aggregate(field: &str, op: serde_json::Value, ty: &str) -> serde_json::Value {
        json!({ "field": field, "source": { "relation": { "aggregate": {
            "table": if op.get("ids").is_some() { "stamps" } else { "events" },
            "op": op, "key": { "direct": "clock_id" }, "value_type": ty,
        } } } })
    }

    /// A date mapping whose `format` reads a time of day only refuses every
    /// timestamp flusso writes: reported over a timestamp column, a `max` of
    /// one, or `ids` of timestamp keys. Over a `time`/`timetz` column (the
    /// documented way to index a time of day), input-only or under an
    /// unindexed join it is no finding.
    #[test]
    fn a_time_of_day_format_over_a_timestamp_column_is_reported() {
        let mut hidden = with_format(timestamp_field("stamped", None), "HH:mm:ss");
        hidden["emit"] = json!(false);
        hidden["field"] = json!("hidden");
        let mut unindexed = json!({ "field": "events", "source": { "relation": { "join": {
            "table": "events", "kind": { "has_many": { "foreign_key": "clock_id" } },
            "primary_key": "id", "indexed": false,
            "fields": [with_format(timestamp_field("at", None), "HH:mm:ss")],
        } } } });
        unindexed["options"] = json!({});
        let message = |format: &str, sql_type: &str| {
            format!(
                "the `format` `{format}` reads a time of day only, but flusso writes a timestamp \
                 (`2024-01-02T03:04:05`) here, from the column's `{sql_type}`, so OpenSearch \
                 refuses every document whose value is not null. Drop the `format`, or use one \
                 that reads it: `strict_date_optional_time`, or yours followed by \
                 `||strict_date_optional_time`"
            )
        };
        assert_eq!(
            clocks_findings(json!([
                int_field("id"),
                with_format(timestamp_field("stamped", None), "HH:mm:ss"),
                with_format(timestamp_field("opens", None), "HH:mm:ss"),
                with_format(timestamp_field("closes", None), "hh:mm a"),
                with_format(
                    events_aggregate("latest", json!({ "max": "at" }), "timestamp"),
                    "hour_minute_second"
                ),
                with_format(
                    events_aggregate("earliest_open", json!({ "min": "opens_at" }), "timestamp"),
                    "hour_minute_second"
                ),
                with_format(
                    json!({ "field": "stamp_ids", "source": { "relation": { "aggregate": {
                        "table": "stamps", "op": { "ids": { "element_type": "timestamp" } },
                        "key": { "direct": "clock_id" },
                    } } } }),
                    "strict_t_time"
                ),
                hidden,
                unindexed,
            ])),
            problems(&[
                (
                    "stamped",
                    DiagnosticKind::TimeOfDayFormat,
                    &message("HH:mm:ss", "timestamp without time zone"),
                ),
                (
                    "latest",
                    DiagnosticKind::TimeOfDayFormat,
                    &message("hour_minute_second", "timestamp without time zone"),
                ),
                (
                    "stamp_ids",
                    DiagnosticKind::TimeOfDayFormat,
                    &message("strict_t_time", "timestamp without time zone"),
                ),
            ]),
        );
        assert_eq!(DiagnosticKind::TimeOfDayFormat.severity(), Severity::Error);
        assert_eq!(
            DiagnosticKind::TimeOfDayFormat.as_str(),
            "time_of_day_format"
        );
    }

    /// A `date` field over a timestamp column holds the column's value, a
    /// timestamp: a `TypeMismatch` that, under a `format` reading no
    /// timestamp, says that format refuses every document. The rule follows
    /// the declared type, so no `||strict_date_optional_time` is appended.
    #[test]
    fn a_date_field_over_a_timestamp_column_says_its_format_refuses_the_timestamp() {
        let date = |name: &str, format: Option<&str>| {
            let field = json!({ "field": name, "source": { "column": {
                "column": "stamped", "ty": "date", "nullable": true,
            } } });
            match format {
                Some(format) => with_format(field, format),
                None => field,
            }
        };
        let plain = "declared type does not accept the column's database type `timestamp without \
                     time zone`";
        let refused = |format: &str| {
            format!(
                "{plain}: flusso writes the column's value, a timestamp \
                 (`2024-01-02T03:04:05`), which the `format` `{format}` refuses, so OpenSearch \
                 refuses every document whose value is not null. Declare the field `timestamp`, \
                 or drop the `format`"
            )
        };
        assert_eq!(
            clocks_findings(json!([
                int_field("id"),
                date("bare", None),
                date("day", Some("yyyy-MM-dd")),
                date("named", Some("strict_date")),
                date("iso", Some("strict_date_optional_time")),
                with_format(
                    events_aggregate("latest", json!({ "max": "at" }), "date"),
                    "year_month_day"
                ),
            ])),
            problems(&[
                ("bare", DiagnosticKind::TypeMismatch, plain),
                ("day", DiagnosticKind::TypeMismatch, &refused("yyyy-MM-dd")),
                (
                    "named",
                    DiagnosticKind::TypeMismatch,
                    &refused("strict_date")
                ),
                ("iso", DiagnosticKind::TypeMismatch, plain),
                (
                    "latest",
                    DiagnosticKind::TypeMismatch,
                    &refused("year_month_day").replace(
                        "declared type does not accept",
                        "declared aggregate type does not accept",
                    ),
                ),
            ]),
        );
    }

    /// Where what flusso writes is known from the schema alone (a `date`
    /// field, a `custom` over date or timestamp types, a constant, a script),
    /// a time-of-day `format` is reported with no database, and once with
    /// one. A `timestamp` column may hold a time of day: only the database
    /// tells.
    #[test]
    fn a_time_of_day_format_where_flusso_writes_a_date_is_reported_with_no_database() {
        let day = with_format(
            json!({ "field": "day", "source": { "column": {
                "column": "day", "ty": "date", "nullable": true,
            } } }),
            "HH:mm",
        );
        let custom = with_format(
            json!({ "field": "zoned", "source": { "column": {
                "column": "stamped", "nullable": true,
                "ty": { "custom": { "postgres": ["timestamptz"], "opensearch": "date" } },
            } } }),
            "time",
        );
        let script = with_format(
            json!({ "field": "computed", "source": { "script": {
                "source": "this.stamped", "ty": "timestamp", "nullable": true,
            } } }),
            "'at 'HH:mm",
        );
        let either = with_format(
            json!({ "field": "either", "source": { "column": {
                "column": "day", "nullable": true,
                "ty": { "custom": { "postgres": ["date", "timestamptz"], "opensearch": "date" } },
            } } }),
            "HH:mm",
        );
        let fields = json!([
            int_field("id"),
            day,
            custom,
            either,
            script,
            with_format(timestamp_field("stamped", None), "HH:mm:ss"),
        ]);
        let reads = |format: &str, what: &str| {
            format!(
                "the `format` `{format}` reads a time of day only, but flusso writes {what} \
                 here, so OpenSearch refuses every document whose value is not null. Drop the \
                 `format`, or use one that reads it: `strict_date_optional_time`, or yours \
                 followed by `||strict_date_optional_time`"
            )
        };
        let offline = vec![
            (
                "day".to_owned(),
                DiagnosticKind::TimeOfDayFormat,
                reads("HH:mm", "a date (`2024-01-02`)"),
            ),
            (
                "zoned".to_owned(),
                DiagnosticKind::TimeOfDayFormat,
                reads("time", "a timestamp (`2024-01-02T03:04:05`)"),
            ),
            (
                "either".to_owned(),
                DiagnosticKind::TimeOfDayFormat,
                reads(
                    "HH:mm",
                    "a date or a timestamp (`2024-01-02`, `2024-01-02T03:04:05`)",
                ),
            ),
            (
                "computed".to_owned(),
                DiagnosticKind::TimeOfDayFormat,
                reads("'at 'HH:mm", "a timestamp (`2024-01-02T03:04:05`)"),
            ),
        ];
        assert_eq!(
            located(validate_spec(&SourceSpec::new(BTreeMap::from([(
                IndexName::try_new("clocks").unwrap(),
                serde_json::from_value(json!({
                    "version": 1, "table": "clocks", "db_schema": "public",
                    "primary_key": "id", "fields": fields.clone(),
                }))
                .unwrap(),
            )])))),
            offline,
        );
        let online = clocks_findings(fields);
        let kinds: Vec<(&str, DiagnosticKind)> = online
            .iter()
            .filter(|found| found.1 == DiagnosticKind::TimeOfDayFormat)
            .map(|found| (found.0.as_str(), found.1))
            .collect();
        assert_eq!(
            kinds,
            [
                ("stamped", DiagnosticKind::TimeOfDayFormat),
                ("day", DiagnosticKind::TimeOfDayFormat),
                ("zoned", DiagnosticKind::TimeOfDayFormat),
                ("either", DiagnosticKind::TimeOfDayFormat),
                ("computed", DiagnosticKind::TimeOfDayFormat),
            ],
            "{online:?}"
        );
    }

    fn column_field(name: &str, column: &str) -> serde_json::Value {
        json!({
            "field": name,
            "source": { "column": { "column": column, "ty": "keyword", "nullable": true } },
        })
    }

    fn orders_join(name: &str, fields: serde_json::Value) -> serde_json::Value {
        json!({ "field": name, "source": { "relation": { "join": {
            "table": "orders", "kind": { "has_many": { "foreign_key": "user_id" } },
            "primary_key": "id", "fields": fields,
        } } } })
    }

    fn duplicates(fields: serde_json::Value) -> Vec<Found> {
        located(validate_spec(&users_spec(json!({ "fields": fields }))))
    }

    #[test]
    fn two_fields_of_one_name_at_the_root_are_a_warning_with_no_database() {
        assert_eq!(
            duplicates(json!([
                int_field("id"),
                column_field("email", "email"),
                column_field("email", "full_name"),
            ])),
            problems(&[(
                "email",
                DiagnosticKind::DuplicateFieldName,
                "`email` is declared 2 times at this level: #2 (column `email`), #3 (column \
                 `full_name`); the index maps the last one, and a document holds the last one's \
                 value, read as the first one declares it. Rename all but one"
            )]),
        );
        assert_eq!(
            DiagnosticKind::DuplicateFieldName.severity(),
            Severity::Warning
        );
    }

    /// `FieldName` trims, so surrounding spaces name the same field; case is
    /// kept, so `Email` is another field.
    #[test]
    fn names_are_compared_as_field_name_reads_them() {
        let found = duplicates(json!([
            column_field("email", "email"),
            column_field(" email ", "full_name"),
            column_field("Email", "email"),
        ]));
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].0, "email");
    }

    #[test]
    fn a_duplicate_inside_a_join_and_two_joins_of_one_name_are_each_reported() {
        assert_eq!(
            duplicates(json!([
                orders_join("orders", json!([int_field("id"), int_field("id")])),
                orders_join("orders", json!([int_field("total")])),
            ])),
            problems(&[
                (
                    "orders",
                    DiagnosticKind::DuplicateFieldNameWrong,
                    "`orders` is declared 2 times at this level: #1 (join on `orders`), #2 \
                     (join on `orders`); the index maps the last one, but a document keeps none \
                     of its child fields: #1 and #2 declare none in common, so every element is \
                     empty, not the object the index maps. Rename all but one"
                ),
                (
                    "orders#1.id",
                    DiagnosticKind::DuplicateFieldName,
                    "`id` is declared 2 times at this level: #1 (column `id`), #2 (column `id`); \
                     the index maps the last one, and a document holds the last one's value, \
                     read as the first one declares it. Rename all but one"
                ),
            ]),
        );
    }

    /// Namesakes that can never build or index a document are errors of their
    /// own kinds, with no database (issue #316); every other pair stays the
    /// warning.
    #[test]
    fn namesakes_that_never_index_are_errors_with_no_database() {
        let kind = |fields: serde_json::Value| {
            let found = duplicates(fields);
            assert_eq!(found.len(), 1, "{found:?}");
            found[0].1
        };
        let tags = json!({ "field": "v", "source": { "column": {
            "column": "tags", "ty": "keyword", "nullable": true, "array": true,
        } } });
        let five = json!({ "field": "v", "source": { "constant": { "BigInt": 5 } } });
        let keyed =
            json!({ "field": "v", "source": { "constant": { "Map": { "a": { "BigInt": 1 } } } } });
        let object = group("v", json!([column_field("k", "k")]));
        let map = json!({ "field": "v", "source": { "column": {
            "column": "m", "ty": { "map": { "values": "keyword" } }, "nullable": true,
        } }, "options": { "dynamic": { "Bool": true } } });

        assert_eq!(
            kind(json!([int_field("id"), tags.clone(), object.clone()])),
            DiagnosticKind::DuplicateFieldNameFailsToBuild
        );
        assert_eq!(
            kind(json!([int_field("id"), five.clone(), object.clone()])),
            DiagnosticKind::DuplicateFieldNameRefused
        );
        assert_eq!(
            kind(json!([int_field("id"), five.clone(), map.clone()])),
            DiagnosticKind::DuplicateFieldNameRefused
        );
        assert_eq!(
            kind(json!([int_field("id"), keyed.clone(), object.clone()])),
            DiagnosticKind::DuplicateFieldNameRefused
        );
        assert_eq!(
            DiagnosticKind::DuplicateFieldNameFailsToBuild.severity(),
            Severity::Error
        );
        assert_eq!(
            DiagnosticKind::DuplicateFieldNameRefused.severity(),
            Severity::Error
        );

        let k = group("v", json!([column_field("a", "a")]));
        let disabled = json!({ "field": "v", "source": { "group": [column_field("k", "k")] },
            "options": { "enabled": { "Bool": false } } });
        let quiet = json!({ "field": "v", "emit": false, "source": { "column": {
            "column": "tags", "ty": "keyword", "nullable": true, "array": true,
        } } });
        for (pair, expected) in [
            (
                json!([int_field("id"), tags.clone(), column_field("v", "k")]),
                DiagnosticKind::DuplicateFieldNameWrong,
            ),
            (
                json!([int_field("id"), quiet, object.clone()]),
                DiagnosticKind::DuplicateFieldNameWrong,
            ),
            (
                json!([int_field("id"), keyed.clone(), k]),
                DiagnosticKind::DuplicateFieldName,
            ),
            (
                json!([int_field("id"), keyed, map]),
                DiagnosticKind::DuplicateFieldName,
            ),
            (
                json!([int_field("id"), five.clone(), disabled]),
                DiagnosticKind::DuplicateFieldNameWrong,
            ),
            (
                json!([int_field("id"), five, column_field("v", "k")]),
                DiagnosticKind::DuplicateFieldName,
            ),
        ] {
            assert_eq!(kind(pair.clone()), expected, "{pair}");
        }
    }

    /// A stopping pair stops only where every document carries it: at the
    /// root, or in plain objects all the way up. Under a join (a parent with
    /// no related row indexes), an input-only or unindexed ancestor (the pair
    /// is never mapped), or an object with `options`, it does not stop: it is
    /// the error where a document stores it (unindexed included) and the
    /// warning where no document holds it and no script reads it. A `dynamic`
    /// the user sets counts as an option of their own.
    #[test]
    fn a_stopping_pair_stops_only_where_every_document_carries_it() {
        let kind = |fields: serde_json::Value| {
            let found = duplicates(fields);
            assert_eq!(found.len(), 1, "{found:?}");
            found[0].1
        };
        let five = json!({ "field": "v", "source": { "constant": { "BigInt": 5 } } });
        let keyed =
            json!({ "field": "v", "source": { "constant": { "Map": { "a": { "BigInt": 1 } } } } });
        let tags = json!({ "field": "v", "source": { "column": {
            "column": "tags", "ty": "keyword", "nullable": true, "array": true,
        } } });
        let object = group("v", json!([column_field("k", "k")]));
        let pair = json!([five.clone(), object.clone()]);
        let with = |mut field: serde_json::Value, key: &str, value: serde_json::Value| {
            field[key] = value;
            field
        };
        let unindexed = {
            let mut join = orders_join("rel", pair.clone());
            join["source"]["relation"]["join"]["indexed"] = json!(false);
            join
        };

        assert_eq!(
            kind(json!([
                int_field("id"),
                group("o", json!([group("p", pair.clone())]))
            ])),
            DiagnosticKind::DuplicateFieldNameRefused
        );
        // Under an input-only ancestor no document holds the pair and no
        // script here reads it: the warning. Anywhere else a document holds,
        // or stores unindexed, what the schema doesn't declare: the error.
        let warning = DiagnosticKind::DuplicateFieldName;
        let wrong = DiagnosticKind::DuplicateFieldNameWrong;
        for (fields, expected) in [
            (json!([int_field("id"), unindexed]), wrong),
            (
                json!([
                    int_field("id"),
                    with(orders_join("rel", pair.clone()), "emit", json!(false))
                ]),
                warning,
            ),
            (
                json!([int_field("id"), orders_join("rel", pair.clone())]),
                wrong,
            ),
            (json!([int_field("id"), to_one("rel", pair.clone())]), wrong),
            (
                json!([
                    int_field("id"),
                    orders_join("rel", json!([tags, object.clone()]))
                ]),
                wrong,
            ),
            (
                json!([
                    int_field("id"),
                    group("o", json!([orders_join("rel", pair.clone())]))
                ]),
                wrong,
            ),
            (
                json!([
                    int_field("id"),
                    with(group("o", pair.clone()), "emit", json!(false))
                ]),
                warning,
            ),
            (
                json!([
                    int_field("id"),
                    with(
                        group("o", pair),
                        "options",
                        json!({ "enabled": { "Bool": false } })
                    )
                ]),
                wrong,
            ),
            (
                json!([
                    int_field("id"),
                    keyed.clone(),
                    with(
                        object.clone(),
                        "options",
                        json!({ "dynamic": { "Bool": true } })
                    )
                ]),
                wrong,
            ),
            (
                json!([
                    int_field("id"),
                    keyed,
                    with(object, "options", json!({ "dynamic": { "Bool": false } }))
                ]),
                wrong,
            ),
        ] {
            assert_eq!(kind(fields.clone()), expected, "{fields}");
        }
    }

    fn script_field(name: &str, source: &str) -> serde_json::Value {
        json!({ "field": name, "source": { "script": {
            "source": source, "ty": "keyword", "nullable": true,
        } } })
    }

    fn stated(fields: serde_json::Value) -> (DiagnosticKind, String) {
        let found = duplicates(fields);
        assert_eq!(found.len(), 1, "{found:?}");
        (found[0].1, found[0].2.clone())
    }

    fn says_nothing_is_refused(message: &str) {
        for claim in [
            "rejects",
            "refuse",
            "sink stops",
            "replication slot",
            "quarantined",
        ] {
            assert!(!message.contains(claim), "says {claim:?}: {message}");
        }
    }

    /// Under an `indexed: false` join or an object with `enabled: false`,
    /// OpenSearch indexes nothing and checks nothing: a value the last
    /// namesake would refuse is stored as it is, in `_source`, where the
    /// schema declares another shape. That is the error, and it says the
    /// value is stored, not refused.
    #[test]
    fn a_rejected_pair_under_an_unindexed_parent_is_an_error_that_says_it_is_stored() {
        let five = json!({ "field": "v", "source": { "constant": { "BigInt": 5 } } });
        let object = group("v", json!([column_field("k", "k")]));
        let pair = json!([five, object.clone()]);
        let unindexed = |fields: serde_json::Value| {
            let mut join = orders_join("rel", fields);
            join["source"]["relation"]["join"]["indexed"] = json!(false);
            join
        };
        let mut disabled = group("o", pair.clone());
        disabled["options"] = json!({ "enabled": { "Bool": false } });
        let script_then_object = json!([script_field("v", "\"s\""), object]);
        for fields in [
            json!([int_field("id"), unindexed(pair)]),
            json!([int_field("id"), disabled]),
            json!([int_field("id"), unindexed(script_then_object)]),
        ] {
            let (kind, message) = stated(fields.clone());
            assert_eq!(kind, DiagnosticKind::DuplicateFieldNameWrong, "{fields}");
            assert!(message.contains("stores"), "{message}");
            assert!(message.contains("`_source`"), "{message}");
            says_nothing_is_refused(&message);
        }
    }

    /// A last namesake that is itself unindexed (an object with `enabled:
    /// false`, an `indexed: false` join) takes any value unchecked: the first
    /// one's constant is stored where the schema declares an object or rows.
    #[test]
    fn a_constant_beside_an_unindexed_object_or_join_is_stored_as_an_error() {
        let five = json!({ "field": "v", "source": { "constant": { "BigInt": 5 } } });
        let mut disabled = group("v", json!([column_field("k", "k")]));
        disabled["options"] = json!({ "enabled": { "Bool": false } });
        let mut rows = orders_join("v", json!([column_field("k", "k")]));
        rows["source"]["relation"]["join"]["indexed"] = json!(false);
        for last in [disabled, rows] {
            let fields = json!([int_field("id"), five.clone(), last]);
            let (kind, message) = stated(fields.clone());
            assert_eq!(kind, DiagnosticKind::DuplicateFieldNameWrong, "{fields}");
            assert!(
                message.contains("a document stores the first one's constant (#2)"),
                "{message}"
            );
            says_nothing_is_refused(&message);
        }
    }

    /// Under an input-only (`emit: false`) parent no document holds the pair,
    /// but a script reads the level through `this` (a script at the parent's
    /// level or above) or `root` (a script anywhere that names it), before the
    /// parent is stripped: then it is the error, saying what the scripts read.
    /// A script inside the input-only subtree never runs, and one beside it
    /// that names no `root` can't reach it: then it is the warning, saying no
    /// document holds it.
    #[test]
    fn a_pair_under_an_input_only_parent_is_an_error_only_where_a_script_reads_it() {
        let five = json!({ "field": "v", "source": { "constant": { "BigInt": 5 } } });
        let object = group("v", json!([column_field("k", "k")]));
        let hidden = |mut fields: Vec<serde_json::Value>| {
            fields.extend([five.clone(), object.clone()]);
            let mut parent = group("o", serde_json::Value::Array(fields));
            parent["emit"] = json!(false);
            parent
        };
        let this_reader = script_field("s", "this.o.v");
        let root_reader = script_field("s", "root.o.v");
        let elsewhere = |script: serde_json::Value| orders_join("rel", json!([script]));
        let warning = DiagnosticKind::DuplicateFieldName;
        let wrong = DiagnosticKind::DuplicateFieldNameWrong;
        for (fields, expected) in [
            (json!([int_field("id"), hidden(vec![])]), warning),
            (
                json!([int_field("id"), this_reader.clone(), hidden(vec![])]),
                wrong,
            ),
            (
                json!([
                    int_field("id"),
                    elsewhere(root_reader.clone()),
                    hidden(vec![])
                ]),
                wrong,
            ),
            (
                json!([
                    int_field("id"),
                    elsewhere(this_reader.clone()),
                    hidden(vec![])
                ]),
                warning,
            ),
            (json!([int_field("id"), hidden(vec![root_reader])]), warning),
            (
                json!([
                    int_field("id"),
                    group("p", json!([this_reader.clone(), hidden(vec![])]))
                ]),
                wrong,
            ),
        ] {
            let (kind, message) = stated(fields.clone());
            assert_eq!(kind, expected, "{fields}");
            assert!(message.contains("no document holds"), "{message}");
            if expected == wrong {
                assert!(message.contains("a script can read"), "{message}");
            } else {
                assert!(message.contains("no script reads"), "{message}");
            }
            says_nothing_is_refused(&message);
        }
        let mut quiet = hidden(vec![]);
        quiet["source"]["group"] = json!([
            { "field": "v", "source": { "column": {
                "column": "tags", "ty": "keyword", "nullable": true, "array": true,
            } } },
            object,
        ]);
        let (kind, message) = stated(json!([int_field("id"), quiet]));
        assert_eq!(
            kind, wrong,
            "a document that fails to build fails under any parent"
        );
        assert!(message.contains("fails to build"), "{message}");
    }

    /// A script namesake under an input-only parent never runs, and a script
    /// never reads another's output: what a reader can get there is the value
    /// the build put in place (review 3 of #486). Still the warning.
    #[test]
    fn a_script_namesake_under_an_input_only_parent_is_not_said_to_run() {
        let mut parent = group(
            "o",
            json!([column_field("v", "k"), script_field("v", "\"s\"")]),
        );
        parent["emit"] = json!(false);
        let (kind, message) = stated(json!([
            int_field("id"),
            script_field("s", "root.o.v"),
            parent
        ]));
        assert_eq!(kind, DiagnosticKind::DuplicateFieldName, "{message}");
        assert!(!message.contains("script's output (#"), "{message}");
        assert!(
            message.contains(
                "a script can read the value the build put there, not #2's output: a script \
                 never reads another script's output"
            ),
            "{message}"
        );
    }

    fn group(name: &str, fields: serde_json::Value) -> serde_json::Value {
        json!({ "field": name, "source": { "group": fields } })
    }

    fn to_one(name: &str, fields: serde_json::Value) -> serde_json::Value {
        json!({ "field": name, "source": { "relation": { "join": {
            "table": "orders", "kind": { "belongs_to": { "column": "order_id" } },
            "primary_key": "id", "nullable": true, "fields": fields,
        } } } })
    }

    /// What the last clause says for each pair of shapes; the Postgres build
    /// reads the last one's value through the first one's declaration.
    #[test]
    fn a_join_or_object_namesake_says_which_child_fields_survive() {
        let kept = |fields: serde_json::Value| {
            let found = duplicates(fields);
            assert_eq!(found.len(), 1, "{found:?}");
            let message = &found[0].2;
            let (_, clause) = message.split_once("; ").unwrap();
            clause.trim_end_matches(". Rename all but one").to_owned()
        };
        let a = || int_field("a");
        let b = || int_field("b");
        let c = || int_field("c");
        assert_eq!(
            kept(json!([
                orders_join("o", json!([a(), b()])),
                orders_join("o", json!([b(), c(), a()]))
            ])),
            "the index maps the last one, but a document holds its value cut to the child \
             fields #1 declares that #2 has too (`a`, `b`), read as #1 declares them, not the \
             object the index maps"
        );
        assert_eq!(
            kept(json!([group("o", json!([a()])), to_one("o", json!([b()]))])),
            "the index maps the last one, but a document keeps none of its child fields: #1 \
             and #2 declare none in common, so the object is empty, not the object the index \
             maps"
        );
        assert_eq!(
            kept(json!([
                orders_join("o", json!([a()])),
                group("o", json!([a(), b()]))
            ])),
            "the index maps the last one, but a document holds its value cut to the child \
             fields #1 declares that #2 has too (`a`), read as #1 declares them, not the \
             object the index maps"
        );
        assert_eq!(
            kept(json!([
                group("o", json!([a()])),
                orders_join("o", json!([a(), b()]))
            ])),
            "the index maps the last one, and a document holds the last one's value as raw JSON, \
             not converted to the declared types, so its values are not what the schema declares"
        );
        assert_eq!(
            kept(json!([int_field("o"), orders_join("o", json!([a()]))])),
            "the index maps the last one, and a document holds the last one's value as raw JSON, \
             not converted to the declared types, so its values are not what the schema declares"
        );
        assert_eq!(
            kept(json!([group("o", json!([a()])), int_field("o")])),
            "the index maps the last one, and a document holds the last one's value as raw \
             JSON, not converted to the declared types, so its values are not what the schema \
             declares"
        );
    }

    /// A namesake pair whose documents fail to build, or that OpenSearch
    /// rejects, says so and what `on_error` then does.
    #[test]
    fn a_namesake_that_fails_to_build_or_to_index_says_so() {
        let clause = |fields: serde_json::Value| {
            let found = duplicates(fields);
            assert_eq!(found.len(), 1, "{found:?}");
            let (_, clause) = found[0].2.split_once("; ").unwrap();
            clause.trim_end_matches(". Rename all but one").to_owned()
        };
        let tags = |ty: &str| {
            json!({ "field": "tags", "source": { "column": {
                "column": "tags", "ty": ty, "nullable": true, "array": true,
            } } })
        };
        let script = |ty: &str| {
            json!({ "field": "tags", "source": { "script": {
                "source": "\"x\"", "ty": ty, "nullable": true,
            } } })
        };
        let column = |ty: &str| {
            json!({ "field": "tags", "source": { "column": {
                "column": "tags", "ty": ty, "nullable": true,
            } } })
        };
        let build = |documents: &str| {
            format!(
                "the index maps the last one, but {documents} fails to build: #1 is an `array: \
                 true` column, which takes scalar elements, and the last one's value holds \
                 objects. With `on_error = stop` (the default) the ingest engine stops and \
                 restarts on it, which halts every index of the deployment; with `skip` such a \
                 document keeps its previous indexed version (none on a first backfill)"
            )
        };
        assert_eq!(
            clause(json!([
                tags("keyword"),
                orders_join("tags", json!([int_field("id")]))
            ])),
            build("every document with a related row")
        );
        assert_eq!(
            clause(json!([
                tags("keyword"),
                group("tags", json!([int_field("id")]))
            ])),
            build("every document")
        );
        assert_eq!(
            clause(json!([tags("integer"), column("json")])),
            build("every document whose value holds an object")
        );
        let mut hidden = tags("keyword");
        hidden["emit"] = json!(false);
        assert_eq!(
            clause(json!([
                hidden,
                orders_join("tags", json!([int_field("id")]))
            ])),
            build("every document with a related row"),
            "the build fails before the input-only one strips the name"
        );
        assert!(
            clause(json!([
                tags("json"),
                orders_join("tags", json!([int_field("id")]))
            ]))
            .ends_with("the last one's value, its elements read as the first one's type"),
            "a `json` array takes objects"
        );
        let rejects = |last: &str, which: &str| {
            format!(
                "the index maps the last one, {last}, but a document holds the last script's \
                 output (#1, a `keyword`): OpenSearch rejects {which}. With `on_error = stop` \
                 (the default) the sink stops and restarts on it, which halts every index it \
                 writes and pins the replication slot; with `skip` such a document is \
                 quarantined"
            )
        };
        assert_eq!(
            clause(json!([
                script("keyword"),
                orders_join("tags", json!([int_field("id")]))
            ])),
            rejects(
                "join on `orders`",
                "every document where the script returns a value"
            )
        );
        assert_eq!(
            clause(json!([script("keyword"), column("integer")])),
            rejects(
                "column `tags`",
                "every document whose output does not read as an `integer`"
            )
        );
        assert_eq!(
            clause(json!([script("keyword"), column("text")])),
            "the index maps the last one, and a document holds the last script's output (#1, \
             a `keyword`)"
        );
        assert_eq!(
            clause(json!([column("date"), script("timestamp")])),
            "the index maps the last one, and a document holds the last script's output (#2, \
             a `date`)"
        );
    }

    /// Every `#n` in a warning is the field's place at its level, never its
    /// place among the namesakes.
    #[test]
    fn a_warning_numbers_every_field_by_its_place_at_the_level() {
        let message = |fields: serde_json::Value| {
            let found = duplicates(fields);
            assert_eq!(found.len(), 1, "{found:?}");
            found[0].2.clone()
        };
        let script = |name: &str, ty: &str| {
            json!({ "field": name, "source": { "script": {
                "source": "\"s\"", "ty": ty, "nullable": true,
            } } })
        };
        let age = json!({ "field": "v", "source": { "column": {
            "column": "age", "ty": "integer", "nullable": true,
        } } });
        assert_eq!(
            message(json!([int_field("id"), script("v", "keyword"), age])),
            "`v` is declared 2 times at this level: #2 (script), #3 (column `age`); the index \
             maps the last one, column `age`, but a document holds the last script's output \
             (#2, a `keyword`): OpenSearch rejects every document whose output does not read \
             as an `integer`. With `on_error = stop` (the default) the sink stops and restarts \
             on it, which halts every index it writes and pins the replication slot; with \
             `skip` such a document is quarantined. Rename all but one"
        );
        assert_eq!(
            message(json!([
                int_field("id"),
                script("v", "keyword"),
                orders_join("v", json!([int_field("id")]))
            ])),
            "`v` is declared 2 times at this level: #2 (script), #3 (join on `orders`); the \
             index maps the last one, join on `orders`, but a document holds the last \
             script's output (#2, a `keyword`): OpenSearch rejects every document where the \
             script returns a value. With `on_error = stop` (the default) the sink stops and \
             restarts on it, which halts every index it writes and pins the replication slot; \
             with `skip` such a document is quarantined. Rename all but one"
        );
        assert_eq!(
            message(json!([
                int_field("id"),
                script("v", "integer"),
                column_field("v", "name")
            ])),
            "`v` is declared 2 times at this level: #2 (script), #3 (column `name`); the index \
             maps the last one, and a document holds the last script's output (#2, an \
             `integer`). Rename all but one"
        );
        let point = json!({ "field": "v", "source": {
            "geo": { "lat": "lat", "lon": "lon", "nullable": true },
        } });
        assert_eq!(
            message(json!([
                int_field("id"),
                orders_join("v", json!([int_field("a")])),
                point
            ])),
            "`v` is declared 2 times at this level: #2 (join on `orders`), #3 (geo point); the \
             index maps the last one, geo point, but a document holds its point cut to the \
             child fields #2 declares, with no `lat` and `lon`: OpenSearch rejects every \
             document with a point. With `on_error = stop` (the default) the sink stops and \
             restarts on it, which halts every index it writes and pins the replication slot; \
             with `skip` such a document is quarantined. Rename all but one"
        );
    }

    /// A constant builds what it holds when it comes first; last, a list or
    /// an object reaches the first one as `null` and a number or boolean as
    /// text. Positions are each field's place at the level.
    #[test]
    fn a_constant_namesake_says_what_a_document_holds() {
        let message = |fields: serde_json::Value| {
            let found = duplicates(fields);
            assert_eq!(found.len(), 1, "{found:?}");
            let (_, clause) = found[0].2.split_once("; ").unwrap();
            clause.trim_end_matches(". Rename all but one").to_owned()
        };
        let constant =
            |value: serde_json::Value| json!({ "field": "v", "source": { "constant": value } });
        let tags = json!({ "field": "v", "source": { "column": {
            "column": "tags", "ty": "keyword", "nullable": true, "array": true,
        } } });
        let point = json!({ "field": "v", "source": {
            "geo": { "lat": "lat", "lon": "lon", "nullable": true },
        } });
        assert_eq!(
            message(json!([
                int_field("id"),
                tags.clone(),
                constant(json!({ "Array": [{ "BigInt": 1 }, { "BigInt": 2 }] }))
            ])),
            "the index maps the last one, but every document holds `null`, not the constant the \
             schema declares: #3 is a constant holding a list or an object, which the document \
             query reads as `null`, and #2 reads that"
        );
        assert_eq!(
            message(json!([
                int_field("id"),
                tags,
                constant(json!({ "BigInt": 5 }))
            ])),
            "the index maps the last one, but a document holds the last one's value as text, \
             in a one-element array since #2 is an `array: true` column, not the value the \
             schema declares: #3 is a number constant, which reaches #2 as a string (`\"5\"`), \
             and only a `decimal` field reads a string as a number"
        );
        let column = |name: &str, ty: &str| {
            json!({ "field": "v", "source": { "column": {
                "column": name, "ty": ty, "nullable": true,
            } } })
        };
        assert_eq!(
            message(json!([
                int_field("id"),
                column("k", "keyword"),
                column("bin", "binary")
            ])),
            "the index maps the last one, but a document holds the last one's value as text, \
             not the value the schema declares: #3 is a `binary` column, whose bytes reach #2 \
             as Postgres hex text (`\"\\x0102\"`), not base64: only a `binary` field decodes \
             them"
        );
        assert_eq!(
            message(json!([
                int_field("id"),
                column("bin", "binary"),
                column("bin", "binary")
            ])),
            "the index maps the last one, and a document holds the last one's value, read as \
             the first one declares it"
        );
        assert_eq!(
            message(json!([
                int_field("id"),
                column("n", "integer"),
                constant(json!({ "Bool": true }))
            ])),
            "the index maps the last one, but a document holds the last one's value as text, \
             not the value the schema declares: #3 is a boolean constant, which reaches #2 as \
             the string `\"true\"` or `\"false\"`"
        );
        assert_eq!(
            message(json!([
                int_field("id"),
                column("dec", "decimal"),
                constant(json!({ "Double": 2.5 }))
            ])),
            "the index maps the last one, and a document holds the last one's value, read as \
             the first one declares it"
        );
        assert_eq!(
            message(json!([
                int_field("id"),
                constant(json!({ "Array": [{ "BigInt": 1 }, { "BigInt": 2 }] })),
                point.clone()
            ])),
            "the index maps the last one, and a document holds the first one's constant (#2), \
             which a constant builds whatever the last one reads"
        );
        assert_eq!(
            message(json!([
                int_field("id"),
                constant(json!({ "BigInt": 5 })),
                point
            ])),
            "the index maps the last one, geo point, but a document holds the first one's \
             constant (#2), which a constant builds whatever the last one reads: OpenSearch \
             rejects every document. With `on_error = stop` (the default) the sink stops and \
             restarts on it, which halts every index it writes and pins the replication slot; \
             with `skip` such a document is quarantined"
        );
        assert_eq!(
            message(json!([
                int_field("id"),
                constant(json!({ "String": "hello" })),
                column_field("v", "k")
            ])),
            "the index maps the last one, and a document holds the first one's constant (#2), \
             which a constant builds whatever the last one reads"
        );
    }

    /// A finding on one of two namesakes says which one, by position.
    #[test]
    fn a_finding_under_a_duplicated_name_carries_its_position() {
        let found = shop_problems(json!([
            int_field("id"),
            int_field("name"),
            json!({ "field": "name", "source": { "column": {
                "column": "nme", "ty": "integer", "nullable": true,
            } } }),
        ]));
        assert_eq!(
            found
                .iter()
                .map(|(at, kind, _)| (at.as_str(), *kind))
                .collect::<Vec<_>>(),
            [
                ("name#3", DiagnosticKind::UnknownColumn),
                ("name", DiagnosticKind::DuplicateFieldName),
            ],
        );
    }

    #[test]
    fn an_input_only_or_script_namesake_says_what_the_document_holds() {
        let mut hidden = column_field("name", "full_name");
        hidden["emit"] = json!(false);
        let script = json!({ "field": "name", "source": { "script": {
            "source": "this.name", "ty": "keyword", "nullable": true,
        } } });
        let messages = |fields| {
            duplicates(fields)
                .into_iter()
                .map(|(_, _, message)| message)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            messages(json!([column_field("name", "name"), hidden.clone()])),
            [
                "`name` is declared 2 times at this level: #1 (column `name`), #2 (column \
              `full_name`, `emit: false`); the index maps the last emitted one, but the \
              input-only (`emit: false`) one strips the name from every document, so no \
              document holds the field the index maps. Rename all but one"
            ],
        );
        assert_eq!(
            messages(json!([script, column_field("name", "name")])),
            [
                "`name` is declared 2 times at this level: #1 (script), #2 (column `name`); the \
              index maps the last one, and a document holds the last script's output (#1, a \
              `keyword`). Rename all but one"
            ],
        );
        assert_eq!(
            messages(json!([hidden.clone(), hidden])),
            [
                "`name` is declared 2 times at this level: #1 (column `full_name`, `emit: false`), \
              #2 (column `full_name`, `emit: false`); none reaches the document, and no \
              script reads this level. Rename all but one"
            ],
        );
    }

    /// Namesakes that are all input-only are meant to reach no document: the
    /// warning (the live matrix's `keyword emit: false` pair). A script that
    /// reads their level reads one value under the name: what it reads decides,
    /// as a document's value does elsewhere.
    #[test]
    fn input_only_namesakes_are_a_warning_unless_a_script_reads_another_shape() {
        let quiet = |mut field: serde_json::Value| {
            field["emit"] = json!(false);
            field
        };
        let five = quiet(json!({ "field": "v", "source": { "constant": { "BigInt": 5 } } }));
        let object = quiet(group("v", json!([column_field("k", "k")])));
        let (a, b) = (
            quiet(column_field("v", "full_name")),
            quiet(column_field("v", "name")),
        );
        let reader = script_field("s", "this.v");
        let warning = DiagnosticKind::DuplicateFieldName;
        let wrong = DiagnosticKind::DuplicateFieldNameWrong;
        for (fields, expected, says) in [
            (
                json!([int_field("id"), a.clone(), b.clone()]),
                warning,
                "none reaches the document, and no script reads this level",
            ),
            (
                json!([int_field("id"), reader.clone(), a, b]),
                warning,
                "a script can read the last one's value, read as the first one declares it",
            ),
            (
                json!([int_field("id"), five.clone(), object.clone()]),
                warning,
                "none reaches the document, and no script reads this level",
            ),
            (
                json!([int_field("id"), reader, five, object]),
                wrong,
                "a script can read the first one's constant (#3)",
            ),
        ] {
            let (kind, message) = stated(fields.clone());
            assert_eq!(kind, expected, "{fields}");
            assert!(message.contains(says), "{message}");
            says_nothing_is_refused(&message);
        }
    }

    #[test]
    fn validate_indexes_reports_duplicates_too() {
        assert_eq!(
            shop_problems(json!([
                int_field("id"),
                int_field("name"),
                int_field("name")
            ])),
            problems(&[(
                "name",
                DiagnosticKind::DuplicateFieldName,
                "`name` is declared 2 times at this level: #2 (column `name`), #3 (column \
                 `name`); the index maps the last one, and a document holds the last one's \
                 value, read as the first one declares it. Rename all but one"
            )]),
        );
    }

    #[test]
    fn a_field_on_a_missing_root_table_is_a_diagnostic() {
        let schema: IndexSchema = serde_json::from_value(json!({
            "version": 1,
            "table": "user",
            "db_schema": "public",
            "fields": [int_field("id")],
        }))
        .unwrap();
        let spec = SourceSpec::new(BTreeMap::from([(
            IndexName::try_new("users").unwrap(),
            schema,
        )]));
        let diagnostics = futures::executor::block_on(validate_indexes(&spec, &Shop)).unwrap();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].kind, DiagnosticKind::UnknownTable);
        assert_eq!(
            diagnostics[0].message,
            "references unknown table `public.user`"
        );
    }

    #[test]
    fn the_declared_root_primary_key_must_exist_even_when_no_field_reads_it() {
        assert_eq!(
            root_problems(json!({ "primary_key": "uid" })),
            problems(&[(
                "@primary_key",
                DiagnosticKind::UnknownColumn,
                "`primary_key` references unknown column `public.users.uid`"
            )]),
        );
    }

    /// `(path, kind, message)` for an index over `table`, reading `user_id`,
    /// with `primary_key` declared.
    fn root_key_problems(table: &str, primary_key: Option<&str>) -> Vec<Found> {
        let mut schema = json!({
            "version": 1, "table": table, "db_schema": "public",
            "fields": [int_field("user_id")],
        });
        if let Some(primary_key) = primary_key {
            schema["primary_key"] = json!(primary_key);
        }
        let spec = SourceSpec::new(BTreeMap::from([(
            IndexName::try_new(table).unwrap(),
            serde_json::from_value(schema).unwrap(),
        )]));
        located(futures::executor::block_on(validate_indexes(&spec, &Shop)).unwrap())
    }

    #[test]
    fn a_root_table_without_a_single_column_primary_key_is_reported_at_table() {
        assert_eq!(
            root_key_problems("notes", None),
            problems(&[(
                "@table",
                DiagnosticKind::RootPrimaryKey,
                "`public.notes` has no primary key: the backfill skips it; live changes are \
                 still indexed"
            )]),
        );
        assert_eq!(
            root_key_problems("pairs", Some("a")),
            problems(&[(
                "@table",
                DiagnosticKind::RootPrimaryKey,
                "`public.pairs` has a composite primary key (`a`, `b`): the backfill skips it; \
                 live changes are still indexed"
            )]),
        );
    }

    /// A root change on a table without a single-column key is built by its
    /// whole key, so a declared `primary_key` naming no column is only read
    /// by a relation: with none it is the same kind, which starts; with one
    /// it is an unknown column, which doesn't.
    #[test]
    fn a_missing_declared_key_on_a_root_without_a_single_column_key_stops_only_with_a_relation() {
        let spec = SourceSpec::new(BTreeMap::from([(
            IndexName::try_new("notes").unwrap(),
            serde_json::from_value(json!({
                "version": 1, "table": "notes", "db_schema": "public", "primary_key": "id",
                "fields": [{ "field": "meta", "source": { "group": [
                    { "field": "user", "source": { "relation": { "join": {
                        "table": "users", "kind": { "belongs_to": { "column": "user_id" } },
                        "primary_key": "id", "fields": [],
                    } } } },
                ] } }],
            }))
            .unwrap(),
        )]));
        assert_eq!(
            located(futures::executor::block_on(validate_indexes(&spec, &Shop)).unwrap()),
            problems(&[
                (
                    "@table",
                    DiagnosticKind::RootPrimaryKey,
                    "`public.notes` has no primary key: the backfill skips it; live changes \
                     are still indexed"
                ),
                (
                    "@primary_key",
                    DiagnosticKind::UnknownColumn,
                    "`primary_key` references unknown column `public.notes.id`"
                ),
            ]),
        );
        assert_eq!(
            root_key_problems("notes", Some("id")),
            problems(&[
                (
                    "@table",
                    DiagnosticKind::RootPrimaryKey,
                    "`public.notes` has no primary key: the backfill skips it; live changes \
                     are still indexed"
                ),
                (
                    "@primary_key",
                    DiagnosticKind::RootPrimaryKey,
                    "`primary_key` references unknown column `public.notes.id`; a root change \
                     built by its whole key does not read it"
                ),
            ]),
        );
    }

    #[test]
    fn a_declared_key_other_than_the_tables_own_is_a_mismatch() {
        assert_eq!(
            root_problems(json!({ "primary_key": "team_id" })),
            problems(&[(
                "@primary_key",
                DiagnosticKind::RootPrimaryKeyMismatch,
                "`primary_key` is `team_id`, but `public.users` is keyed by `id`: the backfill \
                 and every root change would match the wrong row or none"
            )]),
        );
    }

    /// A root keyed by a type the source can't read back writes no document
    /// (issue #295); a relation keyed by one is not asked about.
    #[test]
    fn a_root_key_the_source_cannot_read_back_is_reported_at_primary_key() {
        struct Unreadable;

        #[async_trait]
        impl Catalog for Unreadable {
            async fn column(
                &self,
                schema: &DatabaseSchema,
                table: &TableName,
                column: &ColumnName,
            ) -> Result<ColumnInfo> {
                Shop.column(schema, table, column).await
            }

            async fn has_table(&self, schema: &DatabaseSchema, table: &TableName) -> Result<bool> {
                Shop.has_table(schema, table).await
            }

            async fn primary_key(
                &self,
                schema: &DatabaseSchema,
                table: &TableName,
            ) -> Result<Option<Vec<ColumnName>>> {
                Shop.primary_key(schema, table).await
            }

            async fn reads_key(
                &self,
                _: &DatabaseSchema,
                _: &TableName,
                _: &ColumnName,
            ) -> Result<bool> {
                Ok(false)
            }
        }

        let spec = users_spec(json!({
            "fields": [{ "field": "team", "source": { "relation": { "join": {
                "table": "teams", "kind": { "belongs_to": { "column": "team_id" } },
                "primary_key": "id", "nullable": true, "fields": [],
            } } } }],
        }));
        assert_eq!(
            located(futures::executor::block_on(validate_indexes(&spec, &Unreadable)).unwrap()),
            problems(&[(
                "@primary_key",
                DiagnosticKind::UnsupportedKeyType,
                "`primary_key` `id` is `integer`, a type that can't be read back as a document \
                 key: the backfill would index no row, and no change on a related table would \
                 reach a document"
            )]),
        );
    }

    /// A catalog that can't tell a primary key (the trait's default) skips
    /// every primary-key check but the declared column's existence.
    #[test]
    fn an_unknown_primary_key_skips_the_key_checks() {
        struct Plain;

        #[async_trait]
        impl Catalog for Plain {
            async fn column(
                &self,
                schema: &DatabaseSchema,
                table: &TableName,
                column: &ColumnName,
            ) -> Result<ColumnInfo> {
                Shop.column(schema, table, column).await
            }

            async fn has_table(&self, schema: &DatabaseSchema, table: &TableName) -> Result<bool> {
                Shop.has_table(schema, table).await
            }
        }

        let spec = users_spec(json!({
            "primary_key": "uid",
            "fields": [{ "field": "notes", "source": { "relation": { "join": {
                "table": "notes", "kind": { "has_many": { "foreign_key": "user_id" } },
                "primary_key": "user_id", "fields": [],
            } } } }],
        }));
        assert_eq!(
            located(futures::executor::block_on(validate_indexes(&spec, &Plain)).unwrap()),
            problems(&[(
                "@primary_key",
                DiagnosticKind::UnknownColumn,
                "`primary_key` references unknown column `public.users.uid`"
            )]),
        );
    }

    /// Every table a relation joins or hops through needs a single-column
    /// primary key; a junction does not.
    #[test]
    fn a_relation_table_needs_a_single_column_primary_key() {
        let has_many = |name: &str, table: &str| {
            json!({ "field": name, "source": { "relation": { "join": {
                "table": table, "kind": { "has_many": { "foreign_key": "user_id" } },
                "primary_key": "id", "fields": [],
            } } } })
        };
        let found = shop_problems(json!([
            has_many("notes", "notes"),
            has_many("pairs", "pairs"),
            { "field": "noteItems", "source": { "relation": { "aggregate": {
                "table": "items", "op": "count", "key": { "direct": "order_id" },
                "via": [{ "table": "notes", "foreign_key": "user_id" }],
            } } } },
            { "field": "noteCount", "source": { "relation": { "aggregate": {
                "table": "notes", "op": "count", "key": { "direct": "user_id" },
            } } } },
            { "field": "tags", "source": { "relation": { "join": {
                "table": "tags", "primary_key": "id",
                "kind": { "many_to_many": { "through": {
                    "table": "user_tags", "left_key": "user_id", "right_key": "tag_id",
                } } },
                "fields": [],
            } } } },
        ]));
        assert_eq!(
            found,
            problems(&[
                (
                    "notes",
                    DiagnosticKind::RelationPrimaryKey,
                    "`table` `public.notes` has no primary key; a relation table needs a \
                     single-column one"
                ),
                (
                    "notes",
                    DiagnosticKind::IgnoredPrimaryKey,
                    "`primary_key` references unknown column `public.notes.id`; ignored: the \
                     join is keyed by the table's own primary key"
                ),
                (
                    "pairs",
                    DiagnosticKind::RelationPrimaryKey,
                    "`table` `public.pairs` has a composite primary key (`a`, `b`); a relation \
                     table needs a single-column one"
                ),
                (
                    "pairs",
                    DiagnosticKind::IgnoredPrimaryKey,
                    "`primary_key` references unknown column `public.pairs.id`; ignored: the \
                     join is keyed by the table's own primary key"
                ),
                (
                    "noteItems",
                    DiagnosticKind::RelationPrimaryKey,
                    "`via.table` `public.notes` has no primary key; a relation table needs a \
                     single-column one"
                ),
                (
                    "noteCount",
                    DiagnosticKind::RelationPrimaryKey,
                    "`table` `public.notes` has no primary key; a relation table needs a \
                     single-column one"
                ),
            ]),
        );
    }

    /// A link that doesn't compare with its key is reported once, on the
    /// relation; a link column that doesn't exist only as unknown.
    #[test]
    fn a_link_that_does_not_compare_with_its_key_is_reported_on_the_relation() {
        let has_many = |name: &str, foreign_key: &str| {
            json!({ "field": name, "source": { "relation": { "join": {
                "table": "orders", "kind": { "has_many": { "foreign_key": foreign_key } },
                "primary_key": "id", "fields": [],
            } } } })
        };
        let found = shop_problems(json!([
            has_many("orders", "user_id"),
            has_many("byRef", "user_ref"),
            has_many("byTypo", "user_rf"),
        ]));
        assert_eq!(
            found,
            problems(&[
                (
                    "byRef",
                    DiagnosticKind::LinkTypeMismatch,
                    "`foreign_key` `orders.user_ref` (`integer`) can't be compared with \
                     `users.id` (`integer`): every document build fails"
                ),
                (
                    "byTypo",
                    DiagnosticKind::UnknownColumn,
                    "`foreign_key` references unknown column `public.orders.user_rf`"
                ),
            ]),
        );
    }

    #[test]
    fn transforms_must_apply_to_the_column_type() {
        let lowered = |name: &str, transforms: serde_json::Value| {
            json!({ "field": name, "source": { "column": {
                "column": name, "ty": "integer", "nullable": true, "transforms": transforms,
            } } })
        };
        assert_eq!(
            shop_problems(json!([
                lowered("name", json!(["lowercase", "trim"])),
                lowered("team_id", json!(["lowercase", "trim"])),
            ])),
            problems(&[(
                "team_id",
                DiagnosticKind::TransformNotApplicable,
                "`transforms` `lowercase`, `trim` do not apply to the column's database type \
                 `integer`"
            )]),
        );
    }

    /// A check the store refuses to run is a warning of its own, naming what
    /// was not checked and the store's reason (issue #324).
    #[test]
    fn a_check_the_store_refuses_to_run_is_reported_as_skipped() {
        let found = shop_problems(json!([
            { "field": "label", "source": { "column": {
                "column": "label", "ty": "integer", "nullable": true, "transforms": ["lowercase"],
            } } },
            { "field": "byLabel", "source": { "relation": { "join": {
                "table": "orders", "kind": { "has_many": { "foreign_key": "user_label" } },
                "primary_key": "id", "fields": [],
            } } } },
        ]));
        assert_eq!(
            found,
            problems(&[
                (
                    "label",
                    DiagnosticKind::CheckSkipped,
                    "could not check `transforms` `lowercase` against the column's database \
                     type `integer`: permission denied for schema hidden"
                ),
                (
                    "byLabel",
                    DiagnosticKind::CheckSkipped,
                    "could not check `foreign_key` `orders.user_label` (`integer`) against \
                     `users.id` (`integer`): permission denied for schema hidden"
                ),
            ]),
        );
        assert_eq!(DiagnosticKind::CheckSkipped.severity(), Severity::Warning);
        assert_eq!(DiagnosticKind::CheckSkipped.as_str(), "check_skipped");
    }

    /// The designer tells a root field named like an index-level key from the
    /// key by the anchor.
    #[test]
    fn a_field_named_like_an_index_key_has_its_own_anchor() {
        let field = json!({ "field": "filters", "source": { "column": {
            "column": "nme", "ty": "integer", "nullable": true,
        } } });
        assert_eq!(
            root_problems(json!({
                "fields": [int_field("id"), field],
                "filters": [value_filter("nme")],
            })),
            problems(&[
                (
                    "filters",
                    DiagnosticKind::UnknownColumn,
                    "references unknown column `public.users.nme`"
                ),
                (
                    "@filters",
                    DiagnosticKind::UnknownColumn,
                    "`filters` references unknown column `public.users.nme`"
                ),
            ]),
        );
    }
}

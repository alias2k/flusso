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
//! The one store-specific piece is the [`Catalog`] trait: how a store reports a
//! table and a column. Everything else — the field walk, which table each name
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
//!             ("users", "id") => Ok(ColumnInfo { sql_type: "integer".into(), nullable: false }),
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
//!     "version": 1, "table": "users", "db_schema": "public",
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
use kernel::common::{ColumnName, FieldName, IndexName};
use kernel::{
    Aggregate, AggregateKey, AggregateOp, Column, ColumnPair, DatabaseSchema, Field, FieldSource,
    Filter, FlussoType, Geo, Join, JoinKind, Relation, TableName, Through,
};

use crate::{Result, SourceError, SourceSpec};

/// How a store reports one base-table column: its native type name (as the
/// store spells it, e.g. Postgres `character varying(255)`) and whether it
/// admits null.
#[derive(Debug, Clone)]
pub struct ColumnInfo {
    pub sql_type: String,
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
}

/// How serious a [`Diagnostic`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// The declared schema and the database disagree in a way that will produce
    /// a wrong or rejected mapping.
    Error,
    /// A softer mismatch worth surfacing (e.g. a column declared non-null that
    /// the database allows to be null).
    Warning,
}

/// One disagreement between a declared schema and the live database.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// The index whose schema disagrees.
    pub index: IndexName,
    /// The field the disagreement is on: the last segment of [`path`](Self::path).
    pub field: FieldName,
    /// The field's path from the document root, e.g. `orders`, `items`, `sku`.
    pub path: Vec<FieldName>,
    pub severity: Severity,
    pub message: String,
}

impl Diagnostic {
    /// The field path joined with `.`, e.g. `orders.items.sku`.
    pub fn dotted_path(&self) -> String {
        self.path
            .iter()
            .map(AsRef::as_ref)
            .collect::<Vec<&str>>()
            .join(".")
    }
}

/// Validate every index in `spec` against the store behind `catalog`, returning
/// the disagreements found. An empty result means the declared schema matches
/// the database. The spec already holds only enabled indexes.
pub async fn validate_indexes(spec: &SourceSpec, catalog: &dyn Catalog) -> Result<Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();
    for (name, schema) in spec.indexes() {
        let mut checker = Checker {
            index: name,
            db_schema: &schema.db_schema,
            catalog,
            tables: HashMap::new(),
            path: Vec::new(),
            out: &mut diagnostics,
        };
        checker
            .fields(&schema.table, &schema.fields, schema.primary_key.as_ref())
            .await?;
    }
    Ok(diagnostics)
}

/// One index's walk: where it is (`path`), what it has found (`out`), and the
/// tables it already looked up.
struct Checker<'a> {
    index: &'a IndexName,
    db_schema: &'a DatabaseSchema,
    catalog: &'a dyn Catalog,
    tables: HashMap<TableName, bool>,
    path: Vec<FieldName>,
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

impl Checker<'_> {
    /// Report a disagreement on the field at the current path.
    fn report(&mut self, severity: Severity, message: String) {
        let Some(field) = self.path.last() else {
            return;
        };
        self.out.push(Diagnostic {
            index: self.index.clone(),
            field: field.clone(),
            path: self.path.clone(),
            severity,
            message,
        });
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
            for field in fields {
                self.path.push(field.field.clone());
                self.field(table, field, primary_key).await?;
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
        match &field.source {
            FieldSource::Column(column) => self.column_field(table, column, primary_key).await,
            FieldSource::Relation(Relation::Aggregate(aggregate)) => {
                self.aggregate(table, aggregate).await
            }
            FieldSource::Relation(Relation::Join(join)) => self.join(table, join).await,
            FieldSource::Group(fields) => self.fields(table, fields, primary_key).await,
            FieldSource::Geo(geo) => self.geo(table, geo).await,
            FieldSource::Constant(_) => Ok(()),
        }
    }

    /// Whether `table` exists, reporting it under `role` when it doesn't.
    async fn table(&mut self, table: &TableName, role: &str) -> Result<bool> {
        let exists = match self.tables.get(table) {
            Some(exists) => *exists,
            None => {
                let exists = self.catalog.has_table(self.db_schema, table).await?;
                self.tables.insert(table.clone(), exists);
                exists
            }
        };
        if !exists {
            self.report(
                Severity::Error,
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
                    Severity::Error,
                    format!("{prefix}references unknown column `{what}`"),
                );
                Ok(None)
            }
            Err(SourceError::UnknownTable(what)) => {
                self.report(
                    Severity::Error,
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
    /// table is missing), `related` on `related` (likewise).
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

    /// A join under `parent`: its table, key, `on:` pairs, primary key,
    /// filters and `order_by`, then its fields.
    async fn join(&mut self, parent: &TableName, join: &Join) -> Result<()> {
        let related = self.table(&join.table, "table").await?;
        match &join.kind {
            JoinKind::BelongsTo { column } => self.exists(parent, column, "column").await?,
            JoinKind::HasOne { foreign_key } | JoinKind::HasMany { foreign_key } => {
                if related {
                    self.exists(&join.table, foreign_key, "foreign_key").await?;
                }
            }
            JoinKind::ManyToMany { through } => self.through(through, "through").await?,
        }
        self.on_pairs(&join.on, Some(parent), related.then_some(&join.table), "on")
            .await?;
        if !related {
            return Ok(());
        }
        self.exists(&join.table, &join.primary_key, "primary_key")
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
                self.exists(table, &hop.foreign_key, "via.foreign_key")
                    .await?;
            }
            self.on_pairs(&hop.on, before, here, "via.on").await?;
            if let Some(table) = here {
                self.filters(table, hop.filters.as_deref(), "via.filters")
                    .await?;
            }
            before = here;
        }
        let related = self.table(&aggregate.table, "table").await?;
        match &aggregate.key {
            AggregateKey::Direct(foreign_key) => {
                if related {
                    self.exists(&aggregate.table, foreign_key, "foreign_key")
                        .await?;
                }
            }
            AggregateKey::Through(through) => self.through(through, "through").await?,
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
                if !value_type.accepts_pg(&info.sql_type) {
                    self.report(
                        Severity::Error,
                        format!(
                            "declared aggregate type does not accept the column's database type `{}`",
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
                    Severity::Error,
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
                    Severity::Error,
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

        if !column.ty.accepts_pg(&info.sql_type) {
            self.report(
                Severity::Error,
                format!(
                    "declared type does not accept the column's database type `{}`",
                    info.sql_type
                ),
            );
        }

        // A primary key or a `default` makes the field non-null regardless, so only
        // a plain non-null declaration over a nullable column is worth flagging.
        let forced_non_null = primary_key == Some(&column.column) || column.default.is_some();
        if !column.nullable && info.nullable && !forced_non_null {
            self.report(
                Severity::Warning,
                "declared non-null (`required`) but the database column allows null".to_owned(),
            );
        }
        Ok(())
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
        assert_eq!(diagnostics[0].severity, Severity::Error);
        assert!(
            diagnostics[0].message.contains("unknown column"),
            "got: {}",
            diagnostics[0].message,
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
                nullable: true,
            })
        }

        async fn has_table(&self, _: &DatabaseSchema, _: &TableName) -> Result<bool> {
            Ok(true)
        }
    }

    fn aggregate_messages(op: serde_json::Value, value_type: Option<&str>) -> Vec<String> {
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
            .map(|d| d.message)
            .collect()
    }

    #[test]
    fn every_expression_column_must_exist_and_be_numeric() {
        let messages = aggregate_messages(
            json!({ "sum": "quantity * unit_price + note - missing" }),
            Some("double"),
        );
        assert_eq!(messages.len(), 2, "{messages:?}");
        assert!(
            messages[0].contains("`note` must be numeric, found `text`"),
            "{messages:?}"
        );
        assert!(
            messages[1].contains("unknown column `public.order_lines.missing`"),
            "{messages:?}"
        );
        for op in ["avg", "min", "max"] {
            let messages = aggregate_messages(json!({ op: "quantity * unit_price" }), None);
            assert!(messages.is_empty(), "{op}: {messages:?}");
        }
    }

    #[test]
    fn a_plain_column_keeps_its_value_type_check() {
        let messages = aggregate_messages(json!({ "sum": "note" }), Some("double"));
        assert_eq!(messages.len(), 1, "{messages:?}");
        assert!(
            messages[0].contains("does not accept the column's database type `text`"),
            "{messages:?}"
        );
        assert!(aggregate_messages(json!({ "sum": "quantity" }), Some("integer")).is_empty());
    }

    /// `users(id, name, team_id)`, `teams(id, name)`, `orders(id, user_id,
    /// status, region)`, `items(id, order_id, qty, region)`, `tags(id, label)`,
    /// `user_tags(user_id, tag_id, kind)`; every column an `integer`.
    struct Shop;

    const SHOP: &[(&str, &[&str])] = &[
        ("users", &["id", "name", "team_id"]),
        ("teams", &["id", "name"]),
        ("orders", &["id", "user_id", "status", "region"]),
        ("items", &["id", "order_id", "qty", "region"]),
        ("tags", &["id", "label"]),
        ("user_tags", &["user_id", "tag_id", "kind"]),
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
                nullable: true,
            })
        }

        async fn has_table(&self, _: &DatabaseSchema, table: &TableName) -> Result<bool> {
            Ok(SHOP.iter().any(|(name, _)| *name == table.as_ref()))
        }
    }

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

    /// `(path, message)` for every diagnostic of a `users` index with `fields`.
    fn shop_problems(fields: serde_json::Value) -> Vec<(String, String)> {
        let schema: IndexSchema = serde_json::from_value(json!({
            "version": 1,
            "table": "users",
            "db_schema": "public",
            "primary_key": "id",
            "fields": fields,
        }))
        .unwrap();
        let spec = SourceSpec::new(BTreeMap::from([(
            IndexName::try_new("users").unwrap(),
            schema,
        )]));
        futures::executor::block_on(validate_indexes(&spec, &Shop))
            .unwrap()
            .into_iter()
            .map(|d| {
                assert_eq!(d.severity, Severity::Error);
                assert_eq!(d.path.last(), Some(&d.field));
                (d.dotted_path(), d.message)
            })
            .collect()
    }

    fn problems(expected: &[(&str, &str)]) -> Vec<(String, String)> {
        expected
            .iter()
            .map(|(path, message)| ((*path).to_owned(), (*message).to_owned()))
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
                    "`column` references unknown column `public.users.teamid`"
                ),
                (
                    "team",
                    "`on.local` references unknown column `public.users.nmae`"
                ),
                (
                    "team",
                    "`on.related` references unknown column `public.teams.nmae`"
                ),
                (
                    "team",
                    "`primary_key` references unknown column `public.teams.uid`"
                ),
                (
                    "team",
                    "`filters` references unknown column `public.teams.stat`"
                ),
                (
                    "team",
                    "`filters` references unknown column `public.teams.nam`"
                ),
                (
                    "team",
                    "`order_by` references unknown column `public.teams.rank`"
                ),
                (
                    "orders",
                    "`foreign_key` references unknown column `public.orders.userid`"
                ),
                (
                    "orders.items",
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
                    "`through.left_key` references unknown column `public.user_tags.user`"
                ),
                (
                    "tags",
                    "`through.right_key` references unknown column `public.user_tags.tag`"
                ),
                (
                    "tags",
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
                    "`via.foreign_key` references unknown column `public.orders.userid`"
                ),
                (
                    "itemCount",
                    "`via.on.local` references unknown column `public.users.nme`"
                ),
                (
                    "itemCount",
                    "`via.on.related` references unknown column `public.orders.regio`"
                ),
                (
                    "itemCount",
                    "`via.filters` references unknown column `public.orders.state`"
                ),
                (
                    "itemCount",
                    "`foreign_key` references unknown column `public.items.orderid`"
                ),
                (
                    "itemCount",
                    "`on.local` references unknown column `public.orders.regn`"
                ),
                (
                    "itemCount",
                    "`on.related` references unknown column `public.items.rgn`"
                ),
                (
                    "itemCount",
                    "`filters` references unknown column `public.items.qt`"
                ),
                (
                    "lostHop",
                    "`via.table` references unknown table `public.order`"
                ),
                (
                    "lostTable",
                    "`table` references unknown table `public.item`"
                ),
                (
                    "lostTable",
                    "`through.right_key` references unknown column `public.user_tags.tagid`"
                ),
            ]),
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
        assert_eq!(
            diagnostics[0].message,
            "references unknown table `public.user`"
        );
    }
}

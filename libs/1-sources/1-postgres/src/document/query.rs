//! SQL generation.
//!
//! The document query assembles a whole nested document **server-side** in one
//! round-trip: `json_build_object` for each level, relations as correlated
//! subqueries (`json_agg` for to-many, a scalar subquery for to-one and
//! aggregates), so nested relations never trigger extra queries. Existence and
//! soft-delete fold into the `WHERE`. Reverse-resolution queries (one selected
//! column, filtered by a key) live here too.
//!
//! Identifiers come from `nutype`-validated schema types (so quoting them is
//! injection-safe); every data value is a bound `$n` parameter.

use std::collections::HashMap;

use schema_core::{
    Aggregate, AggregateKey, AggregateOp, ColumnName, DatabaseSchema, Direction, Field,
    FieldSource, Filter, FilterOp, FilterValue, GenericValue, Geo, IndexSchema, Join, JoinKind,
    NullOp, OrderBy, Relation, SoftDelete, TableName, Transform, ValueOpFilter,
};
use sources_core::{Result, SourceError};
use sqlx::Postgres;
use sqlx::postgres::PgArguments;

use super::fields::field_column;

type PgQuery<'q> = sqlx::query::Query<'q, Postgres, PgArguments>;

const ROOT: &str = "root";

/// SQL assembled by this module's query builder, ready to hand to
/// [`sqlx::query`](fn@sqlx::query).
///
/// Since sqlx 0.9, [`sqlx::query`](fn@sqlx::query) only accepts strings that implement
/// [`SqlSafeStr`](sqlx::SqlSafeStr) — natively just `&'static str` — to stop
/// dynamic data being interpolated into SQL. Everything we build here is
/// dynamic, so wrapping it in this type is the single audit point: a value of
/// `SqlString` asserts that the SQL was assembled the safe way — identifiers
/// come from `nutype`-validated schema types (so quoting them is
/// injection-safe) and every data value is a bound `$n` parameter, never
/// formatted into the string. Construct it only from query-builder output.
#[derive(Debug, Clone)]
pub(super) struct SqlString(String);

impl SqlString {
    fn new(sql: String) -> Self {
        Self(sql)
    }

    #[cfg(test)]
    pub(super) fn as_str(&self) -> &str {
        &self.0
    }
}

impl sqlx::SqlSafeStr for SqlString {
    fn into_sql_str(self) -> sqlx::SqlStr {
        // Safe by construction — see the type's documentation.
        sqlx::AssertSqlSafe(self.0).into_sql_str()
    }
}

/// Bind a scalar parameter onto a query, in `params` order.
pub(super) fn bind_param<'q>(query: PgQuery<'q>, value: &GenericValue) -> Result<PgQuery<'q>> {
    Ok(match value {
        GenericValue::Int(i) => query.bind(*i),
        GenericValue::Bool(b) => query.bind(*b),
        GenericValue::Decimal(d) => query.bind(*d),
        GenericValue::String(s) => query.bind(s.clone()),
        GenericValue::Null | GenericValue::Array(_) | GenericValue::Map(_) => {
            return Err(SourceError::Query(
                "cannot bind null, array, or map as a parameter".into(),
            ));
        }
    })
}

/// Build the single query that assembles one document, given its key. Returns
/// the SQL (selecting one `json` column named `document`) and its bound params.
pub(super) fn document_query(
    schema: &IndexSchema,
    key: &[(ColumnName, GenericValue)],
    pks: &HashMap<String, ColumnName>,
    col_types: &HashMap<(String, String), String>,
) -> Result<(SqlString, Vec<GenericValue>)> {
    let mut builder = Builder {
        db: &schema.db_schema,
        pks,
        col_types,
        params: Vec::new(),
        seq: 0,
    };

    let object = builder.object(&schema.fields, ROOT, schema.primary_key.as_ref())?;

    let mut conditions = Vec::new();
    for (column, value) in key {
        let placeholder = builder.placeholder(value.clone())?;
        conditions.push(format!("{} = {placeholder}", qcol(ROOT, column)));
    }
    if let Some(predicate) = builder.soft_delete_predicate(schema)? {
        conditions.push(format!("NOT ({predicate})"));
    }
    if conditions.is_empty() {
        conditions.push("true".to_owned());
    }
    // Root filters scope which rows are documents at all; a row outside the
    // set returns nothing → a tombstone, exactly like soft-delete.
    let root_filters = builder.filters(schema.filters.as_deref(), ROOT, &schema.table)?;

    let sql = format!(
        "SELECT {object} AS \"document\" FROM {} AS \"{ROOT}\" WHERE {}{root_filters}",
        qtable(&schema.db_schema, &schema.table),
        conditions.join(" AND "),
    );
    Ok((SqlString::new(sql), builder.params))
}

/// Build a single query that assembles every document whose root key is in
/// `keys`, for an index with a single-column root key (`pk_column`). Selects
/// the root key as the first column (`doc_key`) beside the assembled document,
/// so the caller can match each row back to its id; a key with no matching
/// row simply doesn't come back, which the caller reads as a tombstone.
///
/// The document is assembled exactly as in [`document_query`] — same nested
/// `json_build_object` / `json_agg` — differing only in selecting the key and
/// matching the root with `IN (…)` instead of a single equality. The key is
/// wrapped in `to_json` so it decodes through the same path as the document.
pub(super) fn documents_query(
    schema: &IndexSchema,
    pk_column: &ColumnName,
    keys: &[GenericValue],
    pks: &HashMap<String, ColumnName>,
    col_types: &HashMap<(String, String), String>,
) -> Result<(SqlString, Vec<GenericValue>)> {
    let mut builder = Builder {
        db: &schema.db_schema,
        pks,
        col_types,
        params: Vec::new(),
        seq: 0,
    };

    // Build the object first: its filters push the leading `$n` params, exactly
    // as `document_query` does, so the key placeholders that follow come after.
    let object = builder.object(&schema.fields, ROOT, schema.primary_key.as_ref())?;

    let mut placeholders = Vec::with_capacity(keys.len());
    for key in keys {
        placeholders.push(builder.placeholder(key.clone())?);
    }
    let mut predicate = format!("{} IN ({})", qcol(ROOT, pk_column), placeholders.join(", "),);
    if let Some(soft_delete) = builder.soft_delete_predicate(schema)? {
        predicate = format!("{predicate} AND NOT ({soft_delete})");
    }
    // Root filters: a requested key outside the set comes back as no row,
    // which the caller reads as a tombstone.
    let root_filters = builder.filters(schema.filters.as_deref(), ROOT, &schema.table)?;
    predicate.push_str(&root_filters);

    let sql = format!(
        "SELECT to_json({key}) AS \"doc_key\", {object} AS \"document\" \
         FROM {} AS \"{ROOT}\" WHERE {predicate}",
        qtable(&schema.db_schema, &schema.table),
        key = qcol(ROOT, pk_column),
    );
    Ok((SqlString::new(sql), builder.params))
}

/// Build a reverse-resolution query: one column from a table, filtered by a key.
pub(super) fn reverse_query(
    db: &DatabaseSchema,
    table: &TableName,
    select_column: &ColumnName,
    key: &[(ColumnName, GenericValue)],
) -> Result<(SqlString, Vec<GenericValue>)> {
    let mut params = Vec::new();
    let mut conditions = Vec::new();
    for (column, value) in key {
        if !value.is_bindable_scalar() {
            return Err(SourceError::Query(
                "cannot bind null, array, or map as a key".into(),
            ));
        }
        params.push(value.clone());
        conditions.push(format!("{} = ${}", qident(column.as_ref()), params.len()));
    }
    if conditions.is_empty() {
        conditions.push("true".to_owned());
    }
    let sql = format!(
        "SELECT {} FROM {} WHERE {}",
        qident(select_column.as_ref()),
        qtable(db, table),
        conditions.join(" AND "),
    );
    Ok((SqlString::new(sql), params))
}

/// Accumulates parameters and unique aliases while building a document query.
struct Builder<'a> {
    db: &'a DatabaseSchema,
    pks: &'a HashMap<String, ColumnName>,
    /// `(table, column)` → SQL type, for casting filter operands.
    col_types: &'a HashMap<(String, String), String>,
    params: Vec<GenericValue>,
    seq: usize,
}

impl Builder<'_> {
    fn placeholder(&mut self, value: GenericValue) -> Result<String> {
        if !value.is_bindable_scalar() {
            return Err(SourceError::Query(
                "cannot bind null, array, or map as a parameter".into(),
            ));
        }
        self.params.push(value);
        Ok(format!("${}", self.params.len()))
    }

    fn alias(&mut self) -> String {
        self.seq += 1;
        format!("rel{}", self.seq)
    }

    fn pk_of(&self, table: &TableName) -> Result<ColumnName> {
        self.pks.get(&table.to_string()).cloned().ok_or_else(|| {
            SourceError::Query(format!("internal: missing primary key for `{table}`"))
        })
    }

    /// `json_build_object('field', <expr>, …)` over a set of fields, where
    /// `parent_alias` is the row in scope and `parent_pk` keys its relations.
    fn object(
        &mut self,
        fields: &[Field],
        parent_alias: &str,
        parent_pk: Option<&ColumnName>,
    ) -> Result<String> {
        let mut pairs = Vec::with_capacity(fields.len());
        for field in fields {
            let value = self.field_value(field, parent_alias, parent_pk)?;
            pairs.push(format!("{}, {value}", json_key(field.field.as_ref())));
        }
        Ok(format!("json_build_object({})", pairs.join(", ")))
    }

    fn field_value(
        &mut self,
        field: &Field,
        parent_alias: &str,
        parent_pk: Option<&ColumnName>,
    ) -> Result<String> {
        match &field.source {
            FieldSource::Relation(Relation::Join(join)) => {
                self.join_value(join, parent_alias, parent_pk)
            }
            FieldSource::Relation(Relation::Aggregate(aggregate)) => {
                self.aggregate_value(aggregate, parent_alias, parent_pk)
            }
            FieldSource::Column(column) => Ok(column_value(
                &column.column,
                &column.transforms,
                column.default.as_ref(),
                parent_alias,
            )),
            // Same-row nested group: same row, same key.
            FieldSource::Group(nested) => self.object(nested, parent_alias, parent_pk),
            // Two same-row columns assembled into a `{lat, lon}` point.
            FieldSource::Geo(geo) => Ok(geo_value(geo, parent_alias)),
            FieldSource::Constant(value) => Ok(literal_or_null(value)),
        }
    }

    fn join_value(
        &mut self,
        join: &Join,
        parent_alias: &str,
        parent_pk: Option<&ColumnName>,
    ) -> Result<String> {
        match &join.kind {
            // The parent row points at the target: correlate the target's
            // primary key with the parent's own column. No parent pk needed.
            JoinKind::BelongsTo { column } => {
                let target = &join.table;
                let target_pk = self.pk_of(target)?;
                let alias = self.alias();
                let object = self.object(&join.fields, &alias, Some(&target_pk))?;
                let filters = self.filters(join.filters.as_deref(), &alias, target)?;
                Ok(format!(
                    "(SELECT {object} FROM {} AS {} WHERE {} = {}{filters} LIMIT 1)",
                    qtable(self.db, target),
                    qident(&alias),
                    qcol(&alias, &target_pk),
                    qcol(parent_alias, column),
                ))
            }
            JoinKind::HasOne { foreign_key } => {
                let parent_pk = require_pk(parent_pk)?;
                let child = &join.table;
                let child_pk = self.pk_of(child)?;
                let alias = self.alias();
                let object = self.object(&join.fields, &alias, Some(&child_pk))?;
                let filters = self.filters(join.filters.as_deref(), &alias, child)?;
                let order = order_clause(join.order_by.as_deref(), &alias);
                Ok(format!(
                    "(SELECT {object} FROM {} AS {} WHERE {} = {}{filters}{order} LIMIT 1)",
                    qtable(self.db, child),
                    qident(&alias),
                    qcol(&alias, foreign_key),
                    qcol(parent_alias, parent_pk),
                ))
            }
            JoinKind::HasMany { foreign_key } => {
                let parent_pk = require_pk(parent_pk)?;
                let child = &join.table;
                let child_pk = self.pk_of(child)?;
                let derived = self.alias();
                let object = self.object(&join.fields, &derived, Some(&child_pk))?;
                let inner = self.alias();
                let filters = self.filters(join.filters.as_deref(), &inner, child)?;
                let inner_sql = format!(
                    "SELECT {ia}.* FROM {} AS {ia} WHERE {} = {}{filters}{}{}",
                    qtable(self.db, child),
                    qcol(&inner, foreign_key),
                    qcol(parent_alias, parent_pk),
                    order_clause(join.order_by.as_deref(), &inner),
                    limit_clause(join.limit),
                    ia = qident(&inner),
                );
                Ok(json_agg_subquery(
                    &object,
                    &inner_sql,
                    &derived,
                    order_clause(join.order_by.as_deref(), &derived),
                ))
            }
            JoinKind::ManyToMany { through } => {
                let parent_pk = require_pk(parent_pk)?;
                let far = &join.table;
                let far_pk = self.pk_of(far)?;
                let derived = self.alias();
                let object = self.object(&join.fields, &derived, Some(&far_pk))?;
                let far_alias = self.alias();
                let junction_alias = self.alias();
                let filters = self.filters(join.filters.as_deref(), &far_alias, far)?;
                let inner_sql = format!(
                    "SELECT {fa}.* FROM {} AS {fa} JOIN {} AS {ja} ON {} = {} WHERE {} = {}{filters}{}{}",
                    qtable(self.db, far),
                    qtable(self.db, &through.table),
                    qcol(&junction_alias, &through.right_key),
                    qcol(&far_alias, &far_pk),
                    qcol(&junction_alias, &through.left_key),
                    qcol(parent_alias, parent_pk),
                    order_clause(join.order_by.as_deref(), &far_alias),
                    limit_clause(join.limit),
                    fa = qident(&far_alias),
                    ja = qident(&junction_alias),
                );
                Ok(json_agg_subquery(
                    &object,
                    &inner_sql,
                    &derived,
                    order_clause(join.order_by.as_deref(), &derived),
                ))
            }
        }
    }

    fn aggregate_value(
        &mut self,
        aggregate: &Aggregate,
        parent_alias: &str,
        parent_pk: Option<&ColumnName>,
    ) -> Result<String> {
        let parent_pk = require_pk(parent_pk)?;
        match &aggregate.key {
            AggregateKey::Direct(foreign_key) => {
                let alias = self.alias();
                let function = agg_function(&aggregate.op, &alias);
                let filters =
                    self.filters(aggregate.filters.as_deref(), &alias, &aggregate.table)?;
                Ok(format!(
                    "(SELECT {function} FROM {} AS {} WHERE {} = {}{filters})",
                    qtable(self.db, &aggregate.table),
                    qident(&alias),
                    qcol(&alias, foreign_key),
                    qcol(parent_alias, parent_pk),
                ))
            }
            AggregateKey::Through(through) => {
                let far_pk = self.pk_of(&aggregate.table)?;
                let alias = self.alias();
                let junction_alias = self.alias();
                let function = agg_function(&aggregate.op, &alias);
                let filters =
                    self.filters(aggregate.filters.as_deref(), &alias, &aggregate.table)?;
                Ok(format!(
                    "(SELECT {function} FROM {} AS {fa} JOIN {} AS {ja} ON {} = {} WHERE {} = {}{filters})",
                    qtable(self.db, &aggregate.table),
                    qtable(self.db, &through.table),
                    qcol(&junction_alias, &through.right_key),
                    qcol(&alias, &far_pk),
                    qcol(&junction_alias, &through.left_key),
                    qcol(parent_alias, parent_pk),
                    fa = qident(&alias),
                    ja = qident(&junction_alias),
                ))
            }
        }
    }

    /// `… AND (cond)` for each filter, qualified to `alias`; `table` names the
    /// relation being filtered, so operands cast to their column's real type.
    fn filters(
        &mut self,
        filters: Option<&[Filter]>,
        alias: &str,
        table: &TableName,
    ) -> Result<String> {
        let mut out = String::new();
        if let Some(filters) = filters {
            for filter in filters {
                let condition = self.filter(filter, alias, table)?;
                out.push_str(" AND (");
                out.push_str(&condition);
                out.push(')');
            }
        }
        Ok(out)
    }

    fn filter(&mut self, filter: &Filter, alias: &str, table: &TableName) -> Result<String> {
        match filter {
            Filter::Raw(raw) => Ok(raw.raw.as_ref().to_owned()),
            Filter::NullCheck(check) => Ok(format!(
                "{} IS {}",
                qcol(alias, &check.column),
                match check.op {
                    NullOp::IsNull => "NULL",
                    NullOp::IsNotNull => "NOT NULL",
                },
            )),
            Filter::ValueOp(op) => self.value_op(op, alias, table),
        }
    }

    fn value_op(
        &mut self,
        filter: &ValueOpFilter,
        alias: &str,
        table: &TableName,
    ) -> Result<String> {
        let column = qcol(alias, &filter.column);
        let target = &filter.column;
        let expr = match (&filter.op, &filter.value) {
            (FilterOp::Eq, FilterValue::Single(v)) => {
                format!("{column} = {}", self.typed_param(v, table, target)?)
            }
            (FilterOp::Neq, FilterValue::Single(v)) => {
                format!("{column} <> {}", self.typed_param(v, table, target)?)
            }
            (FilterOp::Lt, FilterValue::Single(v)) => {
                format!("{column} < {}", self.typed_param(v, table, target)?)
            }
            (FilterOp::Lte, FilterValue::Single(v)) => {
                format!("{column} <= {}", self.typed_param(v, table, target)?)
            }
            (FilterOp::Gt, FilterValue::Single(v)) => {
                format!("{column} > {}", self.typed_param(v, table, target)?)
            }
            (FilterOp::Gte, FilterValue::Single(v)) => {
                format!("{column} >= {}", self.typed_param(v, table, target)?)
            }
            (FilterOp::Like, FilterValue::Single(v)) => {
                format!("{column} LIKE {}", self.typed_param(v, table, target)?)
            }
            (FilterOp::Ilike, FilterValue::Single(v)) => {
                format!("{column} ILIKE {}", self.typed_param(v, table, target)?)
            }
            (FilterOp::In, FilterValue::List(vs)) => {
                format!("{column} IN ({})", self.typed_params(vs, table, target)?)
            }
            (FilterOp::NotIn, FilterValue::List(vs)) => {
                format!(
                    "{column} NOT IN ({})",
                    self.typed_params(vs, table, target)?
                )
            }
            (FilterOp::Between, FilterValue::Range(lo, hi)) => format!(
                "{column} BETWEEN {} AND {}",
                self.typed_param(lo, table, target)?,
                self.typed_param(hi, table, target)?,
            ),
            (op, _) => {
                return Err(SourceError::Query(format!(
                    "filter operator {op:?} does not match its value's arity"
                )));
            }
        };
        Ok(expr)
    }

    /// A bound operand cast to its column's SQL type — `$n::<type>` — so a
    /// `numeric` column compares numerically, a `date` as a date, and so on,
    /// rather than everything degrading to text. The type was resolved from the
    /// catalog before query building (see
    /// [`PgDocumentBuilder::column_type`](super::PgDocumentBuilder::column_type)).
    fn typed_param(
        &mut self,
        value: &str,
        table: &TableName,
        column: &ColumnName,
    ) -> Result<String> {
        let sql_type = self
            .col_types
            .get(&(table.to_string(), column.to_string()))
            .ok_or_else(|| {
                SourceError::Query(format!("internal: missing type for `{table}.{column}`"))
            })?
            .clone();
        let placeholder = self.placeholder(GenericValue::String(value.to_owned()))?;
        Ok(format!("{placeholder}::{sql_type}"))
    }

    fn typed_params(
        &mut self,
        values: &[String],
        table: &TableName,
        column: &ColumnName,
    ) -> Result<String> {
        let mut placeholders = Vec::with_capacity(values.len());
        for value in values {
            placeholders.push(self.typed_param(value, table, column)?);
        }
        Ok(placeholders.join(", "))
    }

    /// A boolean SQL expression that is true when the root row is soft-deleted:
    /// the marker is truthy (true boolean or any present value) and the optional
    /// `when` filters match. Generic over the marker's type via `pg_typeof`.
    fn soft_delete_predicate(&mut self, schema: &IndexSchema) -> Result<Option<String>> {
        let (column, when) = match &schema.soft_delete {
            None => return Ok(None),
            Some(SoftDelete::Column(c)) => (c.column.clone(), c.when.as_deref()),
            Some(SoftDelete::Field(f)) => match field_column(&schema.fields, &f.field) {
                Some(column) => (column.clone(), f.when.as_deref()),
                None => return Ok(None),
            },
        };
        let marker = qcol(ROOT, &column);
        // The cast goes through `text` so the expression plans for any marker
        // type (timestamp, text, …); Postgres type-checks every CASE branch up
        // front, and a direct `{marker}::boolean` would be rejected at plan
        // time for a non-boolean column even though the guard makes that branch
        // unreachable at runtime.
        let truthy = format!(
            "CASE WHEN {marker} IS NULL THEN false \
             WHEN pg_typeof({marker}) = 'boolean'::regtype THEN {marker}::text::boolean \
             ELSE true END"
        );
        let when_sql = self.filters(when, ROOT, &schema.table)?;
        Ok(Some(format!("({truthy}){when_sql}")))
    }
}

fn require_pk(parent_pk: Option<&ColumnName>) -> Result<&ColumnName> {
    parent_pk.ok_or_else(|| {
        SourceError::Unsupported(
            "relations require the parent table to declare a primary key".into(),
        )
    })
}

/// Wrap a per-row `object` over a filtered/ordered/limited `inner` subquery in a
/// `json_agg`, aliasing the derived rows as `derived` (which `object` and its
/// nested subqueries reference).
fn json_agg_subquery(object: &str, inner: &str, derived: &str, agg_order: String) -> String {
    format!(
        "(SELECT coalesce(json_agg({object}{agg_order}), '[]'::json) FROM ({inner}) AS {})",
        qident(derived),
    )
}

/// A column value expression: the qualified column, wrapped by transforms and a
/// default. Defaults render as unknown-typed literals so Postgres adapts them
/// to the column's type.
fn column_value(
    column: &ColumnName,
    transforms: &[Transform],
    default: Option<&GenericValue>,
    alias: &str,
) -> String {
    let mut expr = qcol(alias, column);
    for transform in transforms {
        expr = match transform {
            Transform::Lowercase => format!("lower({expr})"),
            Transform::Trim => format!("trim({expr})"),
        };
    }
    if let Some(literal) = default.and_then(scalar_literal) {
        expr = format!("coalesce({expr}, {literal})");
    }
    expr
}

/// A `geo_point` value: `{ "lat": …, "lon": … }`, or SQL `NULL` when either
/// coordinate is null — so a missing point is absent rather than an invalid
/// `{lat: null, lon: null}` OpenSearch would reject.
fn geo_value(geo: &Geo, alias: &str) -> String {
    let lat = qcol(alias, &geo.lat);
    let lon = qcol(alias, &geo.lon);
    format!(
        "CASE WHEN {lat} IS NULL OR {lon} IS NULL THEN NULL \
         ELSE json_build_object('lat', {lat}, 'lon', {lon}) END"
    )
}

fn scalar_literal(value: &GenericValue) -> Option<String> {
    let text = match value {
        GenericValue::Bool(b) => b.to_string(),
        GenericValue::Int(i) => i.to_string(),
        GenericValue::Decimal(d) => d.to_string(),
        GenericValue::String(s) => s.clone(),
        GenericValue::Null | GenericValue::Array(_) | GenericValue::Map(_) => return None,
    };
    Some(sql_string(&text))
}

/// A scalar value as a SQL literal, or `null` for a null or non-scalar value.
fn literal_or_null(value: &GenericValue) -> String {
    scalar_literal(value).unwrap_or_else(|| "null".to_owned())
}

fn json_key(name: &str) -> String {
    sql_string(name)
}

fn sql_string(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

fn qident(ident: &str) -> String {
    format!("\"{ident}\"")
}

fn qcol(alias: &str, column: &ColumnName) -> String {
    format!("\"{alias}\".\"{column}\"")
}

fn qtable(db: &DatabaseSchema, table: &TableName) -> String {
    format!("\"{db}\".\"{table}\"")
}

fn order_clause(order_by: Option<&[OrderBy]>, alias: &str) -> String {
    let Some(order_by) = order_by else {
        return String::new();
    };
    if order_by.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = order_by
        .iter()
        .map(|ob| format!("{} {}", qcol(alias, &ob.column), direction(ob.direction)))
        .collect();
    format!(" ORDER BY {}", parts.join(", "))
}

fn direction(direction: Option<Direction>) -> &'static str {
    match direction {
        Some(Direction::Desc) => "DESC",
        _ => "ASC",
    }
}

fn limit_clause(limit: Option<u64>) -> String {
    limit.map(|n| format!(" LIMIT {n}")).unwrap_or_default()
}

fn agg_function(op: &AggregateOp, alias: &str) -> String {
    match op {
        AggregateOp::Count => "count(*)".to_owned(),
        AggregateOp::Sum(c) => format!("sum({})", qcol(alias, c)),
        AggregateOp::Avg(c) => format!("avg({})", qcol(alias, c)),
        AggregateOp::Min(c) => format!("min({})", qcol(alias, c)),
        AggregateOp::Max(c) => format!("max({})", qcol(alias, c)),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use schema_core::{IndexSchema, OrderBy, SoftDeleteColumn};

    fn db() -> DatabaseSchema {
        DatabaseSchema::try_new("public").unwrap()
    }
    fn t(n: &str) -> TableName {
        TableName::try_new(n).unwrap()
    }
    fn c(n: &str) -> ColumnName {
        ColumnName::try_new(n).unwrap()
    }
    fn f(n: &str) -> schema_core::FieldName {
        schema_core::FieldName::try_new(n).unwrap()
    }
    fn col_field(name: &str, column: &str) -> Field {
        Field {
            field: f(name),
            options: Default::default(),
            source: FieldSource::Column(schema_core::Column {
                column: c(column),
                ty: schema_core::FlussoType::Keyword,
                nullable: true,
                transforms: Vec::new(),
                default: None,
            }),
        }
    }
    fn index(
        primary_key: Option<&str>,
        soft_delete: Option<SoftDelete>,
        fields: Vec<Field>,
    ) -> IndexSchema {
        IndexSchema {
            version: 1,
            table: t("users"),
            db_schema: db(),
            primary_key: primary_key.map(c),
            doc_id: None,
            soft_delete,
            filters: None,
            fields,
        }
    }

    #[test]
    fn columns_only() {
        let schema = index(
            Some("id"),
            None,
            vec![col_field("id", "id"), col_field("email", "email")],
        );
        let (sql, params) = document_query(
            &schema,
            &[(c("id"), GenericValue::Int(7))],
            &HashMap::new(),
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(
            sql.as_str(),
            r#"SELECT json_build_object('id', "root"."id", 'email', "root"."email") AS "document" FROM "public"."users" AS "root" WHERE "root"."id" = $1"#
        );
        assert_eq!(params, vec![GenericValue::Int(7)]);
    }

    #[test]
    fn root_filters_fold_into_both_query_forms() {
        let mut schema = index(Some("id"), None, vec![col_field("id", "id")]);
        schema.filters = Some(vec![Filter::ValueOp(ValueOpFilter {
            column: c("status"),
            op: FilterOp::Eq,
            value: FilterValue::Single("active".to_owned()),
        })]);
        let mut col_types = HashMap::new();
        col_types.insert(("users".to_owned(), "status".to_owned()), "text".to_owned());

        let (sql, params) = document_query(
            &schema,
            &[(c("id"), GenericValue::Int(7))],
            &HashMap::new(),
            &col_types,
        )
        .unwrap();
        assert_eq!(
            sql.as_str(),
            r#"SELECT json_build_object('id', "root"."id") AS "document" FROM "public"."users" AS "root" WHERE "root"."id" = $1 AND ("root"."status" = $2::text)"#
        );
        assert_eq!(
            params,
            vec![
                GenericValue::Int(7),
                GenericValue::String("active".to_owned())
            ]
        );

        let (sql, _) = documents_query(
            &schema,
            &c("id"),
            &[GenericValue::Int(7)],
            &HashMap::new(),
            &col_types,
        )
        .unwrap();
        assert!(
            sql.as_str()
                .ends_with(r#"WHERE "root"."id" IN ($1) AND ("root"."status" = $2::text)"#),
            "{}",
            sql.as_str()
        );
    }

    #[test]
    fn has_many_with_order_and_limit() {
        let orders = Field {
            field: f("orders"),
            options: Default::default(),
            source: FieldSource::Relation(Relation::Join(Join {
                table: t("orders"),
                kind: JoinKind::HasMany {
                    foreign_key: c("user_id"),
                },
                primary_key: c("primary_key"),
                filters: None,
                order_by: Some(vec![OrderBy {
                    column: c("created_at"),
                    direction: Some(Direction::Desc),
                }]),
                limit: Some(5),
                fields: vec![col_field("id", "id"), col_field("total", "total")],
            })),
        };
        let schema = index(Some("id"), None, vec![orders]);
        let mut pks = HashMap::new();
        pks.insert("orders".to_owned(), c("id"));
        let (sql, _) = document_query(
            &schema,
            &[(c("id"), GenericValue::Int(1))],
            &pks,
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(
            sql.as_str(),
            r#"SELECT json_build_object('orders', (SELECT coalesce(json_agg(json_build_object('id', "rel1"."id", 'total', "rel1"."total") ORDER BY "rel1"."created_at" DESC), '[]'::json) FROM (SELECT "rel2".* FROM "public"."orders" AS "rel2" WHERE "rel2"."user_id" = "root"."id" ORDER BY "rel2"."created_at" DESC LIMIT 5) AS "rel1")) AS "document" FROM "public"."users" AS "root" WHERE "root"."id" = $1"#
        );
    }

    #[test]
    fn belongs_to_correlates_on_the_parent_column() {
        let org = Field {
            field: f("org"),
            options: Default::default(),
            source: FieldSource::Relation(Relation::Join(Join {
                table: t("orgs"),
                kind: JoinKind::BelongsTo {
                    column: c("org_id"),
                },
                primary_key: c("id"),
                filters: None,
                order_by: None,
                limit: None,
                fields: vec![col_field("name", "name")],
            })),
        };
        let schema = index(Some("id"), None, vec![org]);
        let mut pks = HashMap::new();
        pks.insert("orgs".to_owned(), c("id"));
        let (sql, _) = document_query(
            &schema,
            &[(c("id"), GenericValue::Int(1))],
            &pks,
            &HashMap::new(),
        )
        .unwrap();
        // The target is matched by ITS primary key against the parent's own
        // column — the reverse of a has_one — and needs no parent primary key.
        assert_eq!(
            sql.as_str(),
            r#"SELECT json_build_object('org', (SELECT json_build_object('name', "rel1"."name") FROM "public"."orgs" AS "rel1" WHERE "rel1"."id" = "root"."org_id" LIMIT 1)) AS "document" FROM "public"."users" AS "root" WHERE "root"."id" = $1"#
        );
    }

    #[test]
    fn aggregate_count() {
        let count = Field {
            field: f("order_count"),
            options: Default::default(),
            source: FieldSource::Relation(Relation::Aggregate(Aggregate {
                table: t("orders"),
                op: AggregateOp::Count,
                key: AggregateKey::Direct(c("user_id")),
                value_type: None,
                filters: None,
            })),
        };
        let schema = index(Some("id"), None, vec![count]);
        let (sql, _) = document_query(
            &schema,
            &[(c("id"), GenericValue::Int(1))],
            &HashMap::new(),
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(
            sql.as_str(),
            r#"SELECT json_build_object('order_count', (SELECT count(*) FROM "public"."orders" AS "rel1" WHERE "rel1"."user_id" = "root"."id")) AS "document" FROM "public"."users" AS "root" WHERE "root"."id" = $1"#
        );
    }

    #[test]
    fn soft_delete_folds_into_where() {
        let schema = index(
            Some("id"),
            Some(SoftDelete::Column(SoftDeleteColumn {
                column: c("deleted_at"),
                when: None,
            })),
            vec![col_field("id", "id")],
        );
        let (sql, _) = document_query(
            &schema,
            &[(c("id"), GenericValue::Int(1))],
            &HashMap::new(),
            &HashMap::new(),
        )
        .unwrap();
        assert!(sql.as_str().contains(
            r#"WHERE "root"."id" = $1 AND NOT ((CASE WHEN "root"."deleted_at" IS NULL THEN false WHEN pg_typeof("root"."deleted_at") = 'boolean'::regtype THEN "root"."deleted_at"::text::boolean ELSE true END))"#
        ));
    }

    #[test]
    fn documents_query_keys_with_in_and_selects_the_key() {
        let schema = index(
            Some("id"),
            None,
            vec![col_field("id", "id"), col_field("email", "email")],
        );
        let (sql, params) = documents_query(
            &schema,
            &c("id"),
            &[GenericValue::Int(7), GenericValue::Int(9)],
            &HashMap::new(),
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(
            sql.as_str(),
            r#"SELECT to_json("root"."id") AS "doc_key", json_build_object('id', "root"."id", 'email', "root"."email") AS "document" FROM "public"."users" AS "root" WHERE "root"."id" IN ($1, $2)"#
        );
        assert_eq!(params, vec![GenericValue::Int(7), GenericValue::Int(9)]);
    }

    #[test]
    fn documents_query_folds_soft_delete_into_where() {
        let schema = index(
            Some("id"),
            Some(SoftDelete::Column(SoftDeleteColumn {
                column: c("deleted_at"),
                when: None,
            })),
            vec![col_field("id", "id")],
        );
        let (sql, _) = documents_query(
            &schema,
            &c("id"),
            &[GenericValue::Int(1)],
            &HashMap::new(),
            &HashMap::new(),
        )
        .unwrap();
        assert!(sql.as_str().contains(
            r#"WHERE "root"."id" IN ($1) AND NOT ((CASE WHEN "root"."deleted_at" IS NULL THEN false WHEN pg_typeof("root"."deleted_at") = 'boolean'::regtype THEN "root"."deleted_at"::text::boolean ELSE true END))"#
        ));
    }

    #[test]
    fn reverse_query_selects_foreign_key() {
        let (sql, params) = reverse_query(
            &db(),
            &t("orders"),
            &c("user_id"),
            &[(c("id"), GenericValue::Int(9))],
        )
        .unwrap();
        assert_eq!(
            sql.as_str(),
            r#"SELECT "user_id" FROM "public"."orders" WHERE "id" = $1"#
        );
        assert_eq!(params, vec![GenericValue::Int(9)]);
    }
}

/// Property tests over the query builder. The join/aggregate/filter/geo
/// generation is intricate and the example tests above cover only fixed shapes;
/// these feed it randomly-generated schemas — arbitrary nestings of joins,
/// aggregates, groups, geo points, and filters — and assert structural
/// invariants that must hold for *any* well-formed schema:
///
/// - the builder never panics (a panic fails the test);
/// - parentheses balance (the nested subqueries open and close cleanly);
/// - the `$n` placeholders are exactly `1..=params.len()`, contiguous and in
///   range — so every bound parameter is referenced and none dangles;
/// - double-quotes balance (every quoted identifier is closed).
///
/// The identifier universe and the `pks`/`col_types` maps are fixed and
/// complete, so the builder's lookups always resolve — failures come from the
/// SQL it assembles, not from missing metadata. Generated string values are
/// restricted to a quote/paren-free alphabet so they can't forge the structural
/// markers the invariants check.
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod proptests {
    use super::*;
    use proptest::prelude::*;
    use schema_core::{
        Aggregate, AggregateKey, AggregateOp, Column, FieldName, FlussoType, Geo, Join, JoinKind,
        NullCheckFilter, Relation, SoftDeleteColumn, Through,
    };

    // A small, fixed identifier universe so every generated name is valid and
    // reused across tables, and the metadata maps below can cover all of it.
    const TABLES: &[&str] = &["users", "orders", "orgs", "items"];
    const COLUMNS: &[&str] = &[
        "id",
        "name",
        "email",
        "total",
        "status",
        "user_id",
        "org_id",
        "created_at",
    ];
    const FIELDS: &[&str] = &["a", "b", "c", "d", "e", "f", "g"];

    fn pk_map() -> HashMap<String, ColumnName> {
        TABLES
            .iter()
            .map(|t| ((*t).to_owned(), ColumnName::try_new("id").unwrap()))
            .collect()
    }

    fn col_type_map() -> HashMap<(String, String), String> {
        let mut m = HashMap::new();
        for table in TABLES {
            for column in COLUMNS {
                m.insert(
                    ((*table).to_owned(), (*column).to_owned()),
                    "text".to_owned(),
                );
            }
        }
        m
    }

    fn db_schema() -> DatabaseSchema {
        DatabaseSchema::try_new("public").unwrap()
    }

    fn table() -> impl Strategy<Value = TableName> {
        prop::sample::select(TABLES.to_vec()).prop_map(|s| TableName::try_new(s).unwrap())
    }
    fn column() -> impl Strategy<Value = ColumnName> {
        prop::sample::select(COLUMNS.to_vec()).prop_map(|s| ColumnName::try_new(s).unwrap())
    }
    fn field_name() -> impl Strategy<Value = FieldName> {
        prop::sample::select(FIELDS.to_vec()).prop_map(|s| FieldName::try_new(s).unwrap())
    }

    /// Scalar string values restricted to a quote/paren/`$`-free alphabet, so a
    /// generated literal can't forge the structural markers the invariants check.
    fn safe_string() -> impl Strategy<Value = String> {
        prop::collection::vec(
            prop_oneof![
                prop::char::range('a', 'z'),
                prop::char::range('0', '9'),
                Just(' '),
            ],
            0..6,
        )
        .prop_map(|cs| cs.into_iter().collect())
    }

    fn scalar_value() -> impl Strategy<Value = GenericValue> {
        prop_oneof![
            any::<i64>().prop_map(GenericValue::Int),
            any::<bool>().prop_map(GenericValue::Bool),
            safe_string().prop_map(GenericValue::String),
            Just(GenericValue::Null),
        ]
    }

    fn value_op_filter() -> impl Strategy<Value = ValueOpFilter> {
        let single = (
            column(),
            prop_oneof![
                Just(FilterOp::Eq),
                Just(FilterOp::Neq),
                Just(FilterOp::Lt),
                Just(FilterOp::Lte),
                Just(FilterOp::Gt),
                Just(FilterOp::Gte),
                Just(FilterOp::Like),
                Just(FilterOp::Ilike),
            ],
            safe_string(),
        )
            .prop_map(|(column, op, v)| ValueOpFilter {
                column,
                op,
                value: FilterValue::Single(v),
            });
        let list = (
            column(),
            prop_oneof![Just(FilterOp::In), Just(FilterOp::NotIn)],
            prop::collection::vec(safe_string(), 1..4),
        )
            .prop_map(|(column, op, vs)| ValueOpFilter {
                column,
                op,
                value: FilterValue::List(vs),
            });
        let between =
            (column(), safe_string(), safe_string()).prop_map(|(column, lo, hi)| ValueOpFilter {
                column,
                op: FilterOp::Between,
                value: FilterValue::Range(lo, hi),
            });
        // No `Filter::Raw` — its SQL is passed through verbatim and would
        // legitimately defeat the paren/quote invariants; the builder doesn't
        // shape it, so it isn't what these tests are checking.
        prop_oneof![single, list, between]
    }

    fn filter() -> impl Strategy<Value = Filter> {
        prop_oneof![
            (
                column(),
                prop_oneof![Just(NullOp::IsNull), Just(NullOp::IsNotNull)]
            )
                .prop_map(|(column, op)| Filter::NullCheck(NullCheckFilter { column, op })),
            value_op_filter().prop_map(Filter::ValueOp),
        ]
    }

    fn filters_opt() -> impl Strategy<Value = Option<Vec<Filter>>> {
        prop::option::of(prop::collection::vec(filter(), 0..3))
    }

    fn order_by_opt() -> impl Strategy<Value = Option<Vec<OrderBy>>> {
        prop::option::of(prop::collection::vec(
            (
                column(),
                prop::option::of(prop_oneof![Just(Direction::Asc), Just(Direction::Desc)]),
            )
                .prop_map(|(column, direction)| OrderBy { column, direction }),
            0..3,
        ))
    }

    fn through() -> impl Strategy<Value = Through> {
        (table(), column(), column()).prop_map(|(table, left_key, right_key)| Through {
            table,
            left_key,
            right_key,
        })
    }

    fn join_kind() -> impl Strategy<Value = JoinKind> {
        prop_oneof![
            column().prop_map(|column| JoinKind::BelongsTo { column }),
            column().prop_map(|foreign_key| JoinKind::HasOne { foreign_key }),
            column().prop_map(|foreign_key| JoinKind::HasMany { foreign_key }),
            through().prop_map(|through| JoinKind::ManyToMany { through }),
        ]
    }

    fn aggregate() -> impl Strategy<Value = Aggregate> {
        (
            table(),
            prop_oneof![
                Just(AggregateOp::Count),
                column().prop_map(AggregateOp::Sum),
                column().prop_map(AggregateOp::Avg),
                column().prop_map(AggregateOp::Min),
                column().prop_map(AggregateOp::Max),
            ],
            prop_oneof![
                column().prop_map(AggregateKey::Direct),
                through().prop_map(AggregateKey::Through),
            ],
            filters_opt(),
        )
            .prop_map(|(table, op, key, filters)| Aggregate {
                table,
                op,
                key,
                value_type: None,
                filters,
            })
    }

    /// A recursive `FieldSource`: leaves (column, geo, constant, aggregate) plus
    /// nesting via `Group` and `Join`, both of which carry child fields.
    fn field_source() -> impl Strategy<Value = FieldSource> {
        let leaf = prop_oneof![
            (
                column(),
                prop::collection::vec(
                    prop_oneof![Just(Transform::Lowercase), Just(Transform::Trim)],
                    0..3
                ),
                prop::option::of(scalar_value()),
            )
                .prop_map(|(column, transforms, default)| FieldSource::Column(
                    Column {
                        column,
                        ty: FlussoType::Keyword,
                        nullable: true,
                        transforms,
                        default,
                    }
                )),
            (column(), column()).prop_map(|(lat, lon)| FieldSource::Geo(Geo {
                lat,
                lon,
                nullable: true
            })),
            scalar_value().prop_map(FieldSource::Constant),
            aggregate().prop_map(|a| FieldSource::Relation(Relation::Aggregate(a))),
        ];
        leaf.prop_recursive(3, 32, 4, |inner| {
            // `inner` (a BoxedStrategy) is Clone, so build the child-fields
            // strategy on demand for each arm rather than cloning the VecStrategy.
            let children = || {
                let child = (field_name(), inner.clone()).prop_map(|(field, source)| Field {
                    field,
                    options: Default::default(),
                    source,
                });
                prop::collection::vec(child, 1..4)
            };
            prop_oneof![
                children().prop_map(FieldSource::Group),
                (
                    join_kind(),
                    table(),
                    column(),
                    filters_opt(),
                    order_by_opt(),
                    prop::option::of(any::<u64>()),
                    children(),
                )
                    .prop_map(
                        |(kind, table, primary_key, filters, order_by, limit, fields)| {
                            FieldSource::Relation(Relation::Join(Join {
                                table,
                                kind,
                                primary_key,
                                filters,
                                order_by,
                                limit,
                                fields,
                            }))
                        }
                    ),
            ]
        })
    }

    fn field() -> impl Strategy<Value = Field> {
        (field_name(), field_source()).prop_map(|(field, source)| Field {
            field,
            options: Default::default(),
            source,
        })
    }

    fn index_schema() -> impl Strategy<Value = IndexSchema> {
        (
            table(),
            prop::collection::vec(field(), 1..5),
            filters_opt(),
            prop::option::of(
                (column(), filters_opt()).prop_map(|(column, when)| {
                    SoftDelete::Column(SoftDeleteColumn { column, when })
                }),
            ),
        )
            .prop_map(|(table, fields, filters, soft_delete)| IndexSchema {
                version: 1,
                table,
                db_schema: db_schema(),
                primary_key: Some(ColumnName::try_new("id").unwrap()),
                doc_id: None,
                soft_delete,
                filters,
                fields,
            })
    }

    /// Parentheses balance, scanning left to right, never dipping below zero.
    fn parens_balanced(sql: &str) -> bool {
        let mut depth: i64 = 0;
        for ch in sql.chars() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth < 0 {
                        return false;
                    }
                }
                _ => {}
            }
        }
        depth == 0
    }

    /// The set of `$n` placeholders in `sql` is exactly `{1..=params_len}` —
    /// contiguous, none missing, none out of range.
    fn placeholders_match(sql: &str, params_len: usize) -> bool {
        let mut found = std::collections::BTreeSet::new();
        let bytes = sql.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'$' {
                let start = i + 1;
                let mut j = start;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                if j > start {
                    if let Ok(n) = sql[start..j].parse::<usize>() {
                        found.insert(n);
                    }
                    i = j;
                    continue;
                }
            }
            i += 1;
        }
        let expected: std::collections::BTreeSet<usize> = (1..=params_len).collect();
        found == expected
    }

    fn assert_valid(sql: &str, params_len: usize) -> std::result::Result<(), TestCaseError> {
        prop_assert!(parens_balanced(sql), "unbalanced parens: {sql}");
        prop_assert!(
            placeholders_match(sql, params_len),
            "placeholders not 1..={params_len}: {sql}"
        );
        prop_assert!(
            sql.matches('"').count().is_multiple_of(2),
            "unbalanced double-quotes: {sql}"
        );
        Ok(())
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        #[test]
        fn document_query_is_structurally_valid(schema in index_schema()) {
            let key = [(ColumnName::try_new("id").unwrap(), GenericValue::Int(1))];
            let result = document_query(&schema, &key, &pk_map(), &col_type_map());
            prop_assert!(result.is_ok(), "builder errored: {:?}", result.err());
            let (sql, params) = result.unwrap();
            assert_valid(sql.as_str(), params.len())?;
        }

        #[test]
        fn documents_query_is_structurally_valid(schema in index_schema()) {
            let pk = ColumnName::try_new("id").unwrap();
            let keys = [GenericValue::Int(1), GenericValue::Int(2)];
            let result = documents_query(&schema, &pk, &keys, &pk_map(), &col_type_map());
            prop_assert!(result.is_ok(), "builder errored: {:?}", result.err());
            let (sql, params) = result.unwrap();
            assert_valid(sql.as_str(), params.len())?;
        }

        #[test]
        fn reverse_query_is_structurally_valid(
            table in table(),
            select_column in column(),
            key_column in column(),
        ) {
            let key = [(key_column, GenericValue::Int(5))];
            let (sql, params) =
                reverse_query(&db_schema(), &table, &select_column, &key).unwrap();
            assert_valid(sql.as_str(), params.len())?;
        }
    }
}

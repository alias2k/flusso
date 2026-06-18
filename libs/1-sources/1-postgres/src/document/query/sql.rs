//! Pure SQL-string helpers: identifier/table quoting, value expressions
//! (columns with transforms/defaults, geo points), `json_agg` wrapping, and the
//! `ORDER BY` / `LIMIT` / aggregate-function fragments. All take already-validated
//! schema types and produce SQL text; the [`Builder`](super::builder::Builder)
//! and the entry queries assemble them.

use schema_core::{
    AggregateOp, ColumnName, DatabaseSchema, Direction, GenericValue, Geo, OrderBy, TableName,
    Transform,
};

/// Wrap a per-row `object` over a filtered/ordered/limited `inner` subquery in a
/// `json_agg`, aliasing the derived rows as `derived` (which `object` and its
/// nested subqueries reference).
pub(super) fn json_agg_subquery(
    object: &str,
    inner: &str,
    derived: &str,
    agg_order: String,
) -> String {
    format!(
        "(SELECT coalesce(json_agg({object}{agg_order}), '[]'::json) FROM ({inner}) AS {})",
        qident(derived),
    )
}

/// A column value expression: the qualified column, wrapped by transforms and a
/// default. Defaults render as unknown-typed literals so Postgres adapts them
/// to the column's type.
pub(super) fn column_value(
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
pub(super) fn geo_value(geo: &Geo, alias: &str) -> String {
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
pub(super) fn literal_or_null(value: &GenericValue) -> String {
    scalar_literal(value).unwrap_or_else(|| "null".to_owned())
}

pub(super) fn json_key(name: &str) -> String {
    sql_string(name)
}

fn sql_string(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

pub(super) fn qident(ident: &str) -> String {
    format!("\"{ident}\"")
}

pub(super) fn qcol(alias: &str, column: &ColumnName) -> String {
    format!("\"{alias}\".\"{column}\"")
}

pub(super) fn qtable(db: &DatabaseSchema, table: &TableName) -> String {
    format!("\"{db}\".\"{table}\"")
}

pub(super) fn order_clause(order_by: Option<&[OrderBy]>, alias: &str) -> String {
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

pub(super) fn limit_clause(limit: Option<u64>) -> String {
    limit.map(|n| format!(" LIMIT {n}")).unwrap_or_default()
}

/// The scalar aggregate expression for `op` over rows aliased `alias`. `ids` is
/// handled separately ([`ids_agg`]) because its emitted SQL shape varies by key
/// (direct vs through) — only the column-folding ops flow through here.
pub(super) fn agg_function(op: &AggregateOp, alias: &str) -> String {
    match op {
        AggregateOp::Count => "count(*)".to_owned(),
        AggregateOp::Sum(c) => format!("sum({})", qcol(alias, c)),
        AggregateOp::Avg(c) => format!("avg({})", qcol(alias, c)),
        AggregateOp::Min(c) => format!("min({})", qcol(alias, c)),
        AggregateOp::Max(c) => format!("max({})", qcol(alias, c)),
        // The `ids` arms emit through `ids_agg`; reaching here is a builder bug.
        AggregateOp::Ids { .. } => "null".to_owned(),
    }
}

/// `coalesce(json_agg(<alias>.<column>), '[]'::json)` — the flat array of a
/// related table's keys an `ids` aggregate collects, never null for an empty
/// relation.
pub(super) fn ids_agg(alias: &str, column: &ColumnName) -> String {
    format!("coalesce(json_agg({}), '[]'::json)", qcol(alias, column))
}

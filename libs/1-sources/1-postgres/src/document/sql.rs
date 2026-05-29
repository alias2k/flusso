//! sea-query builders for the queries the assembler runs.
//!
//! Every query is single-table — relations are resolved with their own
//! sub-queries rather than SQL joins, which keeps each statement and its row
//! decoding simple. Identifiers come from the (already validated) schema, and
//! all operands are bound as parameters.

use sea_query::extension::postgres::PgExpr;
use sea_query::{Alias, Expr, Func, Order, Query, SelectStatement, SimpleExpr};
use schema_core::{
    Aggregate, AggregateOp, ColumnName, DatabaseSchema, Direction, Filter, FilterOp, FilterValue,
    GenericValue, Join, NullOp, TableName, Through,
};
use sources_core::SourceError;

use super::value::to_sea_value;

fn col(name: &ColumnName) -> Alias {
    Alias::new(name.as_ref())
}

fn table_ref(schema: &DatabaseSchema, table: &TableName) -> (Alias, Alias) {
    (Alias::new(schema.as_ref()), Alias::new(table.as_ref()))
}

/// `WHERE` term for one key column = value.
fn key_eq(column: &ColumnName, value: &GenericValue) -> Result<SimpleExpr, SourceError> {
    Ok(Expr::col(col(column)).eq(Expr::val(to_sea_value(value)?)))
}

/// `SELECT <columns> FROM <schema>.<table> WHERE <key…>` — the root row.
pub(crate) fn root_select(
    schema: &DatabaseSchema,
    table: &TableName,
    columns: &[ColumnName],
    key: &[(ColumnName, GenericValue)],
) -> Result<SelectStatement, SourceError> {
    let mut query = Query::select();
    query.from(table_ref(schema, table));
    for c in columns {
        query.column(col(c));
    }
    for (column, value) in key {
        query.and_where(key_eq(column, value)?);
    }
    Ok(query)
}

/// `SELECT <foreign_key> FROM <schema>.<child> WHERE <child key…>` — the
/// reverse lookup that finds which root rows a changed child row belongs to.
pub(crate) fn reverse_select(
    schema: &DatabaseSchema,
    child: &TableName,
    foreign_key: &ColumnName,
    child_key: &[(ColumnName, GenericValue)],
) -> Result<SelectStatement, SourceError> {
    let mut query = Query::select();
    query.from(table_ref(schema, child));
    query.column(col(foreign_key));
    for (column, value) in child_key {
        query.and_where(key_eq(column, value)?);
    }
    Ok(query)
}

/// `SELECT <sub-columns> FROM <schema>.<join table> WHERE fk = <root pk>`,
/// plus the join's filters, ordering, and limit — the rows folded into a field.
pub(crate) fn join_select(
    schema: &DatabaseSchema,
    join: &Join,
    foreign_key: &ColumnName,
    sub_columns: &[ColumnName],
    root_pk: &GenericValue,
) -> Result<SelectStatement, SourceError> {
    let mut query = Query::select();
    query.from(table_ref(schema, &join.table));
    for c in sub_columns {
        query.column(col(c));
    }
    query.and_where(key_eq(foreign_key, root_pk)?);
    if let Some(filters) = &join.filters {
        apply_filters(&mut query, filters, None)?;
    }
    if let Some(order_by) = &join.order_by {
        for ob in order_by {
            query.order_by(col(&ob.column), order_of(ob.direction));
        }
    }
    if let Some(limit) = join.limit {
        query.limit(limit);
    }
    Ok(query)
}

/// `SELECT <agg>(…) FROM <schema>.<agg table> WHERE fk = <root pk>` plus the
/// aggregate's filters — a single scalar.
pub(crate) fn aggregate_select(
    schema: &DatabaseSchema,
    aggregate: &Aggregate,
    foreign_key: &ColumnName,
    root_pk: &GenericValue,
) -> Result<SelectStatement, SourceError> {
    let mut query = Query::select();
    query.from(table_ref(schema, &aggregate.table));
    let func = match &aggregate.op {
        AggregateOp::Count => Func::count(Expr::col(col(foreign_key))),
        AggregateOp::Sum(c) => Func::sum(Expr::col(col(c))),
        AggregateOp::Avg(c) => Func::avg(Expr::col(col(c))),
        AggregateOp::Min(c) => Func::min(Expr::col(col(c))),
        AggregateOp::Max(c) => Func::max(Expr::col(col(c))),
    };
    query.expr(func);
    query.and_where(key_eq(foreign_key, root_pk)?);
    if let Some(filters) = &aggregate.filters {
        apply_filters(&mut query, filters, None)?;
    }
    Ok(query)
}

/// `SELECT <far>.<sub-columns> FROM <schema>.<far> INNER JOIN <schema>.<junction>
/// ON junction.right_key = far.<far pk> WHERE junction.left_key = <root pk>`,
/// plus the far table's filters, ordering, and limit — a many-to-many relation
/// folded in through its junction table.
pub(crate) fn through_select(
    schema: &DatabaseSchema,
    join: &Join,
    through: &Through,
    far_primary_key: &ColumnName,
    sub_columns: &[ColumnName],
    root_pk: &GenericValue,
) -> Result<SelectStatement, SourceError> {
    let far = &join.table;
    let junction = &through.table;

    let mut query = Query::select();
    query.from(table_ref(schema, far));
    for c in sub_columns {
        query.column((Alias::new(far.as_ref()), Alias::new(c.as_ref())));
    }
    query.inner_join(
        table_ref(schema, junction),
        Expr::col((Alias::new(junction.as_ref()), Alias::new(through.right_key.as_ref())))
            .equals((Alias::new(far.as_ref()), Alias::new(far_primary_key.as_ref()))),
    );
    query.and_where(
        Expr::col((Alias::new(junction.as_ref()), Alias::new(through.left_key.as_ref())))
            .eq(Expr::val(to_sea_value(root_pk)?)),
    );
    if let Some(filters) = &join.filters {
        apply_filters(&mut query, filters, Some(far))?;
    }
    if let Some(order_by) = &join.order_by {
        for ob in order_by {
            query.order_by(
                (Alias::new(far.as_ref()), Alias::new(ob.column.as_ref())),
                order_of(ob.direction),
            );
        }
    }
    if let Some(limit) = join.limit {
        query.limit(limit);
    }
    Ok(query)
}

/// Like [`aggregate_select`], but the aggregated table is reached through a
/// junction: `SELECT <agg>(far.…) FROM far INNER JOIN junction
/// ON junction.right_key = far.<far pk> WHERE junction.left_key = <root pk>`.
pub(crate) fn through_aggregate_select(
    schema: &DatabaseSchema,
    aggregate: &Aggregate,
    through: &Through,
    far_primary_key: &ColumnName,
    root_pk: &GenericValue,
) -> Result<SelectStatement, SourceError> {
    let far = &aggregate.table;
    let junction = &through.table;

    let mut query = Query::select();
    query.from(table_ref(schema, far));
    let func = match &aggregate.op {
        AggregateOp::Count => Func::count(qualified_col(Some(far), far_primary_key)),
        AggregateOp::Sum(c) => Func::sum(qualified_col(Some(far), c)),
        AggregateOp::Avg(c) => Func::avg(qualified_col(Some(far), c)),
        AggregateOp::Min(c) => Func::min(qualified_col(Some(far), c)),
        AggregateOp::Max(c) => Func::max(qualified_col(Some(far), c)),
    };
    query.expr(func);
    query.inner_join(
        table_ref(schema, junction),
        Expr::col((Alias::new(junction.as_ref()), Alias::new(through.right_key.as_ref())))
            .equals((Alias::new(far.as_ref()), Alias::new(far_primary_key.as_ref()))),
    );
    query.and_where(
        Expr::col((Alias::new(junction.as_ref()), Alias::new(through.left_key.as_ref())))
            .eq(Expr::val(to_sea_value(root_pk)?)),
    );
    if let Some(filters) = &aggregate.filters {
        apply_filters(&mut query, filters, Some(far))?;
    }
    Ok(query)
}

/// Apply filters, optionally qualifying each column with a table (needed when
/// the statement joins more than one table, as in [`through_select`]).
fn apply_filters(
    query: &mut SelectStatement,
    filters: &[Filter],
    qualifier: Option<&TableName>,
) -> Result<(), SourceError> {
    for filter in filters {
        query.and_where(filter_expr(filter, qualifier)?);
    }
    Ok(())
}

fn filter_expr(filter: &Filter, qualifier: Option<&TableName>) -> Result<SimpleExpr, SourceError> {
    match filter {
        Filter::Raw(raw) => Ok(Expr::cust(raw.raw.as_ref())),
        Filter::NullCheck(check) => {
            let column = qualified_col(qualifier, &check.column);
            Ok(match check.op {
                NullOp::IsNull => column.is_null(),
                NullOp::IsNotNull => column.is_not_null(),
            })
        }
        Filter::ValueOp(op) => value_op_expr(op, qualifier),
    }
}

/// A column expression, optionally qualified with its table.
fn qualified_col(qualifier: Option<&TableName>, column: &ColumnName) -> Expr {
    match qualifier {
        Some(table) => Expr::col((Alias::new(table.as_ref()), Alias::new(column.as_ref()))),
        None => Expr::col(col(column)),
    }
}

/// Build a comparison. Filter operands are configured as strings and bound as
/// text; comparisons against non-text columns may need a `Raw` filter until
/// typed filter values are supported.
fn value_op_expr(
    filter: &schema_core::ValueOpFilter,
    qualifier: Option<&TableName>,
) -> Result<SimpleExpr, SourceError> {
    let column = qualified_col(qualifier, &filter.column);
    let single = |value: &String| Expr::val(value.clone());

    let expr = match (&filter.op, &filter.value) {
        (FilterOp::Eq, FilterValue::Single(v)) => column.eq(single(v)),
        (FilterOp::Neq, FilterValue::Single(v)) => column.ne(single(v)),
        (FilterOp::Lt, FilterValue::Single(v)) => column.lt(single(v)),
        (FilterOp::Lte, FilterValue::Single(v)) => column.lte(single(v)),
        (FilterOp::Gt, FilterValue::Single(v)) => column.gt(single(v)),
        (FilterOp::Gte, FilterValue::Single(v)) => column.gte(single(v)),
        (FilterOp::Like, FilterValue::Single(v)) => column.like(v.clone()),
        (FilterOp::Ilike, FilterValue::Single(v)) => column.ilike(v.clone()),
        (FilterOp::In, FilterValue::List(vs)) => column.is_in(vs.clone()),
        (FilterOp::NotIn, FilterValue::List(vs)) => column.is_not_in(vs.clone()),
        (FilterOp::Between, FilterValue::Range(a, b)) => column.between(single(a), single(b)),
        (op, _) => {
            return Err(SourceError::Query(format!(
                "filter operator {op:?} does not match its value's arity"
            )));
        }
    };
    Ok(expr)
}

fn order_of(direction: Option<Direction>) -> Order {
    match direction {
        Some(Direction::Desc) => Order::Desc,
        _ => Order::Asc,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use schema_core::{JoinKey, JoinType, OrderBy};
    use sea_query::PostgresQueryBuilder;
    use sea_query_binder::SqlxBinder;

    fn db() -> DatabaseSchema {
        DatabaseSchema::try_new("public").unwrap()
    }

    fn column(name: &str) -> ColumnName {
        ColumnName::try_new(name).unwrap()
    }

    fn table(name: &str) -> TableName {
        TableName::try_new(name).unwrap()
    }

    #[test]
    fn root_select_sql() {
        let (sql, _) = root_select(
            &db(),
            &table("users"),
            &[column("id"), column("email")],
            &[(column("id"), GenericValue::Int(5))],
        )
        .unwrap()
        .build_sqlx(PostgresQueryBuilder);
        assert_eq!(
            sql,
            r#"SELECT "id", "email" FROM "public"."users" WHERE "id" = $1"#
        );
    }

    #[test]
    fn reverse_select_sql() {
        let (sql, _) = reverse_select(
            &db(),
            &table("orders"),
            &column("user_id"),
            &[(column("id"), GenericValue::Int(9))],
        )
        .unwrap()
        .build_sqlx(PostgresQueryBuilder);
        assert_eq!(
            sql,
            r#"SELECT "user_id" FROM "public"."orders" WHERE "id" = $1"#
        );
    }

    #[test]
    fn join_select_applies_order_and_limit() {
        let join = Join {
            table: table("orders"),
            join_type: JoinType::OneToMany,
            key: schema_core::JoinKey::Direct(column("user_id")),
            filters: None,
            order_by: Some(vec![OrderBy {
                column: column("created_at"),
                direction: Some(Direction::Desc),
            }]),
            limit: Some(5),
        };
        let (sql, _) = join_select(
            &db(),
            &join,
            &column("user_id"),
            &[column("id"), column("total")],
            &GenericValue::Int(1),
        )
        .unwrap()
        .build_sqlx(PostgresQueryBuilder);
        assert_eq!(
            sql,
            r#"SELECT "id", "total" FROM "public"."orders" WHERE "user_id" = $1 ORDER BY "created_at" DESC LIMIT $2"#
        );
    }

    #[test]
    fn through_select_joins_via_junction() {
        let through = Through {
            table: table("user_tags"),
            left_key: column("user_id"),
            right_key: column("tag_id"),
        };
        let join = Join {
            table: table("tags"),
            join_type: JoinType::ManyToMany,
            key: JoinKey::Through(through.clone()),
            filters: None,
            order_by: None,
            limit: None,
        };
        let (sql, _) = through_select(
            &db(),
            &join,
            &through,
            &column("id"),
            &[column("name")],
            &GenericValue::Int(1),
        )
        .unwrap()
        .build_sqlx(PostgresQueryBuilder);
        assert_eq!(
            sql,
            r#"SELECT "tags"."name" FROM "public"."tags" INNER JOIN "public"."user_tags" ON "user_tags"."tag_id" = "tags"."id" WHERE "user_tags"."user_id" = $1"#
        );
    }

    #[test]
    fn through_aggregate_sums_far_column() {
        let through = Through {
            table: table("user_tags"),
            left_key: column("user_id"),
            right_key: column("tag_id"),
        };
        let aggregate = Aggregate {
            table: table("tags"),
            op: AggregateOp::Sum(column("weight")),
            key: JoinKey::Through(through.clone()),
            filters: None,
        };
        let (sql, _) =
            through_aggregate_select(&db(), &aggregate, &through, &column("id"), &GenericValue::Int(1))
                .unwrap()
                .build_sqlx(PostgresQueryBuilder);
        assert_eq!(
            sql,
            r#"SELECT SUM("tags"."weight") FROM "public"."tags" INNER JOIN "public"."user_tags" ON "user_tags"."tag_id" = "tags"."id" WHERE "user_tags"."user_id" = $1"#
        );
    }

    #[test]
    fn aggregate_count_sql() {
        let aggregate = Aggregate {
            table: table("orders"),
            op: AggregateOp::Count,
            key: schema_core::JoinKey::Direct(column("user_id")),
            filters: None,
        };
        let (sql, _) = aggregate_select(&db(), &aggregate, &column("user_id"), &GenericValue::Int(1))
            .unwrap()
            .build_sqlx(PostgresQueryBuilder);
        assert_eq!(
            sql,
            r#"SELECT COUNT("user_id") FROM "public"."orders" WHERE "user_id" = $1"#
        );
    }
}

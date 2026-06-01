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
    Aggregate, AggregateOp, ColumnName, DatabaseSchema, Direction, Field, FieldRelation, Filter,
    FilterOp, FilterValue, GenericValue, IndexSchema, Join, JoinKey, JoinType, NullOp, OrderBy,
    SoftDelete, TableName, Transform, ValueOpFilter,
};
use sources_core::{Result, SourceError};
use sqlx::postgres::PgArguments;
use sqlx::Postgres;

use super::fields::field_column;

type PgQuery<'q> = sqlx::query::Query<'q, Postgres, PgArguments>;

const ROOT: &str = "root";

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
) -> Result<(String, Vec<GenericValue>)> {
    let mut builder = Builder {
        db: &schema.db_schema,
        pks,
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

    let sql = format!(
        "SELECT {object} AS \"document\" FROM {} AS \"{ROOT}\" WHERE {}",
        qtable(&schema.db_schema, &schema.table),
        conditions.join(" AND "),
    );
    Ok((sql, builder.params))
}

/// Build a reverse-resolution query: one column from a table, filtered by a key.
pub(super) fn reverse_query(
    db: &DatabaseSchema,
    table: &TableName,
    select_column: &ColumnName,
    key: &[(ColumnName, GenericValue)],
) -> Result<(String, Vec<GenericValue>)> {
    let mut params = Vec::new();
    let mut conditions = Vec::new();
    for (column, value) in key {
        if matches!(
            value,
            GenericValue::Null | GenericValue::Array(_) | GenericValue::Map(_)
        ) {
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
    Ok((sql, params))
}

/// Accumulates parameters and unique aliases while building a document query.
struct Builder<'a> {
    db: &'a DatabaseSchema,
    pks: &'a HashMap<String, ColumnName>,
    params: Vec<GenericValue>,
    seq: usize,
}

impl Builder<'_> {
    fn placeholder(&mut self, value: GenericValue) -> Result<String> {
        if matches!(
            value,
            GenericValue::Null | GenericValue::Array(_) | GenericValue::Map(_)
        ) {
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
        self.pks
            .get(&table.to_string())
            .cloned()
            .ok_or_else(|| SourceError::Query(format!("internal: missing primary key for `{table}`")))
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
        match &field.relation {
            Some(FieldRelation::Join(join)) => self.join_value(field, join, parent_alias, parent_pk),
            Some(FieldRelation::Aggregate(aggregate)) => {
                self.aggregate_value(aggregate, parent_alias, parent_pk)
            }
            None => match (&field.column, &field.fields) {
                (Some(column), _) => Ok(column_value(
                    column,
                    field.transforms.as_deref(),
                    field.default.as_ref(),
                    parent_alias,
                )),
                // Same-row nested group: same row, same key.
                (None, Some(nested)) => self.object(nested, parent_alias, parent_pk),
                (None, None) => Ok(literal_or_null(field.default.as_ref())),
            },
        }
    }

    fn join_value(
        &mut self,
        field: &Field,
        join: &Join,
        parent_alias: &str,
        parent_pk: Option<&ColumnName>,
    ) -> Result<String> {
        let parent_pk = require_pk(parent_pk)?;
        let sub = field.fields.as_deref().unwrap_or_default();

        match &join.key {
            JoinKey::Direct(foreign_key) => {
                let child = &join.table;
                let child_pk = self.pk_of(child)?;
                match join.join_type {
                    JoinType::OneToOne => {
                        let alias = self.alias();
                        let object = self.object(sub, &alias, Some(&child_pk))?;
                        let filters = self.filters(join.filters.as_deref(), &alias)?;
                        let order = order_clause(join.order_by.as_deref(), &alias);
                        Ok(format!(
                            "(SELECT {object} FROM {} AS {} WHERE {} = {}{filters}{order} LIMIT 1)",
                            qtable(self.db, child),
                            qident(&alias),
                            qcol(&alias, foreign_key),
                            qcol(parent_alias, parent_pk),
                        ))
                    }
                    JoinType::OneToMany | JoinType::ManyToMany => {
                        let derived = self.alias();
                        let object = self.object(sub, &derived, Some(&child_pk))?;
                        let inner = self.alias();
                        let filters = self.filters(join.filters.as_deref(), &inner)?;
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
                }
            }
            JoinKey::Through(through) => {
                let far = &join.table;
                let far_pk = self.pk_of(far)?;
                let derived = self.alias();
                let object = self.object(sub, &derived, Some(&far_pk))?;
                let far_alias = self.alias();
                let junction_alias = self.alias();
                let filters = self.filters(join.filters.as_deref(), &far_alias)?;
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
            JoinKey::Direct(foreign_key) => {
                let alias = self.alias();
                let function = agg_function(&aggregate.op, &alias);
                let filters = self.filters(aggregate.filters.as_deref(), &alias)?;
                Ok(format!(
                    "(SELECT {function} FROM {} AS {} WHERE {} = {}{filters})",
                    qtable(self.db, &aggregate.table),
                    qident(&alias),
                    qcol(&alias, foreign_key),
                    qcol(parent_alias, parent_pk),
                ))
            }
            JoinKey::Through(through) => {
                let far_pk = self.pk_of(&aggregate.table)?;
                let alias = self.alias();
                let junction_alias = self.alias();
                let function = agg_function(&aggregate.op, &alias);
                let filters = self.filters(aggregate.filters.as_deref(), &alias)?;
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

    /// `… AND (cond)` for each filter, qualified to `alias`.
    fn filters(&mut self, filters: Option<&[Filter]>, alias: &str) -> Result<String> {
        let mut out = String::new();
        if let Some(filters) = filters {
            for filter in filters {
                let condition = self.filter(filter, alias)?;
                out.push_str(" AND (");
                out.push_str(&condition);
                out.push(')');
            }
        }
        Ok(out)
    }

    fn filter(&mut self, filter: &Filter, alias: &str) -> Result<String> {
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
            Filter::ValueOp(op) => self.value_op(op, alias),
        }
    }

    fn value_op(&mut self, filter: &ValueOpFilter, alias: &str) -> Result<String> {
        let column = qcol(alias, &filter.column);
        let expr = match (&filter.op, &filter.value) {
            (FilterOp::Eq, FilterValue::Single(v)) => format!("{column} = {}", self.text_param(v)?),
            (FilterOp::Neq, FilterValue::Single(v)) => format!("{column} <> {}", self.text_param(v)?),
            (FilterOp::Lt, FilterValue::Single(v)) => format!("{column} < {}", self.text_param(v)?),
            (FilterOp::Lte, FilterValue::Single(v)) => format!("{column} <= {}", self.text_param(v)?),
            (FilterOp::Gt, FilterValue::Single(v)) => format!("{column} > {}", self.text_param(v)?),
            (FilterOp::Gte, FilterValue::Single(v)) => format!("{column} >= {}", self.text_param(v)?),
            (FilterOp::Like, FilterValue::Single(v)) => format!("{column} LIKE {}", self.text_param(v)?),
            (FilterOp::Ilike, FilterValue::Single(v)) => {
                format!("{column} ILIKE {}", self.text_param(v)?)
            }
            (FilterOp::In, FilterValue::List(vs)) => format!("{column} IN ({})", self.text_params(vs)?),
            (FilterOp::NotIn, FilterValue::List(vs)) => {
                format!("{column} NOT IN ({})", self.text_params(vs)?)
            }
            (FilterOp::Between, FilterValue::Range(lo, hi)) => {
                format!("{column} BETWEEN {} AND {}", self.text_param(lo)?, self.text_param(hi)?)
            }
            (op, _) => {
                return Err(SourceError::Query(format!(
                    "filter operator {op:?} does not match its value's arity"
                )));
            }
        };
        Ok(expr)
    }

    fn text_param(&mut self, value: &str) -> Result<String> {
        self.placeholder(GenericValue::String(value.to_owned()))
    }

    fn text_params(&mut self, values: &[String]) -> Result<String> {
        let mut placeholders = Vec::with_capacity(values.len());
        for value in values {
            placeholders.push(self.text_param(value)?);
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
        let truthy = format!(
            "CASE WHEN {marker} IS NULL THEN false \
             WHEN pg_typeof({marker}) = 'boolean'::regtype THEN {marker}::boolean \
             ELSE true END"
        );
        let when_sql = self.filters(when, ROOT)?;
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
    transforms: Option<&[Transform]>,
    default: Option<&GenericValue>,
    alias: &str,
) -> String {
    let mut expr = qcol(alias, column);
    if let Some(transforms) = transforms {
        for transform in transforms {
            expr = match transform {
                Transform::Lowercase => format!("lower({expr})"),
                Transform::Trim => format!("trim({expr})"),
            };
        }
    }
    if let Some(literal) = default.and_then(scalar_literal) {
        expr = format!("coalesce({expr}, {literal})");
    }
    expr
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

fn literal_or_null(default: Option<&GenericValue>) -> String {
    default.and_then(scalar_literal).unwrap_or_else(|| "null".to_owned())
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
            column: Some(c(column)),
            mapping: None,
            relation: None,
            transforms: None,
            default: None,
            fields: None,
        }
    }
    fn index(primary_key: Option<&str>, soft_delete: Option<SoftDelete>, fields: Vec<Field>) -> IndexSchema {
        IndexSchema {
            version: 1,
            table: t("users"),
            db_schema: db(),
            primary_key: primary_key.map(c),
            doc_id: None,
            soft_delete,
            fields,
        }
    }

    #[test]
    fn columns_only() {
        let schema = index(Some("id"), None, vec![col_field("id", "id"), col_field("email", "email")]);
        let (sql, params) =
            document_query(&schema, &[(c("id"), GenericValue::Int(7))], &HashMap::new()).unwrap();
        assert_eq!(
            sql,
            r#"SELECT json_build_object('id', "root"."id", 'email', "root"."email") AS "document" FROM "public"."users" AS "root" WHERE "root"."id" = $1"#
        );
        assert_eq!(params, vec![GenericValue::Int(7)]);
    }

    #[test]
    fn one_to_many_with_order_and_limit() {
        let orders = Field {
            field: f("orders"),
            column: None,
            mapping: None,
            relation: Some(FieldRelation::Join(Join {
                table: t("orders"),
                join_type: JoinType::OneToMany,
                key: JoinKey::Direct(c("user_id")),
                filters: None,
                order_by: Some(vec![OrderBy {
                    column: c("created_at"),
                    direction: Some(Direction::Desc),
                }]),
                limit: Some(5),
            })),
            transforms: None,
            default: None,
            fields: Some(vec![col_field("id", "id"), col_field("total", "total")]),
        };
        let schema = index(Some("id"), None, vec![orders]);
        let mut pks = HashMap::new();
        pks.insert("orders".to_owned(), c("id"));
        let (sql, _) =
            document_query(&schema, &[(c("id"), GenericValue::Int(1))], &pks).unwrap();
        assert_eq!(
            sql,
            r#"SELECT json_build_object('orders', (SELECT coalesce(json_agg(json_build_object('id', "rel1"."id", 'total', "rel1"."total") ORDER BY "rel1"."created_at" DESC), '[]'::json) FROM (SELECT "rel2".* FROM "public"."orders" AS "rel2" WHERE "rel2"."user_id" = "root"."id" ORDER BY "rel2"."created_at" DESC LIMIT 5) AS "rel1")) AS "document" FROM "public"."users" AS "root" WHERE "root"."id" = $1"#
        );
    }

    #[test]
    fn aggregate_count() {
        let count = Field {
            field: f("order_count"),
            column: None,
            mapping: None,
            relation: Some(FieldRelation::Aggregate(Aggregate {
                table: t("orders"),
                op: AggregateOp::Count,
                key: JoinKey::Direct(c("user_id")),
                filters: None,
            })),
            transforms: None,
            default: None,
            fields: None,
        };
        let schema = index(Some("id"), None, vec![count]);
        let (sql, _) =
            document_query(&schema, &[(c("id"), GenericValue::Int(1))], &HashMap::new()).unwrap();
        assert_eq!(
            sql,
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
        let (sql, _) =
            document_query(&schema, &[(c("id"), GenericValue::Int(1))], &HashMap::new()).unwrap();
        assert!(sql.contains(
            r#"WHERE "root"."id" = $1 AND NOT ((CASE WHEN "root"."deleted_at" IS NULL THEN false WHEN pg_typeof("root"."deleted_at") = 'boolean'::regtype THEN "root"."deleted_at"::boolean ELSE true END))"#
        ));
    }

    #[test]
    fn reverse_query_selects_foreign_key() {
        let (sql, params) =
            reverse_query(&db(), &t("orders"), &c("user_id"), &[(c("id"), GenericValue::Int(9))]).unwrap();
        assert_eq!(sql, r#"SELECT "user_id" FROM "public"."orders" WHERE "id" = $1"#);
        assert_eq!(params, vec![GenericValue::Int(9)]);
    }
}

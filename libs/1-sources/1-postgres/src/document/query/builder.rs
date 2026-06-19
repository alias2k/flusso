//! The document-assembly query builder.
//!
//! [`Builder`] walks an [`IndexSchema`]'s field tree and emits the nested
//! `json_build_object` expression that assembles a whole document server-side:
//! relations become correlated subqueries (`json_agg` for to-many, a scalar
//! subquery for to-one and aggregates), filters and soft-delete fold into the
//! `WHERE`. It accumulates the bound parameters and the unique relation aliases
//! as it goes. The entry queries in [`super`] drive it; the SQL-text fragments it
//! stitches together live in [`super::sql`].

use std::collections::HashMap;

use schema_core::{
    Aggregate, AggregateKey, AggregateOp, ColumnName, DatabaseSchema, Field, FieldSource, Filter,
    FilterOp, FilterValue, GenericValue, IndexSchema, Join, JoinKind, NullOp, Relation, SoftDelete,
    TableName, ValueOpFilter,
};
use sources_core::{Result, SourceError};

use super::ROOT;
use super::sql::{
    agg_function, column_value, geo_value, ids_agg, json_agg_subquery, json_key, limit_clause,
    literal_or_null, order_clause, qcol, qident, qtable,
};
use crate::document::fields::field_column;

/// Most `(key, value)` field pairs one `json_build_object` can hold: Postgres
/// caps a function call at 100 arguments and each pair spends two.
const MAX_PAIRS_PER_OBJECT: usize = 50;

/// Accumulates parameters and unique aliases while building a document query.
pub(super) struct Builder<'a> {
    pub(super) db: &'a DatabaseSchema,
    pub(super) pks: &'a HashMap<String, ColumnName>,
    /// `(table, column)` → SQL type, for casting filter operands.
    pub(super) col_types: &'a HashMap<(String, String), String>,
    pub(super) params: Vec<GenericValue>,
    pub(super) seq: usize,
}

impl Builder<'_> {
    pub(super) fn placeholder(&mut self, value: GenericValue) -> Result<String> {
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
    ///
    /// Postgres caps a function call at 100 arguments, and each field spends two
    /// (its key and value), so a single `json_build_object` holds at most 50
    /// fields. Past that the object is built in chunks and stitched together with
    /// the jsonb `||` operator — `json` has no concatenation, so the chunks use
    /// `jsonb_build_object`; the consumer decodes `json`/`jsonb` identically.
    pub(super) fn object(
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
        if pairs.len() <= MAX_PAIRS_PER_OBJECT {
            return Ok(format!("json_build_object({})", pairs.join(", ")));
        }
        let chunks: Vec<String> = pairs
            .chunks(MAX_PAIRS_PER_OBJECT)
            .map(|chunk| format!("jsonb_build_object({})", chunk.join(", ")))
            .collect();
        Ok(format!("({})", chunks.join(" || ")))
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
            FieldSource::Group(nested) => self.object(nested, parent_alias, parent_pk),
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
        let is_ids = matches!(&aggregate.op, AggregateOp::Ids { .. });
        match &aggregate.key {
            AggregateKey::Direct(foreign_key) => {
                let alias = self.alias();
                let function = if is_ids {
                    ids_agg(&alias, &self.pk_of(&aggregate.table)?)
                } else {
                    agg_function(&aggregate.op, &alias)
                };
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
            // `ids` over a junction needs no far-table join: the junction's
            // `right_key` already holds the far table's primary-key values, so
            // collect them straight off the junction.
            AggregateKey::Through(through) if is_ids => {
                let junction_alias = self.alias();
                let function = ids_agg(&junction_alias, &through.right_key);
                Ok(format!(
                    "(SELECT {function} FROM {} AS {ja} WHERE {} = {})",
                    qtable(self.db, &through.table),
                    qcol(&junction_alias, &through.left_key),
                    qcol(parent_alias, parent_pk),
                    ja = qident(&junction_alias),
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
    pub(super) fn filters(
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

    /// A bound value cast to its column's catalog SQL type — `$n::<type>`.
    ///
    /// Every value re-injected into a predicate — filter operands *and* the
    /// keys/foreign keys that drive document lookup — goes through here, so a
    /// non-`text` column compares against its own type (`uuid = uuid`,
    /// `date = date`) instead of degrading to `text` and tripping
    /// `operator does not exist: uuid = text`. The type is the catalog's
    /// `format_type` name, resolved before query building (see
    /// [`PgDocumentBuilder::column_type`](crate::document::PgDocumentBuilder::column_type)).
    /// A missing type is an internal error, not a silent bare bind: that is what
    /// keeps the cast mandatory.
    pub(super) fn typed_placeholder(
        &mut self,
        value: GenericValue,
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
        let placeholder = self.placeholder(value)?;
        Ok(format!("{placeholder}::{sql_type}"))
    }

    /// [`typed_placeholder`](Self::typed_placeholder) for a filter operand, whose
    /// value is carried as a raw string.
    fn typed_param(
        &mut self,
        value: &str,
        table: &TableName,
        column: &ColumnName,
    ) -> Result<String> {
        self.typed_placeholder(GenericValue::String(value.to_owned()), table, column)
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
    pub(super) fn soft_delete_predicate(&mut self, schema: &IndexSchema) -> Result<Option<String>> {
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

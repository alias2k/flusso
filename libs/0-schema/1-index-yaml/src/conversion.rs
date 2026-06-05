use std::collections::BTreeMap;
use std::str::FromStr;

use rust_decimal::Decimal;
use schema_core::{
    Aggregate, AggregateOp, Column, ColumnName, Direction, Field, FieldSource, Filter, FilterOp,
    FilterValue, FlussoType, GenericValue, Join, JoinKey, JoinType, NullCheckFilter, NullOp,
    OrderBy, RawFilter, Relation, SoftDelete, SoftDeleteColumn, SoftDeleteField, Through,
    Transform, ValueOpFilter,
};

use crate::ConversionError;
use crate::entities;

/// The default leaf type for a column field that declares none — the historical
/// "anything text-like is a keyword" fallback, now explicit and database-free.
const DEFAULT_COLUMN_TYPE: FlussoType = FlussoType::Keyword;

pub(crate) fn convert_field(f: entities::Field) -> Result<Field, ConversionError> {
    match f {
        entities::Field::Short(name) => {
            // Shorthand `- foo` is a `keyword` column of the same name.
            let column = default_column(&name)?;
            Ok(Field {
                field: name,
                options: BTreeMap::new(),
                source: FieldSource::Column(Column {
                    column,
                    ty: DEFAULT_COLUMN_TYPE,
                    nullable: true,
                    transforms: Vec::new(),
                    default: None,
                }),
            })
        }
        entities::Field::Group(g) => {
            // A same-row sub-object: an `object`, never null, assembled from its
            // nested fields. The `group` key is its document key.
            let options = g
                .options
                .into_iter()
                .map(|(k, v)| (k, yaml_to_generic(v)))
                .collect();
            let fields = g
                .fields
                .into_iter()
                .map(convert_field)
                .collect::<Result<_, _>>()?;
            Ok(Field {
                field: g.group,
                options,
                source: FieldSource::Group(fields),
            })
        }
        entities::Field::Full(def) => {
            let def = *def;
            let field_name = def.field.to_string();
            let options: BTreeMap<String, GenericValue> = def
                .options
                .into_iter()
                .map(|(k, v)| (k, yaml_to_generic(v)))
                .collect();

            match (def.join, def.aggregate) {
                (Some(_), Some(_)) => Err(ConversionError::ConflictingRelation),
                // A join folds in a related table; its shape (`nested`/`object`)
                // is structural, so a declared `type` or `kind` is rejected.
                (Some(j), None) => {
                    reject_type_on_relation(&def.ty, &def.kind, &field_name)?;
                    Ok(Field {
                        field: def.field,
                        options,
                        source: FieldSource::Relation(Relation::Join(convert_join(j)?)),
                    })
                }
                (None, Some(a)) => {
                    if def.kind.is_some() {
                        return Err(ConversionError::KindOnNonScalarField);
                    }
                    Ok(Field {
                        field: def.field,
                        options,
                        source: FieldSource::Relation(Relation::Aggregate(convert_aggregate(
                            a, def.ty,
                        )?)),
                    })
                }
                (None, None) => {
                    let column = match def.column {
                        Some(column) => column,
                        None => default_column(&def.field)?,
                    };
                    let (ty, options) = resolve_column_type(def.ty, def.kind, options)?;
                    Ok(Field {
                        field: def.field,
                        options,
                        source: FieldSource::Column(Column {
                            column,
                            ty,
                            nullable: !def.required.unwrap_or(false),
                            transforms: def
                                .transforms
                                .map(|ts| ts.into_iter().map(convert_transform).collect())
                                .unwrap_or_default(),
                            default: def.default.map(yaml_to_generic),
                        }),
                    })
                }
            }
        }
    }
}

/// A declared `type` or `kind` makes no sense on a structural field (a join or a
/// group), whose shape is fixed by the relation, not the schema.
fn reject_type_on_relation(
    ty: &Option<FlussoType>,
    kind: &Option<entities::FieldKind>,
    field: &str,
) -> Result<(), ConversionError> {
    if ty.is_some() {
        return Err(ConversionError::TypeOnNonScalarField {
            field: field.to_owned(),
        });
    }
    if kind.is_some() {
        return Err(ConversionError::KindOnNonScalarField);
    }
    Ok(())
}

/// The declared type of a column field, folding in the `kind` shorthand. `kind`
/// forces `text` and adds the matching `flusso_*` analyzer (an explicit
/// `analyzer` option always wins); without `kind`, an omitted `type` defaults to
/// `keyword`.
fn resolve_column_type(
    ty: Option<FlussoType>,
    kind: Option<entities::FieldKind>,
    mut options: BTreeMap<String, GenericValue>,
) -> Result<(FlussoType, BTreeMap<String, GenericValue>), ConversionError> {
    let Some(kind) = kind else {
        return Ok((ty.unwrap_or(DEFAULT_COLUMN_TYPE), options));
    };

    // `kind` is a full-text hint, so the type must be `text`.
    match &ty {
        None | Some(FlussoType::Text) => {}
        Some(other) => {
            return Err(ConversionError::KindRequiresTextType {
                got: format!("{other:?}"),
            });
        }
    }

    let analyzer = match kind {
        entities::FieldKind::Code => "flusso_code",
        entities::FieldKind::Prose => "flusso_text",
    };
    options
        .entry("analyzer".to_owned())
        .or_insert_with(|| GenericValue::String(analyzer.to_owned()));

    Ok((FlussoType::Text, options))
}

/// The column a field reads from when none is given: the field name itself.
/// `ColumnName` lowercases, matching Postgres's folding of unquoted identifiers;
/// a field name that isn't a valid column identifier must set `column`
/// explicitly.
fn default_column(field: &schema_core::FieldName) -> Result<ColumnName, ConversionError> {
    Ok(ColumnName::try_new(field.as_ref())?)
}

fn convert_transform(t: entities::Transform) -> Transform {
    match t {
        entities::Transform::Lowercase => Transform::Lowercase,
        entities::Transform::Trim => Transform::Trim,
    }
}

pub(crate) fn convert_soft_delete(sd: entities::SoftDelete) -> Result<SoftDelete, ConversionError> {
    match sd {
        entities::SoftDelete::Field(f) => Ok(SoftDelete::Field(SoftDeleteField {
            field: f.field,
            when: convert_filters_opt(f.when)?,
        })),
        entities::SoftDelete::Column(c) => Ok(SoftDelete::Column(SoftDeleteColumn {
            column: c.column,
            when: convert_filters_opt(c.when)?,
        })),
    }
}

pub(crate) fn convert_filters_opt(
    filters: Option<Vec<entities::Filter>>,
) -> Result<Option<Vec<Filter>>, ConversionError> {
    filters
        .map(|fs| fs.into_iter().map(convert_filter).collect())
        .transpose()
}

fn convert_filter(f: entities::Filter) -> Result<Filter, ConversionError> {
    match f {
        entities::Filter::Raw(r) => Ok(Filter::Raw(RawFilter { raw: r.raw })),
        entities::Filter::NullCheck(n) => Ok(Filter::NullCheck(NullCheckFilter {
            column: n.column,
            op: match n.op {
                entities::NullOp::IsNull => NullOp::IsNull,
                entities::NullOp::IsNotNull => NullOp::IsNotNull,
            },
        })),
        entities::Filter::ValueOp(v) => {
            let op = convert_filter_op(v.op);
            let value = convert_filter_value(v.op, v.value)?;
            Ok(Filter::ValueOp(ValueOpFilter {
                column: v.column,
                op,
                value,
            }))
        }
    }
}

fn convert_filter_op(op: entities::FilterOp) -> FilterOp {
    match op {
        entities::FilterOp::Eq => FilterOp::Eq,
        entities::FilterOp::Neq => FilterOp::Neq,
        entities::FilterOp::Lt => FilterOp::Lt,
        entities::FilterOp::Lte => FilterOp::Lte,
        entities::FilterOp::Gt => FilterOp::Gt,
        entities::FilterOp::Gte => FilterOp::Gte,
        entities::FilterOp::In => FilterOp::In,
        entities::FilterOp::NotIn => FilterOp::NotIn,
        entities::FilterOp::Like => FilterOp::Like,
        entities::FilterOp::Ilike => FilterOp::Ilike,
        entities::FilterOp::Between => FilterOp::Between,
    }
}

fn convert_filter_value(
    op: entities::FilterOp,
    value: Option<serde_yaml::Value>,
) -> Result<FilterValue, ConversionError> {
    let op_name = filter_op_name(op);
    let v = value.ok_or(ConversionError::MissingFilterValue { op: op_name })?;

    match op {
        entities::FilterOp::In | entities::FilterOp::NotIn => match v {
            serde_yaml::Value::Sequence(seq) => Ok(FilterValue::List(
                seq.into_iter().map(yaml_scalar_to_string).collect(),
            )),
            _ => Err(ConversionError::ExpectedListValue { op: op_name }),
        },
        entities::FilterOp::Between => match v {
            serde_yaml::Value::Sequence(seq) if seq.len() == 2 => {
                let mut iter = seq.into_iter();
                let lower = yaml_scalar_to_string(iter.next().unwrap_or(serde_yaml::Value::Null));
                let upper = yaml_scalar_to_string(iter.next().unwrap_or(serde_yaml::Value::Null));
                Ok(FilterValue::Range(lower, upper))
            }
            serde_yaml::Value::Sequence(seq) => {
                Err(ConversionError::InvalidBetweenArity { got: seq.len() })
            }
            _ => Err(ConversionError::ExpectedListValue { op: op_name }),
        },
        _ => Ok(FilterValue::Single(yaml_scalar_to_string(v))),
    }
}

fn filter_op_name(op: entities::FilterOp) -> &'static str {
    match op {
        entities::FilterOp::Eq => "eq",
        entities::FilterOp::Neq => "neq",
        entities::FilterOp::Lt => "lt",
        entities::FilterOp::Lte => "lte",
        entities::FilterOp::Gt => "gt",
        entities::FilterOp::Gte => "gte",
        entities::FilterOp::In => "in",
        entities::FilterOp::NotIn => "not_in",
        entities::FilterOp::Like => "like",
        entities::FilterOp::Ilike => "ilike",
        entities::FilterOp::Between => "between",
    }
}

fn yaml_scalar_to_string(v: serde_yaml::Value) -> String {
    match v {
        serde_yaml::Value::String(s) => s,
        serde_yaml::Value::Number(n) => n.to_string(),
        serde_yaml::Value::Bool(b) => b.to_string(),
        serde_yaml::Value::Null => "null".to_owned(),
        _ => String::new(),
    }
}

pub(crate) fn yaml_to_generic(v: serde_yaml::Value) -> GenericValue {
    match v {
        serde_yaml::Value::Null => GenericValue::Null,
        serde_yaml::Value::Bool(b) => GenericValue::Bool(b),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                GenericValue::Int(i)
            } else {
                let s = n.to_string();
                match Decimal::from_str(&s) {
                    Ok(d) => GenericValue::Decimal(d),
                    Err(_) => GenericValue::String(s),
                }
            }
        }
        serde_yaml::Value::String(s) => GenericValue::String(s),
        serde_yaml::Value::Sequence(seq) => {
            GenericValue::Array(seq.into_iter().map(yaml_to_generic).collect())
        }
        serde_yaml::Value::Mapping(map) => GenericValue::Map(
            map.into_iter()
                .filter_map(|(k, v)| {
                    if let serde_yaml::Value::String(s) = k {
                        Some((s, yaml_to_generic(v)))
                    } else {
                        None
                    }
                })
                .collect(),
        ),
        serde_yaml::Value::Tagged(tagged) => yaml_to_generic(tagged.value),
    }
}

pub(crate) fn convert_join(j: entities::Join) -> Result<Join, ConversionError> {
    let key = match (j.foreign_key, j.through) {
        (Some(fk), None) => JoinKey::Direct(fk),
        (None, Some(t)) => JoinKey::Through(Through {
            table: t.table,
            left_key: t.left_key,
            right_key: t.right_key,
        }),
        _ => return Err(ConversionError::InvalidJoinKey),
    };
    let fields = j
        .fields
        .into_iter()
        .map(convert_field)
        .collect::<Result<_, _>>()?;
    Ok(Join {
        table: j.table,
        join_type: convert_join_type(j.join_type),
        primary_key: j.primary_key,
        key,
        filters: convert_filters_opt(j.filters)?,
        order_by: j
            .order_by
            .map(|os| os.into_iter().map(convert_order_by).collect()),
        limit: j.limit,
        fields,
    })
}

fn convert_join_type(jt: entities::JoinType) -> JoinType {
    match jt {
        entities::JoinType::OneToOne => JoinType::OneToOne,
        entities::JoinType::OneToMany => JoinType::OneToMany,
        entities::JoinType::ManyToMany => JoinType::ManyToMany,
    }
}

fn convert_order_by(ob: entities::OrderBy) -> OrderBy {
    OrderBy {
        column: ob.column,
        direction: ob.direction.map(|d| match d {
            entities::Direction::Asc => Direction::Asc,
            entities::Direction::Desc => Direction::Desc,
        }),
    }
}

pub(crate) fn convert_aggregate(
    a: entities::Aggregate,
    ty: Option<FlussoType>,
) -> Result<Aggregate, ConversionError> {
    // `count`/`avg` have fixed result types (`long`/`double`); `sum`/`min`/`max`
    // mirror the aggregated column, so they must carry a declared `type`.
    let (op, value_type) = match a.op {
        entities::AggregateOp::Count => (AggregateOp::Count, None),
        entities::AggregateOp::Avg => (
            AggregateOp::Avg(require_aggregate_column(a.column, "avg")?),
            None,
        ),
        entities::AggregateOp::Sum => (
            AggregateOp::Sum(require_aggregate_column(a.column, "sum")?),
            Some(require_aggregate_type(ty, "sum")?),
        ),
        entities::AggregateOp::Min => (
            AggregateOp::Min(require_aggregate_column(a.column, "min")?),
            Some(require_aggregate_type(ty, "min")?),
        ),
        entities::AggregateOp::Max => (
            AggregateOp::Max(require_aggregate_column(a.column, "max")?),
            Some(require_aggregate_type(ty, "max")?),
        ),
    };
    let key = match (a.foreign_key, a.through) {
        (Some(fk), None) => JoinKey::Direct(fk),
        (None, Some(t)) => JoinKey::Through(Through {
            table: t.table,
            left_key: t.left_key,
            right_key: t.right_key,
        }),
        _ => return Err(ConversionError::InvalidJoinKey),
    };
    Ok(Aggregate {
        table: a.table,
        op,
        key,
        value_type,
        filters: convert_filters_opt(a.filters)?,
    })
}

fn require_aggregate_column(
    column: Option<ColumnName>,
    op: &'static str,
) -> Result<ColumnName, ConversionError> {
    column.ok_or(ConversionError::MissingAggregateColumn { op })
}

fn require_aggregate_type(
    ty: Option<FlussoType>,
    op: &'static str,
) -> Result<FlussoType, ConversionError> {
    ty.ok_or(ConversionError::MissingAggregateType { op })
}

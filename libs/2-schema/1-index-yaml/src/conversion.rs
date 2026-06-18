use std::collections::BTreeMap;
use std::str::FromStr;

use rust_decimal::Decimal;
use schema_core::{
    Aggregate, AggregateKey, AggregateOp, Column, ColumnName, Direction, Field, FieldSource,
    Filter, FilterOp, FilterValue, FlussoType, GenericValue, Geo, Join, JoinKind, NullCheckFilter,
    NullOp, OrderBy, RawFilter, Relation, SoftDelete, SoftDeleteColumn, SoftDeleteField, Through,
    Transform, ValueOpFilter,
};

use crate::ConversionError;
use crate::entities;

pub(crate) fn convert_field(f: entities::Field) -> Result<Field, ConversionError> {
    match f {
        entities::Field::Scalar(ty, body) => convert_scalar(ty, body),
        entities::Field::Geo(body) => convert_geo(body),
        entities::Field::Object(body) => {
            let fields = body
                .fields
                .into_iter()
                .map(convert_field)
                .collect::<Result<_, _>>()?;
            Ok(Field {
                field: body.field,
                options: convert_options(body.options),
                source: FieldSource::Group(fields),
            })
        }
        entities::Field::Join(verb, body) => convert_join_field(verb, *body),
        entities::Field::Aggregate(op, body) => convert_aggregate_field(op, *body),
        entities::Field::Constant(body) => Ok(Field {
            field: body.field,
            options: BTreeMap::new(),
            source: FieldSource::Constant(yaml_to_generic(body.value)),
        }),
    }
}

/// A scalar leaf: a column with a declared type and nullability.
fn convert_scalar(ty: FlussoType, body: entities::ScalarBody) -> Result<Field, ConversionError> {
    let column = match body.column {
        Some(column) => column,
        None => default_column(&body.field)?,
    };
    let options = convert_options(body.options);
    let (ty, options) = resolve_column_type(ty, options);
    let transforms = body
        .transforms
        .map(|ts| ts.into_iter().map(convert_transform).collect())
        .unwrap_or_default();
    Ok(Field {
        field: body.field,
        options,
        source: FieldSource::Column(Column {
            column,
            ty,
            nullable: !body.required,
            transforms,
            default: body.default.map(yaml_to_generic),
        }),
    })
}

/// A geo point: either two coordinate columns (`lat`/`lon`) assembled into a
/// `geo_point`, or a single `column` already holding a `geo_point`-shaped value.
fn convert_geo(body: entities::GeoBody) -> Result<Field, ConversionError> {
    let options = convert_options(body.options);
    let source = match (body.lat, body.lon, body.column) {
        (Some(lat), Some(lon), None) => FieldSource::Geo(Geo {
            lat,
            lon,
            nullable: !body.required,
        }),
        (None, None, column) => {
            let column = match column {
                Some(column) => column,
                None => default_column(&body.field)?,
            };
            FieldSource::Column(Column {
                column,
                ty: FlussoType::GeoPoint,
                nullable: !body.required,
                transforms: Vec::new(),
                default: None,
            })
        }
        _ => return Err(ConversionError::InvalidGeoSource),
    };
    Ok(Field {
        field: body.field,
        options,
        source,
    })
}

fn convert_join_field(
    verb: entities::JoinVerb,
    body: entities::JoinBody,
) -> Result<Field, ConversionError> {
    let kind = join_kind(verb, &body)?;
    // Ordering/limiting is only meaningful when there are rows to choose among.
    // `has_one` keeps `order_by` (it picks *which* row becomes the object);
    // `belongs_to` targets a unique row by primary key, so neither applies, and
    // both to-one verbs imply their own LIMIT 1.
    if matches!(verb, entities::JoinVerb::BelongsTo) && body.order_by.is_some() {
        return Err(sibling_not_allowed(verb, "order_by"));
    }
    if matches!(
        verb,
        entities::JoinVerb::BelongsTo | entities::JoinVerb::HasOne
    ) && body.limit.is_some()
    {
        return Err(sibling_not_allowed(verb, "limit"));
    }
    let fields = body
        .fields
        .into_iter()
        .map(convert_field)
        .collect::<Result<_, _>>()?;
    let join = Join {
        table: body.table,
        kind,
        primary_key: body.primary_key,
        filters: convert_filters_opt(body.filters)?,
        order_by: body
            .order_by
            .map(|os| os.into_iter().map(convert_order_by).collect()),
        limit: body.limit,
        fields,
    };
    Ok(Field {
        field: body.field,
        options: convert_options(body.options),
        source: FieldSource::Relation(Relation::Join(join)),
    })
}

/// Build the join's [`JoinKind`] from its verb, enforcing that exactly the key
/// sibling that verb takes is present: `column` for `belongs_to` (defaulting to
/// the field name, the same way scalar fields default), `foreign_key` for
/// `has_one`/`has_many`, `through` for `many_to_many`.
fn join_kind(
    verb: entities::JoinVerb,
    body: &entities::JoinBody,
) -> Result<JoinKind, ConversionError> {
    use entities::JoinVerb;

    let allowed: &[&str] = match verb {
        JoinVerb::BelongsTo => &["column"],
        JoinVerb::HasOne | JoinVerb::HasMany => &["foreign_key"],
        JoinVerb::ManyToMany => &["through"],
    };
    let present: [(&str, bool); 3] = [
        ("column", body.column.is_some()),
        ("foreign_key", body.foreign_key.is_some()),
        ("through", body.through.is_some()),
    ];
    for (sibling, is_present) in present {
        if is_present && !allowed.contains(&sibling) {
            return Err(ConversionError::UnexpectedJoinKey {
                verb: verb.as_str(),
                sibling,
                expected: key_hint(verb),
            });
        }
    }

    Ok(match verb {
        JoinVerb::BelongsTo => JoinKind::BelongsTo {
            column: match body.column.clone() {
                Some(column) => column,
                None => default_column(&body.field)?,
            },
        },
        JoinVerb::HasOne => JoinKind::HasOne {
            foreign_key: require_join_key(verb, body.foreign_key.clone())?,
        },
        JoinVerb::HasMany => JoinKind::HasMany {
            foreign_key: require_join_key(verb, body.foreign_key.clone())?,
        },
        JoinVerb::ManyToMany => JoinKind::ManyToMany {
            through: match body.through.clone() {
                Some(t) => Through {
                    table: t.table,
                    left_key: t.left_key,
                    right_key: t.right_key,
                },
                None => {
                    return Err(ConversionError::MissingJoinKey {
                        verb: verb.as_str(),
                        expected: key_hint(verb),
                    });
                }
            },
        },
    })
}

/// What the verb's key sibling is, for error messages.
fn key_hint(verb: entities::JoinVerb) -> &'static str {
    match verb {
        entities::JoinVerb::BelongsTo => {
            "`column` — this table's column pointing at the related row \
             (defaults to the field name)"
        }
        entities::JoinVerb::HasOne | entities::JoinVerb::HasMany => {
            "`foreign_key` — the related table's column pointing back at this row"
        }
        entities::JoinVerb::ManyToMany => "`through` — the junction table and its two keys",
    }
}

fn require_join_key(
    verb: entities::JoinVerb,
    foreign_key: Option<ColumnName>,
) -> Result<ColumnName, ConversionError> {
    foreign_key.ok_or(ConversionError::MissingJoinKey {
        verb: verb.as_str(),
        expected: key_hint(verb),
    })
}

fn sibling_not_allowed(verb: entities::JoinVerb, sibling: &'static str) -> ConversionError {
    ConversionError::UnexpectedJoinSibling {
        verb: verb.as_str(),
        sibling,
    }
}

fn convert_aggregate_field(
    op: entities::AggregateOp,
    body: entities::AggregateBody,
) -> Result<Field, ConversionError> {
    let (op, value_type) = convert_aggregate_op(op, body.column, body.value_type)?;
    let key = aggregate_key(body.foreign_key, body.through)?;
    let aggregate = Aggregate {
        table: body.table,
        op,
        key,
        value_type,
        filters: convert_filters_opt(body.filters)?,
    };
    Ok(Field {
        field: body.field,
        options: convert_options(body.options),
        source: FieldSource::Relation(Relation::Aggregate(aggregate)),
    })
}

/// `count`/`avg` have fixed result types (`long`/`double`); `sum`/`min`/`max`
/// mirror the aggregated column, so they require a `column` and a `value_type`.
fn convert_aggregate_op(
    op: entities::AggregateOp,
    column: Option<ColumnName>,
    value_type: Option<FlussoType>,
) -> Result<(AggregateOp, Option<FlussoType>), ConversionError> {
    Ok(match op {
        entities::AggregateOp::Count => (AggregateOp::Count, None),
        entities::AggregateOp::Avg => (
            AggregateOp::Avg(require_aggregate_column(column, "avg")?),
            None,
        ),
        entities::AggregateOp::Sum => (
            AggregateOp::Sum(require_aggregate_column(column, "sum")?),
            Some(require_aggregate_type(value_type, "sum")?),
        ),
        entities::AggregateOp::Min => (
            AggregateOp::Min(require_aggregate_column(column, "min")?),
            Some(require_aggregate_type(value_type, "min")?),
        ),
        entities::AggregateOp::Max => (
            AggregateOp::Max(require_aggregate_column(column, "max")?),
            Some(require_aggregate_type(value_type, "max")?),
        ),
    })
}

/// An aggregate key: exactly one of `foreign_key` or `through`.
fn aggregate_key(
    foreign_key: Option<ColumnName>,
    through: Option<entities::Through>,
) -> Result<AggregateKey, ConversionError> {
    match (foreign_key, through) {
        (Some(fk), None) => Ok(AggregateKey::Direct(fk)),
        (None, Some(t)) => Ok(AggregateKey::Through(Through {
            table: t.table,
            left_key: t.left_key,
            right_key: t.right_key,
        })),
        _ => Err(ConversionError::InvalidAggregateKey),
    }
}

fn convert_options(options: BTreeMap<String, serde_yaml::Value>) -> BTreeMap<String, GenericValue> {
    options
        .into_iter()
        .map(|(k, v)| (k, yaml_to_generic(v)))
        .collect()
}

/// Apply type-specific option defaults. The `identifier` type is analyzed `text`
/// carrying the `flusso_code` analyzer (an explicit `analyzer` option wins).
fn resolve_column_type(
    ty: FlussoType,
    mut options: BTreeMap<String, GenericValue>,
) -> (FlussoType, BTreeMap<String, GenericValue>) {
    if ty == FlussoType::Identifier {
        options
            .entry("analyzer".to_owned())
            .or_insert_with(|| GenericValue::String("flusso_code".to_owned()));
    }
    (ty, options)
}

/// The column a field reads from when none is given: the field name itself.
/// `ColumnName` lowercases, matching Postgres's folding of unquoted identifiers;
/// a field name that isn't a valid column identifier must set `column`.
fn default_column(field: &schema_core::FieldName) -> Result<ColumnName, ConversionError> {
    Ok(ColumnName::try_new(field.as_ref())?)
}

fn convert_transform(t: entities::Transform) -> Transform {
    match t {
        entities::Transform::Lowercase => Transform::Lowercase,
        entities::Transform::Trim => Transform::Trim,
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

fn require_aggregate_column(
    column: Option<ColumnName>,
    op: &'static str,
) -> Result<ColumnName, ConversionError> {
    column.ok_or(ConversionError::MissingAggregateColumn { op })
}

/// The `value_type` of a `sum`/`min`/`max` aggregate mirrors the aggregated
/// column, so it must be a scalar type. `geo_point` and `custom` are not
/// meaningful results for these ops and are rejected.
fn require_aggregate_type(
    ty: Option<FlussoType>,
    op: &'static str,
) -> Result<FlussoType, ConversionError> {
    match ty.ok_or(ConversionError::MissingAggregateType { op })? {
        FlussoType::GeoPoint | FlussoType::Custom { .. } => {
            Err(ConversionError::InvalidAggregateType { op })
        }
        scalar => Ok(scalar),
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

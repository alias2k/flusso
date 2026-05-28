use std::str::FromStr;

use rust_decimal::Decimal;
use schema_core::{
    Aggregate, AggregateOp, Direction, Field, FieldRelation, Filter, FilterOp, FilterValue,
    GenericValue, Join, JoinKey, JoinType, Mapping, MappingType, NullCheckFilter, NullOp,
    OrderBy, RawFilter, SoftDelete, SoftDeleteColumn, SoftDeleteField, Through, Transform,
    ValueOpFilter,
};

use crate::entities;
use crate::ConversionError;

pub(crate) fn convert_field(f: entities::Field) -> Result<Field, ConversionError> {
    match f {
        entities::Field::Short(name) => Ok(Field {
            field: name,
            column: None,
            mapping: None,
            relation: None,
            transforms: None,
            default: None,
            fields: None,
        }),
        entities::Field::Full(def) => {
            let relation = match (def.join, def.aggregate) {
                (Some(j), None) => Some(FieldRelation::Join(convert_join(j)?)),
                (None, Some(a)) => Some(FieldRelation::Aggregate(convert_aggregate(a)?)),
                (None, None) => None,
                (Some(_), Some(_)) => return Err(ConversionError::ConflictingRelation),
            };
            Ok(Field {
                field: def.field,
                column: def.column,
                mapping: def.mapping.map(convert_mapping),
                relation,
                transforms: def
                    .transforms
                    .map(|ts| ts.into_iter().map(convert_transform).collect()),
                default: def.default.map(yaml_to_generic),
                fields: def
                    .fields
                    .map(|fs| fs.into_iter().map(convert_field).collect::<Result<_, _>>())
                    .transpose()?,
            })
        }
    }
}

fn convert_mapping(m: entities::Mapping) -> Mapping {
    Mapping {
        mapping_type: parse_mapping_type(m.mapping_type),
        extra: m
            .extra
            .into_iter()
            .map(|(k, v)| (k, yaml_to_generic(v)))
            .collect(),
    }
}

fn parse_mapping_type(s: String) -> MappingType {
    match s.as_str() {
        "text" => MappingType::Text,
        "keyword" => MappingType::Keyword,
        "boolean" => MappingType::Boolean,
        "byte" => MappingType::Byte,
        "short" => MappingType::Short,
        "integer" => MappingType::Integer,
        "long" => MappingType::Long,
        "float" => MappingType::Float,
        "double" => MappingType::Double,
        "half_float" => MappingType::HalfFloat,
        "scaled_float" => MappingType::ScaledFloat,
        "date" => MappingType::Date,
        "object" => MappingType::Object,
        "nested" => MappingType::Nested,
        _ => MappingType::Other(s),
    }
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
                let lower = yaml_scalar_to_string(
                    iter.next().unwrap_or(serde_yaml::Value::Null),
                );
                let upper = yaml_scalar_to_string(
                    iter.next().unwrap_or(serde_yaml::Value::Null),
                );
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
    Ok(Join {
        table: j.table,
        join_type: convert_join_type(j.join_type),
        key,
        filters: convert_filters_opt(j.filters)?,
        order_by: j
            .order_by
            .map(|os| os.into_iter().map(convert_order_by).collect()),
        limit: j.limit,
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

pub(crate) fn convert_aggregate(a: entities::Aggregate) -> Result<Aggregate, ConversionError> {
    let op = match a.op {
        entities::AggregateOp::Count => AggregateOp::Count,
        entities::AggregateOp::Sum => {
            AggregateOp::Sum(a.column.ok_or(ConversionError::MissingAggregateColumn { op: "sum" })?)
        }
        entities::AggregateOp::Avg => {
            AggregateOp::Avg(a.column.ok_or(ConversionError::MissingAggregateColumn { op: "avg" })?)
        }
        entities::AggregateOp::Min => {
            AggregateOp::Min(a.column.ok_or(ConversionError::MissingAggregateColumn { op: "min" })?)
        }
        entities::AggregateOp::Max => {
            AggregateOp::Max(a.column.ok_or(ConversionError::MissingAggregateColumn { op: "max" })?)
        }
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
        filters: convert_filters_opt(a.filters)?,
    })
}

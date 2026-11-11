use std::collections::BTreeMap;
use std::str::FromStr;

use kernel::{
    Aggregate, AggregateKey, AggregateOp, Column, ColumnName, ColumnPair, Direction, Field,
    FieldSource, Filter, FilterOp, FilterValue, FlussoType, GenericValue, Geo, Join, JoinKind,
    NullCheckFilter, NullOp, OrderBy, RawFilter, Relation, Script, SoftDelete, SoftDeleteColumn,
    SoftDeleteField, Through, Transform, ValueOpFilter, ViaHop,
};
use rust_decimal::Decimal;

use super::ConversionError;
use super::entities;

pub(crate) fn convert_field(f: entities::Field) -> Result<Field, ConversionError> {
    let mut field = convert_field_kind(f.kind)?;
    if let Some(emit) = f.emit {
        field.emit = emit;
    }
    Ok(field)
}

fn convert_field_kind(kind: entities::FieldKind) -> Result<Field, ConversionError> {
    match kind {
        entities::FieldKind::Scalar(ty, body) => convert_scalar(ty, body),
        entities::FieldKind::Enum(body) => convert_enum(body),
        entities::FieldKind::Geo(body) => convert_geo(body),
        entities::FieldKind::Object(body) => {
            let fields = body
                .fields
                .into_iter()
                .map(convert_field)
                .collect::<Result<_, _>>()?;
            Ok(Field {
                field: body.field,
                options: convert_options(body.options),
                emit: true,
                source: FieldSource::Group(fields),
            })
        }
        entities::FieldKind::Map(body) => convert_map(body),
        entities::FieldKind::Join(verb, body) => convert_join_field(verb, *body),
        entities::FieldKind::Aggregate(op, body) => convert_aggregate_field(op, *body),
        entities::FieldKind::Constant(body) => Ok(Field {
            field: body.field,
            options: BTreeMap::new(),
            emit: true,
            source: FieldSource::Constant(yaml_to_generic(body.value)),
        }),
    }
}

/// A scalar leaf: a column with a declared type and nullability, or a script
/// computing a value of that type.
fn convert_scalar(ty: FlussoType, body: entities::ScalarBody) -> Result<Field, ConversionError> {
    if let Some(source) = body.script {
        if body.indexed.is_some() {
            return Err(ConversionError::IndexedNotAllowed {
                tag: scalar_tag(&ty),
                field: body.field.to_string(),
            });
        }
        return convert_script(
            ScriptParts {
                field: body.field,
                ty,
                source,
                required: body.required,
                array: body.array,
                enum_order: Vec::new(),
                subfields: body.subfields,
                options: body.options,
            },
            [
                ("column", body.column.is_some()),
                ("transforms", body.transforms.is_some()),
                ("default", body.default.is_some()),
            ],
        );
    }
    if body.array.is_some() {
        return Err(ConversionError::ArrayWithoutScript {
            field: body.field.to_string(),
        });
    }
    let column = match body.column {
        Some(column) => column,
        None => default_column(&body.field)?,
    };
    let tag = scalar_tag(&ty);
    let subfields = match body.subfields {
        Some(_)
            if !matches!(
                ty,
                FlussoType::Text | FlussoType::Keyword | FlussoType::Uuid
            ) =>
        {
            return Err(ConversionError::SubfieldsNotAllowed {
                tag,
                field: body.field.to_string(),
            });
        }
        subfields => subfields.unwrap_or(true),
    };
    let indexed = match body.indexed {
        Some(_) if ty != FlussoType::Json => {
            return Err(ConversionError::IndexedNotAllowed {
                tag,
                field: body.field.to_string(),
            });
        }
        indexed => indexed.unwrap_or(true),
    };
    if !indexed && !body.options.is_empty() {
        return Err(ConversionError::OptionsOnUnindexed {
            field: body.field.to_string(),
        });
    }
    let options = convert_options(body.options);
    let (ty, options) = resolve_column_type(ty, options);
    let transforms = body
        .transforms
        .map(|ts| ts.into_iter().map(convert_transform).collect())
        .unwrap_or_default();
    Ok(Field {
        field: body.field,
        options,
        emit: true,
        source: FieldSource::Column(Column {
            column,
            ty,
            nullable: !body.required,
            transforms,
            default: convert_default(body.default)?,
            enum_order: Vec::new(),
            subfields,
            indexed,
        }),
    })
}

/// A script field's parts, gathered from a scalar or `enum:` body.
struct ScriptParts {
    field: kernel::FieldName,
    ty: FlussoType,
    source: String,
    required: bool,
    array: Option<bool>,
    enum_order: Vec<String>,
    subfields: Option<bool>,
    options: BTreeMap<String, serde_yaml::Value>,
}

/// A script field: a typed leaf computed from the built document. It reads no
/// column, so each column sibling in `column_keys` that was set is an error.
fn convert_script<const N: usize>(
    parts: ScriptParts,
    column_keys: [(&'static str, bool); N],
) -> Result<Field, ConversionError> {
    let tag = scalar_tag(&parts.ty);
    let field = parts.field.to_string();
    if let Some((key, _)) = column_keys.into_iter().find(|(_, set)| *set) {
        return Err(ConversionError::ScriptWithColumnKey { tag, field, key });
    }
    if !is_script_type(&parts.ty) {
        return Err(ConversionError::ScriptTypeNotAllowed { tag, field });
    }
    if parts.source.trim().is_empty() {
        return Err(ConversionError::EmptyScript { field });
    }
    let subfields = match parts.subfields {
        Some(_)
            if !matches!(
                parts.ty,
                FlussoType::Text | FlussoType::Keyword | FlussoType::Uuid
            ) =>
        {
            return Err(ConversionError::SubfieldsNotAllowed { tag, field });
        }
        subfields => subfields.unwrap_or(true),
    };
    let (ty, options) = resolve_column_type(parts.ty, convert_options(parts.options));
    Ok(Field {
        field: parts.field,
        options,
        emit: true,
        source: FieldSource::Script(Script {
            source: parts.source,
            ty,
            nullable: !parts.required,
            array: parts.array.unwrap_or(false),
            enum_order: parts.enum_order,
            subfields,
        }),
    })
}

/// Whether a script may return this type: a leaf with a plain value form.
fn is_script_type(ty: &FlussoType) -> bool {
    matches!(
        ty,
        FlussoType::Text
            | FlussoType::Identifier
            | FlussoType::Keyword
            | FlussoType::Enum
            | FlussoType::Uuid
            | FlussoType::Boolean
            | FlussoType::Short
            | FlussoType::Integer
            | FlussoType::Long
            | FlussoType::Float
            | FlussoType::Double
            | FlussoType::Decimal
            | FlussoType::Date
            | FlussoType::Timestamp
    )
}

/// The YAML type tag a scalar type is written with, for error messages.
fn scalar_tag(ty: &FlussoType) -> String {
    match serde_yaml::to_value(ty) {
        Ok(serde_yaml::Value::String(tag)) => tag,
        _ => "custom".to_owned(),
    }
}

/// Convert a scalar field's `default`, rejecting non-scalar values. A `default`
/// is coalesced into a single column value at read time, so only a scalar (or
/// null) is meaningful; an array/object/binary default would be silently dropped
/// at SQL-build time, so it's an error here instead.
fn convert_default(
    value: Option<serde_yaml::Value>,
) -> Result<Option<GenericValue>, ConversionError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let got = match yaml_to_generic(value) {
        GenericValue::Array(_) => "array",
        GenericValue::Map(_) => "object",
        GenericValue::Bytes(_) => "binary",
        scalar => return Ok(Some(scalar)),
    };
    Err(ConversionError::NonScalarDefault { got })
}

/// A dynamic-key object: a `json`/`jsonb` column read through verbatim, mapped
/// as an OpenSearch `object` with `dynamic: true` so its runtime keys are
/// accepted without enumeration. `values` declares the shared leaf type every
/// key is mapped as and must be a leaf kind (text/keyword/number/date);
/// `dynamic: true` is injected into the field's mapping options (an explicit
/// `dynamic` option wins).
fn convert_map(body: entities::MapBody) -> Result<Field, ConversionError> {
    let values = require_map_value_type(body.values)?;
    let column = match body.column {
        Some(column) => column,
        None => default_column(&body.field)?,
    };
    let mut options = convert_options(body.options);
    options
        .entry("dynamic".to_owned())
        .or_insert(GenericValue::Bool(true));
    Ok(Field {
        field: body.field,
        options,
        emit: true,
        source: FieldSource::Column(Column {
            column,
            ty: FlussoType::Map {
                values: Box::new(values),
            },
            nullable: !body.required,
            transforms: Vec::new(),
            default: None,
            enum_order: Vec::new(),
            subfields: true,
            indexed: true,
        }),
    })
}

/// An `enum:` scalar. Its declared `variants` order (empty for a plain keyword
/// alias) is validated and lands on the column as `enum_order`; everything else
/// is a normal scalar leaf.
fn convert_enum(body: entities::EnumBody) -> Result<Field, ConversionError> {
    validate_enum_order(&body.variants)?;
    if let Some(source) = body.script {
        return convert_script(
            ScriptParts {
                field: body.field,
                ty: FlussoType::Enum,
                source,
                required: body.required,
                array: body.array,
                enum_order: body.variants,
                subfields: None,
                options: body.options,
            },
            [
                ("column", body.column.is_some()),
                ("transforms", body.transforms.is_some()),
                ("default", body.default.is_some()),
            ],
        );
    }
    if body.array.is_some() {
        return Err(ConversionError::ArrayWithoutScript {
            field: body.field.to_string(),
        });
    }
    let column = match body.column {
        Some(column) => column,
        None => default_column(&body.field)?,
    };
    let (ty, options) = resolve_column_type(FlussoType::Enum, convert_options(body.options));
    let transforms = body
        .transforms
        .map(|ts| ts.into_iter().map(convert_transform).collect())
        .unwrap_or_default();
    Ok(Field {
        field: body.field,
        options,
        emit: true,
        source: FieldSource::Column(Column {
            column,
            ty,
            nullable: !body.required,
            transforms,
            default: convert_default(body.default)?,
            enum_order: body.variants,
            subfields: true,
            indexed: true,
        }),
    })
}

/// Validate a declared `enum` order. Empty means "no declared order" (a plain
/// keyword) and is fine. Otherwise every variant must be non-blank and free of
/// `=>` / newlines — it becomes a rule in the OpenSearch `mapping` char filter
/// that prebakes the sort rank — and no variant may repeat.
fn validate_enum_order(order: &[String]) -> Result<(), ConversionError> {
    let mut seen = std::collections::BTreeSet::new();
    for variant in order {
        if variant.trim().is_empty() || variant.contains("=>") || variant.contains('\n') {
            return Err(ConversionError::InvalidEnumVariant {
                variant: variant.clone(),
            });
        }
        if !seen.insert(variant.as_str()) {
            return Err(ConversionError::DuplicateEnumVariant {
                variant: variant.clone(),
            });
        }
    }
    Ok(())
}

/// A `map`'s `values` must be a leaf kind that has a typed query surface —
/// text/keyword (string), a numeric, or a date. `boolean`, `binary`, `json`,
/// `geo_point`, and `custom` are rejected (no value-kind handle, and a nested
/// `map`/`json` defeats the dynamic-object purpose).
fn require_map_value_type(ty: FlussoType) -> Result<FlussoType, ConversionError> {
    match ty {
        FlussoType::Text
        | FlussoType::Identifier
        | FlussoType::Keyword
        | FlussoType::Enum
        | FlussoType::Uuid
        | FlussoType::Short
        | FlussoType::Integer
        | FlussoType::Long
        | FlussoType::Float
        | FlussoType::Double
        | FlussoType::Decimal
        | FlussoType::Date
        | FlussoType::Timestamp => Ok(ty),
        FlussoType::Boolean => Err(ConversionError::InvalidMapValueType { got: "boolean" }),
        FlussoType::Binary => Err(ConversionError::InvalidMapValueType { got: "binary" }),
        FlussoType::Json => Err(ConversionError::InvalidMapValueType { got: "json" }),
        FlussoType::Map { .. } => Err(ConversionError::InvalidMapValueType { got: "map" }),
        FlussoType::GeoPoint => Err(ConversionError::InvalidMapValueType { got: "geo" }),
        FlussoType::Custom { .. } => Err(ConversionError::InvalidMapValueType { got: "custom" }),
    }
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
                enum_order: Vec::new(),
                subfields: true,
                indexed: true,
            })
        }
        _ => return Err(ConversionError::InvalidGeoSource),
    };
    Ok(Field {
        field: body.field,
        options,
        emit: true,
        source,
    })
}

fn convert_join_field(
    verb: entities::JoinVerb,
    body: entities::JoinBody,
) -> Result<Field, ConversionError> {
    let kind = join_kind(verb, &body)?;
    let context = format!("{} `{}`", verb.as_str(), body.field);
    if matches!(kind, JoinKind::ManyToMany { .. }) && has_pairs(body.on.as_deref()) {
        return Err(ConversionError::OnNotAllowed {
            context,
            place: "a `many_to_many` join",
        });
    }
    let on = convert_on(&context, body.on)?;
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
    // `required` only describes a to-one object's presence; a to-many join is
    // inherently a (possibly empty) non-null array, so the knob is meaningless
    // there and is rejected rather than silently ignored.
    let nullable = if kind.is_to_many() {
        if body.required.is_some() {
            return Err(sibling_not_allowed(verb, "required"));
        }
        false
    } else {
        !body.required.unwrap_or(false)
    };
    let indexed = body.indexed.unwrap_or(true);
    if !indexed && !body.options.is_empty() {
        return Err(ConversionError::OptionsOnUnindexed {
            field: body.field.to_string(),
        });
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
        nullable,
        filters: convert_filters_opt(body.filters)?,
        order_by: body
            .order_by
            .map(|os| os.into_iter().map(convert_order_by).collect()),
        limit: body.limit,
        fields,
        indexed,
        on,
    };
    Ok(Field {
        field: body.field,
        options: convert_options(body.options),
        emit: true,
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
                Some(t) => convert_through(t)?,
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
    let (op, value_type) = convert_aggregate_op(op, &body)?;
    let key = aggregate_key(body.foreign_key, body.through)?;
    if body.distinct && matches!(key, AggregateKey::Direct(_)) {
        return Err(ConversionError::DistinctWithoutThrough);
    }
    let context = format!("aggregate `{}`", body.field);
    if matches!(key, AggregateKey::Through(_)) && has_pairs(body.on.as_deref()) {
        return Err(ConversionError::OnNotAllowed {
            context,
            place: "an aggregate over `through`",
        });
    }
    let on = convert_on(&context, body.on)?;
    let via = body
        .via
        .unwrap_or_default()
        .into_iter()
        .map(|hop| convert_via_hop(&body.field, hop))
        .collect::<Result<_, _>>()?;
    let aggregate = Aggregate {
        table: body.table,
        op,
        key,
        value_type,
        filters: convert_filters_opt(body.filters)?,
        distinct: body.distinct,
        via,
        on,
    };
    Ok(Field {
        field: body.field,
        options: convert_options(body.options),
        emit: true,
        source: FieldSource::Relation(Relation::Aggregate(aggregate)),
    })
}

/// `count`/`avg` have fixed result types (`long`/`double`); `sum`/`min`/`max`
/// mirror the aggregated column, so they require a `column` and a `value_type`;
/// `ids` collects the related primary key (no `column`) into an array of an
/// explicit `element_type`, carried on the op itself.
fn convert_aggregate_op(
    op: entities::AggregateOp,
    body: &entities::AggregateBody,
) -> Result<(AggregateOp, Option<FlussoType>), ConversionError> {
    let column = body.column.clone();
    if body.element_type.is_some() && op != entities::AggregateOp::Ids {
        return Err(ConversionError::UnexpectedAggregateSibling {
            sibling: "element_type",
        });
    }
    Ok(match op {
        entities::AggregateOp::Count => (AggregateOp::Count, None),
        entities::AggregateOp::Avg => (
            AggregateOp::Avg(require_aggregate_column(column, "avg")?),
            None,
        ),
        entities::AggregateOp::Sum => (
            AggregateOp::Sum(require_aggregate_column(column, "sum")?),
            Some(require_aggregate_type(body.value_type.clone(), "sum")?),
        ),
        entities::AggregateOp::Min => (
            AggregateOp::Min(require_aggregate_column(column, "min")?),
            Some(require_aggregate_type(body.value_type.clone(), "min")?),
        ),
        entities::AggregateOp::Max => (
            AggregateOp::Max(require_aggregate_column(column, "max")?),
            Some(require_aggregate_type(body.value_type.clone(), "max")?),
        ),
        entities::AggregateOp::Exists => {
            if body.column.is_some() {
                return Err(ConversionError::UnexpectedExistsSibling { sibling: "column" });
            }
            if body.value_type.is_some() {
                return Err(ConversionError::UnexpectedExistsSibling {
                    sibling: "value_type",
                });
            }
            (AggregateOp::Exists, None)
        }
        entities::AggregateOp::Ids => {
            if body.column.is_some() {
                return Err(ConversionError::UnexpectedIdsSibling { sibling: "column" });
            }
            if body.value_type.is_some() {
                return Err(ConversionError::UnexpectedIdsSibling {
                    sibling: "value_type",
                });
            }
            let element_type = require_element_type(body.element_type.clone())?;
            (AggregateOp::Ids { element_type }, None)
        }
    })
}

/// An aggregate key: exactly one of `foreign_key` or `through`.
fn aggregate_key(
    foreign_key: Option<ColumnName>,
    through: Option<entities::Through>,
) -> Result<AggregateKey, ConversionError> {
    match (foreign_key, through) {
        (Some(fk), None) => Ok(AggregateKey::Direct(fk)),
        (None, Some(t)) => Ok(AggregateKey::Through(convert_through(t)?)),
        _ => Err(ConversionError::InvalidAggregateKey),
    }
}

/// A `via` entry: a downward foreign-key hop. A `column` (the `belongs_to`
/// direction) or a `through` (a junction in the middle) is rejected.
fn convert_via_hop(
    field: &kernel::FieldName,
    hop: entities::ViaHop,
) -> Result<ViaHop, ConversionError> {
    let field = field.to_string();
    let table = hop.table.to_string();
    if hop.through.is_some() {
        return Err(ConversionError::ViaJunction { field, table });
    }
    if hop.column.is_some() {
        return Err(ConversionError::ViaBelongsTo { field, table });
    }
    let on = convert_on(
        &format!("aggregate `{field}`, `via` entry `{table}`"),
        hop.on,
    )?;
    Ok(ViaHop {
        foreign_key: hop
            .foreign_key
            .ok_or(ConversionError::ViaMissingForeignKey { field, table })?,
        table: hop.table,
        filters: convert_filters_opt(hop.filters)?,
        on,
    })
}

fn has_pairs(on: Option<&[entities::ColumnPair]>) -> bool {
    on.is_some_and(|pairs| !pairs.is_empty())
}

/// An `on` list: every pair complete and listed once. Absent and `[]` are the
/// same: no extra equality.
fn convert_on(
    context: &str,
    on: Option<Vec<entities::ColumnPair>>,
) -> Result<Vec<ColumnPair>, ConversionError> {
    let mut pairs: Vec<ColumnPair> = Vec::new();
    for pair in on.unwrap_or_default() {
        let (Some(local), Some(related)) = (pair.local, pair.related) else {
            return Err(ConversionError::OnIncompletePair {
                context: context.to_owned(),
            });
        };
        let pair = ColumnPair { local, related };
        if pairs.contains(&pair) {
            return Err(ConversionError::OnDuplicatePair {
                context: context.to_owned(),
                pair: format!("`{}` = `{}`", pair.local, pair.related),
            });
        }
        pairs.push(pair);
    }
    Ok(pairs)
}

fn convert_through(through: entities::Through) -> Result<Through, ConversionError> {
    Ok(Through {
        table: through.table,
        left_key: through.left_key,
        right_key: through.right_key,
        filters: convert_filters_opt(through.filters)?,
    })
}

fn convert_options(options: BTreeMap<String, serde_yaml::Value>) -> BTreeMap<String, GenericValue> {
    options
        .into_iter()
        .map(|(k, v)| (k, yaml_to_generic(v)))
        .collect()
}

/// Apply the type's option defaults ([`FlussoType::default_options`]); an
/// explicit option wins.
fn resolve_column_type(
    ty: FlussoType,
    mut options: BTreeMap<String, GenericValue>,
) -> (FlussoType, BTreeMap<String, GenericValue>) {
    for (key, value) in ty.default_options() {
        options.entry(key).or_insert(value);
    }
    (ty, options)
}

/// The column a field reads from when none is given: the field name itself.
/// `ColumnName` lowercases, matching Postgres's folding of unquoted identifiers;
/// a field name that isn't a valid column identifier must set `column`.
fn default_column(field: &kernel::FieldName) -> Result<ColumnName, ConversionError> {
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

/// The `element_type` of an `ids` aggregate — the scalar type of each collected
/// primary key. Required and scalar; `geo_point` and `custom` are rejected.
fn require_element_type(ty: Option<FlussoType>) -> Result<FlussoType, ConversionError> {
    match ty.ok_or(ConversionError::MissingElementType)? {
        FlussoType::GeoPoint | FlussoType::Custom { .. } => {
            Err(ConversionError::InvalidElementType)
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
                GenericValue::BigInt(i)
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

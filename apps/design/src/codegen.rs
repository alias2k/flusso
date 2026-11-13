//! Canonical regeneration: an in-memory model → the files flusso reads.
//!
//! The designer's model *is* the validated vocabulary — [`IndexSchema`] for a
//! `*.schema.yml`, [`ConfigToml`] for `flusso.toml` — so codegen is the inverse
//! of parsing: it emits the type-first YAML grammar (`- keyword: email`, join
//! verbs, aggregate ops) and the TOML wiring.
//!
//! Output is **canonical**, not format-preserving: a deterministic, tidy layout
//! derived from the model. Hand-written comments and incidental formatting are
//! not retained (that was the design choice for this surface). What *is*
//! guaranteed is a clean round-trip of meaning — [`schema_to_yaml`]'s output
//! parses back to a schema that resolves to the same mapping, which the
//! `roundtrips_*` tests assert against the real parser.
//!
//! ```
//! # use kernel::IndexSchema;
//! # fn demo(schema: &IndexSchema) -> anyhow::Result<()> {
//! let yaml = design::codegen::schema_to_yaml(schema)?;
//! assert!(yaml.starts_with("version:"));
//! # Ok(())
//! # }
//! ```

use anyhow::Result;
use config::toml::ConfigToml;
use kernel::{
    Aggregate, AggregateKey, AggregateOp, Column, ColumnPair, Field, FieldSource, Filter,
    FilterValue, Geo, IndexSchema, Join, JoinKind, NullOp, OrderBy, Relation, Script, SoftDelete,
    Through, Transform, ViaHop,
};
use kernel::{FlussoType, GenericValue};
use rust_decimal::Decimal;
use serde_yaml::{Mapping, Value};

/// Render an [`IndexSchema`] as a type-first `*.schema.yml` document.
pub fn schema_to_yaml(schema: &IndexSchema) -> Result<String> {
    let mut root = Mapping::new();
    insert(&mut root, "version", Value::from(schema.version));
    insert(&mut root, "table", Value::from(schema.table.as_ref()));
    if schema.db_schema.as_ref() != "public" {
        insert(&mut root, "schema", Value::from(schema.db_schema.as_ref()));
    }
    if let Some(pk) = &schema.primary_key {
        insert(&mut root, "primary_key", Value::from(pk.as_ref()));
    }
    if let Some(sd) = &schema.soft_delete {
        insert(&mut root, "soft_delete", soft_delete_value(sd)?);
    }
    if let Some(filters) = &schema.filters {
        insert(&mut root, "filters", filters_value(filters)?);
    }
    insert(&mut root, "fields", fields_value(&schema.fields)?);

    Ok(serde_yaml::to_string(&Value::Mapping(root))?)
}

/// Render a [`ConfigToml`] as a `flusso.toml` document.
pub fn config_to_toml(config: &ConfigToml) -> Result<String> {
    Ok(toml::to_string(config)?)
}

fn fields_value(fields: &[Field]) -> Result<Value> {
    let mut out = Vec::with_capacity(fields.len());
    for field in fields {
        out.push(field_value(field)?);
    }
    Ok(Value::Sequence(out))
}

fn field_value(field: &Field) -> Result<Value> {
    let name = field.field.as_ref();
    let mut body = Mapping::new();
    let tag = match &field.source {
        FieldSource::Column(column) => column_field(name, column, &mut body)?,
        FieldSource::Geo(geo) => geo_field(geo, &mut body),
        FieldSource::Group(fields) => {
            body.insert(Value::from("fields"), fields_value(fields)?);
            "object"
        }
        FieldSource::Relation(Relation::Join(join)) => join_field(join, &mut body)?,
        FieldSource::Relation(Relation::Aggregate(agg)) => aggregate_field(agg, &mut body)?,
        FieldSource::Constant(value) => {
            body.insert(Value::from("value"), generic_value(value)?);
            "constant"
        }
        FieldSource::Script(script) => script_field(script, &mut body)?,
    };
    if !field.options.is_empty() {
        body.insert(Value::from("options"), options_value(&field.options)?);
    }
    if !field.emit {
        body.insert(Value::from("emit"), Value::from(false));
    }
    // The tag's value is the document key; everything else is a sibling. Build a
    // single-entry mapping with the tag first so the file reads `- <type>: name`.
    let mut field_map = Mapping::new();
    field_map.insert(Value::from(tag), Value::from(name));
    for (k, v) in body {
        field_map.insert(k, v);
    }
    Ok(Value::Mapping(field_map))
}

fn column_field(name: &str, column: &Column, body: &mut Mapping) -> Result<&'static str> {
    let tag = match &column.ty {
        FlussoType::Map { values } => {
            body.insert(Value::from("values"), serde_yaml::to_value(values)?);
            "map"
        }
        FlussoType::Custom {
            postgres,
            opensearch,
        } => {
            body.insert(Value::from("postgres"), serde_yaml::to_value(postgres)?);
            body.insert(Value::from("opensearch"), Value::from(opensearch.as_str()));
            "custom"
        }
        ty => scalar_tag(ty),
    };
    // `column` defaults to the field name (lowercased); emit only when it differs.
    if column.column.as_ref() != name {
        body.insert(Value::from("column"), Value::from(column.column.as_ref()));
    }
    body.insert(Value::from("required"), Value::from(!column.nullable));
    // An enum's declared variant order (empty for a bare enum / non-enum column).
    if !column.enum_order.is_empty() {
        body.insert(
            Value::from("variants"),
            serde_yaml::to_value(&column.enum_order)?,
        );
    }
    if !column.transforms.is_empty() {
        body.insert(
            Value::from("transforms"),
            transforms_value(&column.transforms),
        );
    }
    // `default: null` reads back as no default, which is what a null default means.
    if let Some(default) = column
        .default
        .as_ref()
        .filter(|d| !matches!(d, GenericValue::Null))
    {
        body.insert(Value::from("default"), generic_value(default)?);
    }
    if !column.subfields {
        body.insert(Value::from("subfields"), Value::from(false));
    }
    if !column.indexed {
        body.insert(Value::from("indexed"), Value::from(false));
    }
    Ok(tag)
}

fn script_field(script: &Script, body: &mut Mapping) -> Result<&'static str> {
    body.insert(Value::from("required"), Value::from(!script.nullable));
    if !script.enum_order.is_empty() {
        body.insert(
            Value::from("variants"),
            serde_yaml::to_value(&script.enum_order)?,
        );
    }
    if script.array {
        body.insert(Value::from("array"), Value::from(true));
    }
    if !script.subfields {
        body.insert(Value::from("subfields"), Value::from(false));
    }
    body.insert(Value::from("script"), Value::from(script.source.as_str()));
    Ok(scalar_tag(&script.ty))
}

fn geo_field(geo: &Geo, body: &mut Mapping) -> &'static str {
    body.insert(Value::from("lat"), Value::from(geo.lat.as_ref()));
    body.insert(Value::from("lon"), Value::from(geo.lon.as_ref()));
    body.insert(Value::from("required"), Value::from(!geo.nullable));
    "geo"
}

fn join_field(join: &Join, body: &mut Mapping) -> Result<&'static str> {
    body.insert(Value::from("table"), Value::from(join.table.as_ref()));
    body.insert(
        Value::from("primary_key"),
        Value::from(join.primary_key.as_ref()),
    );
    let (tag, to_many) = match &join.kind {
        JoinKind::BelongsTo { column } => {
            body.insert(Value::from("column"), Value::from(column.to_string()));
            ("belongs_to", false)
        }
        JoinKind::HasOne { foreign_key } => {
            body.insert(
                Value::from("foreign_key"),
                Value::from(foreign_key.as_ref()),
            );
            ("has_one", false)
        }
        JoinKind::HasMany { foreign_key } => {
            body.insert(
                Value::from("foreign_key"),
                Value::from(foreign_key.as_ref()),
            );
            ("has_many", true)
        }
        JoinKind::ManyToMany { through } => {
            body.insert(Value::from("through"), through_value(through)?);
            ("many_to_many", true)
        }
    };
    // `required` describes a to-one object's presence; a to-many join rejects it.
    if !to_many {
        body.insert(Value::from("required"), Value::from(!join.nullable));
    }
    if let Some(filters) = &join.filters {
        body.insert(Value::from("filters"), filters_value(filters)?);
    }
    if let Some(order_by) = &join.order_by {
        body.insert(Value::from("order_by"), order_by_value(order_by));
    }
    if let Some(limit) = join.limit {
        body.insert(Value::from("limit"), Value::from(limit));
    }
    if !join.indexed {
        body.insert(Value::from("indexed"), Value::from(false));
    }
    insert_on(body, &join.on);
    body.insert(Value::from("fields"), fields_value(&join.fields)?);
    Ok(tag)
}

fn aggregate_field(agg: &Aggregate, body: &mut Mapping) -> Result<&'static str> {
    body.insert(Value::from("table"), Value::from(agg.table.as_ref()));
    if let Some(input) = agg.op.input() {
        body.insert(Value::from("column"), Value::from(input.to_string()));
    }
    let tag = match &agg.op {
        AggregateOp::Count => "count",
        AggregateOp::Exists => "exists",
        AggregateOp::Avg(_) => "avg",
        AggregateOp::Sum(_) => "sum",
        AggregateOp::Min(_) => "min",
        AggregateOp::Max(_) => "max",
        AggregateOp::Ids { element_type } => {
            body.insert(
                Value::from("element_type"),
                serde_yaml::to_value(element_type)?,
            );
            "ids"
        }
    };
    // `sum`/`min`/`max` mirror their column's type and carry it explicitly;
    // `count`/`avg` have fixed result types and `ids` uses `element_type`.
    if let Some(value_type) = &agg.value_type {
        body.insert(Value::from("value_type"), serde_yaml::to_value(value_type)?);
    }
    match &agg.key {
        AggregateKey::Direct(fk) => {
            body.insert(Value::from("foreign_key"), Value::from(fk.as_ref()));
        }
        AggregateKey::Through(through) => {
            body.insert(Value::from("through"), through_value(through)?);
        }
    }
    if agg.distinct {
        body.insert(Value::from("distinct"), Value::from(true));
    }
    if let Some(filters) = &agg.filters {
        body.insert(Value::from("filters"), filters_value(filters)?);
    }
    if !agg.via.is_empty() {
        let hops = agg.via.iter().map(via_value).collect::<Result<_>>()?;
        body.insert(Value::from("via"), Value::Sequence(hops));
    }
    insert_on(body, &agg.on);
    Ok(tag)
}

/// `on:` as a list of `{ local, related }` flow maps, left out when empty.
fn insert_on(body: &mut Mapping, on: &[ColumnPair]) {
    if on.is_empty() {
        return;
    }
    let pairs = on
        .iter()
        .map(|pair| {
            let mut map = Mapping::new();
            map.insert(Value::from("local"), Value::from(pair.local.as_ref()));
            map.insert(Value::from("related"), Value::from(pair.related.as_ref()));
            Value::Mapping(map)
        })
        .collect();
    body.insert(Value::from("on"), Value::Sequence(pairs));
}

fn via_value(hop: &ViaHop) -> Result<Value> {
    let mut map = Mapping::new();
    map.insert(Value::from("table"), Value::from(hop.table.as_ref()));
    map.insert(
        Value::from("foreign_key"),
        Value::from(hop.foreign_key.as_ref()),
    );
    if let Some(filters) = &hop.filters {
        map.insert(Value::from("filters"), filters_value(filters)?);
    }
    insert_on(&mut map, &hop.on);
    Ok(Value::Mapping(map))
}

fn through_value(through: &Through) -> Result<Value> {
    let mut map = Mapping::new();
    map.insert(Value::from("table"), Value::from(through.table.as_ref()));
    map.insert(
        Value::from("left_key"),
        Value::from(through.left_key.as_ref()),
    );
    map.insert(
        Value::from("right_key"),
        Value::from(through.right_key.as_ref()),
    );
    if let Some(filters) = &through.filters {
        map.insert(Value::from("filters"), filters_value(filters)?);
    }
    Ok(Value::Mapping(map))
}

fn order_by_value(order_by: &[OrderBy]) -> Value {
    Value::Sequence(
        order_by
            .iter()
            .map(|ob| {
                let mut map = Mapping::new();
                map.insert(Value::from("column"), Value::from(ob.column.as_ref()));
                if let Some(direction) = ob.direction {
                    let token = match direction {
                        kernel::Direction::Asc => "asc",
                        kernel::Direction::Desc => "desc",
                    };
                    map.insert(Value::from("direction"), Value::from(token));
                }
                Value::Mapping(map)
            })
            .collect(),
    )
}

fn transforms_value(transforms: &[Transform]) -> Value {
    Value::Sequence(
        transforms
            .iter()
            .map(|t| {
                Value::from(match t {
                    Transform::Lowercase => "lowercase",
                    Transform::Trim => "trim",
                })
            })
            .collect(),
    )
}

fn filters_value(filters: &[Filter]) -> Result<Value> {
    let mut out = Vec::with_capacity(filters.len());
    for filter in filters {
        out.push(filter_value(filter)?);
    }
    Ok(Value::Sequence(out))
}

fn filter_value(filter: &Filter) -> Result<Value> {
    let mut map = Mapping::new();
    match filter {
        Filter::Raw(raw) => {
            map.insert(Value::from("raw"), Value::from(raw.raw.as_ref()));
        }
        Filter::NullCheck(null) => {
            map.insert(Value::from("column"), Value::from(null.column.as_ref()));
            map.insert(
                Value::from("op"),
                Value::from(match null.op {
                    NullOp::IsNull => "is_null",
                    NullOp::IsNotNull => "is_not_null",
                }),
            );
        }
        Filter::ValueOp(value_op) => {
            map.insert(Value::from("column"), Value::from(value_op.column.as_ref()));
            map.insert(Value::from("op"), serde_yaml::to_value(value_op.op)?);
            map.insert(Value::from("value"), filter_operand(&value_op.value));
        }
    }
    Ok(Value::Mapping(map))
}

fn filter_operand(value: &FilterValue) -> Value {
    match value {
        FilterValue::Single(s) => Value::from(s.as_str()),
        FilterValue::List(items) => {
            Value::Sequence(items.iter().map(|s| Value::from(s.as_str())).collect())
        }
        FilterValue::Range(lo, hi) => {
            Value::Sequence(vec![Value::from(lo.as_str()), Value::from(hi.as_str())])
        }
    }
}

fn soft_delete_value(soft_delete: &SoftDelete) -> Result<Value> {
    let mut map = Mapping::new();
    let when = match soft_delete {
        SoftDelete::Field(field) => {
            map.insert(Value::from("field"), Value::from(field.field.as_ref()));
            &field.when
        }
        SoftDelete::Column(column) => {
            map.insert(Value::from("column"), Value::from(column.column.as_ref()));
            &column.when
        }
    };
    if let Some(when) = when {
        map.insert(Value::from("when"), filters_value(when)?);
    }
    Ok(Value::Mapping(map))
}

fn options_value(options: &std::collections::BTreeMap<String, GenericValue>) -> Result<Value> {
    let mut map = Mapping::new();
    for (k, v) in options {
        map.insert(Value::from(k.as_str()), generic_value(v)?);
    }
    Ok(Value::Mapping(map))
}

/// A scalar value as its saved file spells it (`1000.0`, `'007'`, `null`).
pub(crate) fn scalar_text(value: &GenericValue) -> Result<String> {
    Ok(serde_yaml::to_string(&generic_value(value)?)?
        .trim_end()
        .to_owned())
}

/// A value as plain YAML — no `!Tag` — in the form the parser reads back as the
/// same [`GenericValue`].
///
/// The parser yields only `Null`, `Bool`, `BigInt` (an integer in `i64`),
/// `Decimal` (any other number, through `f64`), `String`, `Array` and `Map`; a
/// string that would read as another kind is quoted (`'123'`, `'null'`).
/// Variants it never yields are written as the nearest form it reads: the
/// narrower integers and floats as numbers, temporals and `Uuid` as strings,
/// `Bytes` as a list of integers.
fn generic_value(value: &GenericValue) -> Result<Value> {
    Ok(match value {
        GenericValue::Null => Value::Null,
        GenericValue::Bool(b) => Value::Bool(*b),
        GenericValue::SmallInt(n) => Value::from(*n),
        GenericValue::Int(n) => Value::from(*n),
        GenericValue::BigInt(n) => Value::from(*n),
        GenericValue::Float(n) => Value::from(f64::from(*n)),
        GenericValue::Double(n) => Value::from(*n),
        GenericValue::Decimal(d) => decimal_value(d),
        GenericValue::String(s) => Value::from(s.as_str()),
        GenericValue::Uuid(v) => serde_yaml::to_value(v)?,
        GenericValue::Date(v) => serde_yaml::to_value(v)?,
        GenericValue::Time(v) => serde_yaml::to_value(v)?,
        GenericValue::Timestamp(v) => serde_yaml::to_value(v)?,
        GenericValue::TimestampTz(v) => serde_yaml::to_value(v)?,
        GenericValue::Bytes(v) => serde_yaml::to_value(v)?,
        GenericValue::Array(items) => Value::Sequence(
            items
                .iter()
                .map(generic_value)
                .collect::<Result<Vec<_>>>()?,
        ),
        GenericValue::Map(map) => {
            let mut out = Mapping::new();
            for (k, v) in map {
                out.insert(Value::from(k.as_str()), generic_value(v)?);
            }
            Value::Mapping(out)
        }
    })
}

/// A decimal as a YAML number. The parser reads an integer past `i64` as that
/// decimal and any other non-`i64` number through `f64` (`Decimal` of its
/// shortest text), so a decimal it produced is written back as exactly that
/// integer or `f64`. A decimal it can't produce is written as near as a YAML
/// number reads back: a whole number `f64` can't hold exactly as the integer
/// (reading back as a `BigInt`), anything else (a trailing zero, more digits
/// than `f64` holds) at `f64` precision. One the parser rounded to 28 places
/// reads back equal, possibly at another scale.
fn decimal_value(d: &Decimal) -> Value {
    let text = d.to_string();
    let float = text.parse::<f64>().unwrap_or(f64::NAN);
    if let Ok(n) = text.parse::<u64>()
        && n > i64::MAX.unsigned_abs()
    {
        return Value::from(n);
    }
    if let Ok(n) = text.parse::<i64>()
        && float.to_string() != text
    {
        return Value::from(n);
    }
    Value::from(float)
}

fn insert(map: &mut Mapping, key: &str, value: Value) {
    map.insert(Value::from(key), value);
}

/// The YAML type tag for a scalar [`FlussoType`]. `Map`/`Custom`/`GeoPoint` are
/// handled by their own field forms, never here.
fn scalar_tag(ty: &FlussoType) -> &'static str {
    match ty {
        FlussoType::Text => "text",
        FlussoType::Identifier => "identifier",
        FlussoType::Keyword => "keyword",
        FlussoType::Enum => "enum",
        FlussoType::Uuid => "uuid",
        FlussoType::Boolean => "boolean",
        FlussoType::Short => "short",
        FlussoType::Integer => "integer",
        FlussoType::Long => "long",
        FlussoType::Float => "float",
        FlussoType::Double => "double",
        FlussoType::Decimal => "decimal",
        FlussoType::Date => "date",
        FlussoType::Timestamp => "timestamp",
        FlussoType::Binary => "binary",
        FlussoType::Json => "json",
        // Handled before reaching here; map to a stable tag rather than panic.
        FlussoType::Map { .. } => "map",
        FlussoType::GeoPoint => "geo",
        FlussoType::Custom { .. } => "custom",
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use config::yaml::SchemaYaml;
    use kernel::ParseFrom;

    /// Parse a `*.schema.yml` body into the validated [`IndexSchema`].
    fn parse(yaml: &str) -> IndexSchema {
        let entity = SchemaYaml::try_parse(yaml).unwrap();
        IndexSchema::try_from(entity).unwrap()
    }

    /// The core round-trip guarantee: emitting a schema and re-parsing it yields
    /// a structurally identical schema. Run against every shipped dev schema —
    /// they exercise scalars, objects, joins, aggregates, `order_by`, and
    /// soft-delete between them.
    fn assert_roundtrips(original_yaml: &str) {
        let schema = parse(original_yaml);
        let generated = schema_to_yaml(&schema).unwrap();
        let reparsed = parse(&generated);

        assert_eq!(
            serde_json::to_value(&schema).unwrap(),
            serde_json::to_value(&reparsed).unwrap(),
            "regenerated schema differs from the original\n--- generated ---\n{generated}",
        );
    }

    #[test]
    fn roundtrips_orders_schema() {
        assert_roundtrips(include_str!("../../../dev/orders.schema.yml"));
    }

    #[test]
    fn roundtrips_products_schema() {
        assert_roundtrips(include_str!("../../../dev/products.schema.yml"));
    }

    const VALUES: &str = "version: 1\ntable: users\nprimary_key: id\nfields:\n\
         - constant: nothing\n  value: null\n\
         - constant: label\n  value: hi\n\
         - constant: rate\n  value: 1.5\n\
         - constant: scaled\n  value: 1.50\n\
         - constant: huge\n  value: 12345678901234567890\n\
         - constant: exp\n  value: 1e3\n\
         - constant: yes\n  value: true\n\
         - constant: count\n  value: -7\n\
         - constant: word_null\n  value: 'null'\n\
         - constant: digits\n  value: '123'\n\
         - constant: nested\n  value: { a: [1, 0.5, 'true'], 'null': ~ }\n\
         - keyword: email\n  required: false\n  default: null\n\
         - double: score\n  required: true\n  default: 0.1\n  options: { boost: 2.5, null_value: 'NaN' }\n";

    #[test]
    fn roundtrips_every_value_kind_as_plain_yaml() {
        assert_roundtrips(VALUES);
        let generated = schema_to_yaml(&parse(VALUES)).unwrap();
        assert!(!generated.contains('!'), "a tag was written:\n{generated}");
        for line in [
            "value: 1.5\n",
            "value: 12345678901234567890\n",
            "value: 1000.0\n",
            "value: 'null'\n",
            "value: '123'\n",
            "default: 0.1\n",
        ] {
            assert!(generated.contains(line), "missing {line:?}:\n{generated}");
        }
    }

    /// What every designer Save wrote before plain values: a tagged decimal,
    /// which the parser reads as a string, so a numeric constant became text.
    #[test]
    fn a_tagged_decimal_reads_back_as_a_string() {
        let tagged = serde_yaml::to_string(&GenericValue::Decimal("1.5".parse().unwrap())).unwrap();
        assert_eq!(tagged.trim(), "!Decimal '1.5'");
        let schema = parse(&format!(
            "version: 1\ntable: t\nprimary_key: id\nfields:\n- constant: rate\n  value: {tagged}"
        ));
        let FieldSource::Constant(value) = &schema.fields[0].source else {
            panic!("not a constant");
        };
        assert!(matches!(value, GenericValue::String(s) if s == "1.5"));
    }

    /// A file a released designer wrote, tags and all, still loads to the same
    /// schema and content hash as its plain spelling; a tagged decimal keeps
    /// reading as the string it always read as.
    #[test]
    fn a_tagged_file_keeps_its_meaning_and_content_hash() {
        let index = kernel::common::IndexName::try_new("users").unwrap();
        let head = "version: 1\ntable: users\nprimary_key: id\nfields:\n";
        let tagged = parse(&format!(
            "{head}- constant: a\n  value: !String hi\n- constant: b\n  value: !BigInt 5\n\
             - constant: c\n  value: !Bool true\n- constant: d\n  value: !Decimal '1.5'\n\
             - keyword: e\n  required: false\n  default: !String x\n  options:\n    boost: !Double 2.5\n"
        ));
        let plain = parse(&format!(
            "{head}- constant: a\n  value: hi\n- constant: b\n  value: 5\n\
             - constant: c\n  value: true\n- constant: d\n  value: '1.5'\n\
             - keyword: e\n  required: false\n  default: x\n  options:\n    boost: 2.5\n"
        ));
        assert_eq!(
            serde_json::to_value(&tagged).unwrap(),
            serde_json::to_value(&plain).unwrap()
        );
        assert_eq!(
            tagged.resolve(index.clone()).hash,
            plain.resolve(index).hash
        );
        assert_roundtrips(&schema_to_yaml(&tagged).unwrap());
    }

    #[test]
    fn a_whole_decimal_past_f64_is_written_as_its_integer() {
        let mut schema =
            parse("version: 1\ntable: t\nprimary_key: id\nfields:\n- constant: c\n  value: 1\n");
        schema.fields[0].source =
            FieldSource::Constant(GenericValue::Decimal("9007199254740993".parse().unwrap()));
        assert!(
            schema_to_yaml(&schema)
                .unwrap()
                .contains("value: 9007199254740993\n")
        );
    }

    #[test]
    fn a_null_default_is_left_out() {
        let mut schema = parse(
            "version: 1\ntable: t\nprimary_key: id\nfields:\n- keyword: k\n  required: false\n",
        );
        if let FieldSource::Column(column) = &mut schema.fields[0].source {
            column.default = Some(GenericValue::Null);
        }
        assert!(!schema_to_yaml(&schema).unwrap().contains("default"));
    }

    #[test]
    fn roundtrips_users_schema() {
        assert_roundtrips(include_str!("../../../dev/users.schema.yml"));
    }

    #[test]
    fn roundtrips_dev_config_toml() {
        let raw = include_str!("../../../dev/flusso.toml");
        let config = ConfigToml::try_parse(raw).unwrap();
        let regenerated = config_to_toml(&config).unwrap();
        let reparsed = ConfigToml::try_parse(&regenerated).unwrap();
        assert_eq!(
            serde_json::to_value(&config).unwrap(),
            serde_json::to_value(&reparsed).unwrap(),
            "regenerated flusso.toml differs\n--- generated ---\n{regenerated}",
        );
    }

    #[test]
    fn roundtrips_per_index_sink_options() {
        let raw = r#"
            [source]
            type = "postgres"

            [sinks.primary]
            type = "opensearch"
            url = "https://search:9200"
            index_settings = { "mapping.total_fields.limit" = 5000 }

            [[index]]
            name = "orders"
            schema = "orders.schema.yml"
            enabled = true

            [index.sinks.primary]
            index_settings = { "mapping.nested_objects.limit" = 20000 }
        "#;
        let config = ConfigToml::try_parse(raw).unwrap();
        let regenerated = config_to_toml(&config).unwrap();
        let reparsed = ConfigToml::try_parse(&regenerated).unwrap();
        assert_eq!(
            serde_json::to_value(&config).unwrap(),
            serde_json::to_value(&reparsed).unwrap(),
            "regenerated flusso.toml differs\n--- generated ---\n{regenerated}",
        );
        assert!(!reparsed.index[0].sinks.is_empty());
    }

    #[test]
    fn emits_type_first_form() {
        let schema = parse(include_str!("../../../dev/orders.schema.yml"));
        let yaml = schema_to_yaml(&schema).unwrap();
        assert!(yaml.contains("- integer: id"));
        assert!(yaml.contains("- has_many: items"));
        assert!(yaml.contains("- count: itemCount"));
    }
}

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
//! # use schema_core::IndexSchema;
//! # fn demo(schema: &IndexSchema) -> anyhow::Result<()> {
//! let yaml = design::codegen::schema_to_yaml(schema)?;
//! assert!(yaml.starts_with("version:"));
//! # Ok(())
//! # }
//! ```

use anyhow::Result;
use schema_config_toml::ConfigToml;
use schema_core::{
    Aggregate, AggregateKey, AggregateOp, Column, Field, FieldSource, Filter, FilterValue, Geo,
    IndexSchema, Join, JoinKind, NullOp, OrderBy, Relation, SoftDelete, Through, Transform,
};
use schema_core::{FlussoType, GenericValue};
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
            body.insert(Value::from("value"), serde_yaml::to_value(value)?);
            "constant"
        }
    };
    if !field.options.is_empty() {
        body.insert(Value::from("options"), options_value(&field.options)?);
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
    if !column.transforms.is_empty() {
        body.insert(
            Value::from("transforms"),
            transforms_value(&column.transforms),
        );
    }
    if let Some(default) = &column.default {
        body.insert(Value::from("default"), serde_yaml::to_value(default)?);
    }
    Ok(tag)
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
            body.insert(Value::from("column"), Value::from(column.as_ref()));
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
            body.insert(Value::from("through"), through_value(through));
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
    body.insert(Value::from("fields"), fields_value(&join.fields)?);
    Ok(tag)
}

fn aggregate_field(agg: &Aggregate, body: &mut Mapping) -> Result<&'static str> {
    body.insert(Value::from("table"), Value::from(agg.table.as_ref()));
    let tag = match &agg.op {
        AggregateOp::Count => "count",
        AggregateOp::Avg(column) => {
            body.insert(Value::from("column"), Value::from(column.as_ref()));
            "avg"
        }
        AggregateOp::Sum(column) => {
            body.insert(Value::from("column"), Value::from(column.as_ref()));
            "sum"
        }
        AggregateOp::Min(column) => {
            body.insert(Value::from("column"), Value::from(column.as_ref()));
            "min"
        }
        AggregateOp::Max(column) => {
            body.insert(Value::from("column"), Value::from(column.as_ref()));
            "max"
        }
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
            body.insert(Value::from("through"), through_value(through));
        }
    }
    if let Some(filters) = &agg.filters {
        body.insert(Value::from("filters"), filters_value(filters)?);
    }
    Ok(tag)
}

fn through_value(through: &Through) -> Value {
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
    Value::Mapping(map)
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
                        schema_core::Direction::Asc => "asc",
                        schema_core::Direction::Desc => "desc",
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
        map.insert(Value::from(k.as_str()), serde_yaml::to_value(v)?);
    }
    Ok(Value::Mapping(map))
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
    use schema_core::ParseFrom;
    use schema_index_yaml::SchemaYaml;

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
    fn emits_type_first_form() {
        let schema = parse(include_str!("../../../dev/orders.schema.yml"));
        let yaml = schema_to_yaml(&schema).unwrap();
        assert!(yaml.contains("- integer: id"));
        assert!(yaml.contains("- has_many: items"));
        assert!(yaml.contains("- count: itemCount"));
    }
}

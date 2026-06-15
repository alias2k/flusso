//! The YAML field form: `- <type>: <name>` with type-specific siblings.
//!
//! The field's **type is the key** — `keyword: email`, `text: bio`,
//! `geo: location`, `has_many: orders`, `count: orderCount`. The key's value
//! is the document key; the sibling keys are whatever that type allows.
//!
//! Each variant owns exactly one body struct that derives [`Deserialize`] with
//! `deny_unknown_fields`, so a field's data lives in one place. A custom
//! `Deserialize` for [`Field`] finds the one recognized type tag among the
//! mapping's keys (order-independent, and a typo'd tag is reported), moves its
//! value under a `field` key, and deserializes the rest straight into that body.

use std::collections::BTreeMap;

use serde::de::{self, DeserializeOwned};
use serde::{Deserialize, Deserializer};

use schema_core::FlussoType;
use schema_core::common;

use super::{AggregateOp, Filter, JoinVerb, OrderBy, Through, Transform};

/// One field of a document, parsed from the type-as-key form. Each variant
/// carries the type/op the tag denoted plus a body holding the rest.
#[derive(Debug, Clone)]
pub enum Field {
    /// A scalar leaf reading a column, with its declared [`FlussoType`].
    Scalar(FlussoType, ScalarBody),
    /// A geographic point (`geo:`).
    Geo(GeoBody),
    /// A same-row sub-object (`object:`).
    Object(ObjectBody),
    /// A related table folded in, with its relationship verb.
    Join(JoinVerb, Box<JoinBody>),
    /// A rollup over a related table, with its operation.
    Aggregate(AggregateOp, Box<AggregateBody>),
    /// A constant value (`constant:`).
    Constant(ConstantBody),
}

/// A scalar leaf. `required` is mandatory; `column` defaults to the field name.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScalarBody {
    pub field: common::FieldName,
    #[serde(default)]
    pub column: Option<common::ColumnName>,
    pub required: bool,
    #[serde(default)]
    pub options: BTreeMap<String, serde_yaml::Value>,
    #[serde(default)]
    pub transforms: Option<Vec<Transform>>,
    #[serde(default)]
    pub default: Option<serde_yaml::Value>,
}

/// A `custom:` scalar — an explicit Postgres/OpenSearch type pair. Converted
/// into a [`Field::Scalar`] with a [`FlussoType::Custom`].
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomBody {
    pub field: common::FieldName,
    pub postgres: Vec<String>,
    pub opensearch: String,
    #[serde(default)]
    pub column: Option<common::ColumnName>,
    pub required: bool,
    #[serde(default)]
    pub options: BTreeMap<String, serde_yaml::Value>,
    #[serde(default)]
    pub default: Option<serde_yaml::Value>,
}

/// A geo point: two coordinate columns (`lat`/`lon`), or a single `column`
/// already holding a `geo_point`-shaped value.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoBody {
    pub field: common::FieldName,
    #[serde(default)]
    pub lat: Option<common::ColumnName>,
    #[serde(default)]
    pub lon: Option<common::ColumnName>,
    #[serde(default)]
    pub column: Option<common::ColumnName>,
    pub required: bool,
    #[serde(default)]
    pub options: BTreeMap<String, serde_yaml::Value>,
}

/// A same-row sub-object assembled from nested `fields`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectBody {
    pub field: common::FieldName,
    #[serde(default)]
    pub options: BTreeMap<String, serde_yaml::Value>,
    pub fields: Vec<Field>,
}

/// A join field (its relationship verb is the type key). Which key sibling is
/// allowed depends on the verb: `column` for `belongs_to` (defaulting to the
/// field name), `foreign_key` for `has_one`/`has_many`, `through` for
/// `many_to_many` — enforced at conversion.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JoinBody {
    pub field: common::FieldName,
    pub table: common::TableName,
    pub primary_key: common::ColumnName,
    #[serde(default)]
    pub column: Option<common::ColumnName>,
    #[serde(default)]
    pub foreign_key: Option<common::ColumnName>,
    #[serde(default)]
    pub through: Option<Through>,
    #[serde(default)]
    pub filters: Option<Vec<Filter>>,
    #[serde(default)]
    pub order_by: Option<Vec<OrderBy>>,
    #[serde(default)]
    pub limit: Option<u64>,
    pub fields: Vec<Field>,
    #[serde(default)]
    pub options: BTreeMap<String, serde_yaml::Value>,
}

/// An aggregate field (its op is the type key). `value_type` is required for
/// `sum`/`min`/`max`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AggregateBody {
    pub field: common::FieldName,
    pub table: common::TableName,
    #[serde(default)]
    pub column: Option<common::ColumnName>,
    #[serde(default)]
    pub value_type: Option<FlussoType>,
    #[serde(default)]
    pub foreign_key: Option<common::ColumnName>,
    #[serde(default)]
    pub through: Option<Through>,
    #[serde(default)]
    pub filters: Option<Vec<Filter>>,
    #[serde(default)]
    pub options: BTreeMap<String, serde_yaml::Value>,
}

/// A constant field with no database source.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstantBody {
    pub field: common::FieldName,
    pub value: serde_yaml::Value,
}

// ── custom deserialize: find the type tag, parse the body ────────────────────

/// Which kind of field a type tag denotes.
enum TagKind {
    Scalar(FlussoType),
    Custom,
    Geo,
    Object,
    Join(JoinVerb),
    Aggregate(AggregateOp),
    Constant,
}

/// Classify a key as a field type tag, or `None` if it's an ordinary sibling.
fn classify(key: &str) -> Option<TagKind> {
    if let Some(ty) = scalar_type(key) {
        return Some(TagKind::Scalar(ty));
    }
    Some(match key {
        "custom" => TagKind::Custom,
        "geo" => TagKind::Geo,
        "object" => TagKind::Object,
        "belongs_to" => TagKind::Join(JoinVerb::BelongsTo),
        "has_one" => TagKind::Join(JoinVerb::HasOne),
        "has_many" => TagKind::Join(JoinVerb::HasMany),
        "many_to_many" => TagKind::Join(JoinVerb::ManyToMany),
        "count" => TagKind::Aggregate(AggregateOp::Count),
        "sum" => TagKind::Aggregate(AggregateOp::Sum),
        "avg" => TagKind::Aggregate(AggregateOp::Avg),
        "min" => TagKind::Aggregate(AggregateOp::Min),
        "max" => TagKind::Aggregate(AggregateOp::Max),
        "constant" => TagKind::Constant,
        _ => return None,
    })
}

/// The [`FlussoType`] a named scalar tag denotes (`geo_point` and `custom` are
/// handled separately, as the `geo:` and `custom:` tags).
fn scalar_type(key: &str) -> Option<FlussoType> {
    Some(match key {
        "text" => FlussoType::Text,
        "identifier" => FlussoType::Identifier,
        "keyword" => FlussoType::Keyword,
        "enum" => FlussoType::Enum,
        "uuid" => FlussoType::Uuid,
        "boolean" => FlussoType::Boolean,
        "short" => FlussoType::Short,
        "integer" => FlussoType::Integer,
        "long" => FlussoType::Long,
        "float" => FlussoType::Float,
        "double" => FlussoType::Double,
        "decimal" => FlussoType::Decimal,
        "date" => FlussoType::Date,
        "timestamp" => FlussoType::Timestamp,
        "binary" => FlussoType::Binary,
        "json" => FlussoType::Json,
        _ => return None,
    })
}

/// Find the single recognized type tag among a mapping's keys.
fn find_tag<E: de::Error>(map: &serde_yaml::Mapping) -> Result<(String, TagKind), E> {
    let mut found: Option<(String, TagKind)> = None;
    for (key, _) in map {
        if let serde_yaml::Value::String(key) = key
            && let Some(kind) = classify(key)
        {
            if let Some((previous, _)) = &found {
                return Err(E::custom(format!(
                    "field has more than one type tag: `{previous}` and `{key}`"
                )));
            }
            found = Some((key.clone(), kind));
        }
    }
    found.ok_or_else(|| {
        // Name the field's keys so it can be located — its position is otherwise
        // unreliable (see `render_yaml_error`), and with no tag there's no name.
        let keys: Vec<&str> = map.iter().filter_map(|(key, _)| key.as_str()).collect();
        let here = if keys.is_empty() {
            String::new()
        } else {
            format!(" (this field has keys: {})", keys.join(", "))
        };
        E::custom(format!(
            "field is missing a type tag{here}; expected one of: a scalar type like \
             `keyword`/`text`/`integer`, or `custom`, `geo`, `object`, \
             `belongs_to`/`has_one`/`has_many`/`many_to_many`, \
             `count`/`sum`/`avg`/`min`/`max`, `constant`"
        ))
    })
}

/// Deserialize a body from the field mapping (after the tag value has been moved
/// under the `field` key). On failure the message names the field — its type tag
/// and document key — and hides the internal `field` key we inject, so the user
/// sees only keys they can actually write.
fn body_from<T: DeserializeOwned, E: de::Error>(
    body: serde_yaml::Value,
    tag: &str,
    name: &str,
) -> Result<T, E> {
    serde_yaml::from_value(body).map_err(|e| {
        let detail = e
            .to_string()
            .replace("`field`, ", "")
            .replace(", `field`", "")
            .replace("unknown field", "unknown key")
            .replace("missing field", "missing key");
        E::custom(format!("`{tag}` field `{name}`: {detail}"))
    })
}

/// The document key a field carries (the value of its type tag), for use in
/// error messages. Non-string keys are uncommon but possible to write.
fn field_name(value: &serde_yaml::Value) -> String {
    match value {
        serde_yaml::Value::String(s) => s.clone(),
        other => format!("{other:?}"),
    }
}

impl<'de> Deserialize<'de> for Field {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut map = serde_yaml::Mapping::deserialize(deserializer)?;
        let (tag_key, kind) = find_tag::<D::Error>(&map)?;

        // Move the tag's value (the document key) under `field`, so the body —
        // which owns a `field` member — deserializes the whole mapping directly.
        let name = map
            .remove(serde_yaml::Value::String(tag_key.clone()))
            .ok_or_else(|| de::Error::custom("internal: type tag vanished"))?;
        let name_str = field_name(&name);
        let tag = tag_key.as_str();
        map.insert(serde_yaml::Value::String("field".to_owned()), name);
        let body = serde_yaml::Value::Mapping(map);

        Ok(match kind {
            TagKind::Scalar(ty) => Field::Scalar(ty, body_from(body, tag, &name_str)?),
            TagKind::Custom => {
                let c: CustomBody = body_from(body, tag, &name_str)?;
                Field::Scalar(
                    FlussoType::Custom {
                        postgres: c.postgres,
                        opensearch: c.opensearch,
                    },
                    ScalarBody {
                        field: c.field,
                        column: c.column,
                        required: c.required,
                        options: c.options,
                        transforms: None,
                        default: c.default,
                    },
                )
            }
            TagKind::Geo => Field::Geo(body_from(body, tag, &name_str)?),
            TagKind::Object => Field::Object(body_from(body, tag, &name_str)?),
            TagKind::Join(join_type) => {
                Field::Join(join_type, Box::new(body_from(body, tag, &name_str)?))
            }
            TagKind::Aggregate(op) => {
                Field::Aggregate(op, Box::new(body_from(body, tag, &name_str)?))
            }
            TagKind::Constant => Field::Constant(body_from(body, tag, &name_str)?),
        })
    }
}

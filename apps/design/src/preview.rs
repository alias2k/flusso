//! Deriving the search document's shape from a schema, database-free.
//!
//! The whole point of a visual designer is seeing the document you're building.
//! [`preview`] takes an [`IndexSchema`] and returns its resolved
//! [`IndexMapping`] (the typed OpenSearch mapping flusso would create) plus a
//! friendlier [`DocumentNode`] tree for rendering — both from the declared
//! schema alone, exactly as `flusso check` derives a mapping without touching a
//! database. [`example_document`] goes one step further: it synthesizes a whole
//! example document from those declared types — the fallback the sample preview
//! shows when the root table has no rows to sample.

use schema_core::{IndexMapping, IndexName, IndexSchema, Mapping, MappingType, ResolvedField};
use serde::Serialize;
use serde_json::{Map, Value, json};

/// The previewed shape of a schema: the typed mapping plus a render-friendly
/// document tree.
#[derive(Debug, Serialize)]
pub struct Preview {
    /// The resolved OpenSearch mapping (every field's type, nesting, options).
    pub mapping: IndexMapping,
    /// The document tree as the designer renders it.
    pub document: Vec<DocumentNode>,
}

/// One node in the previewed document tree.
#[derive(Debug, Serialize)]
pub struct DocumentNode {
    /// The field/document key.
    pub name: String,
    /// A human-readable type label (e.g. `keyword`, `nested`, `map<text>`).
    #[serde(rename = "type")]
    pub type_label: String,
    /// Whether the value may be absent/null.
    pub nullable: bool,
    /// Whether the field is an array (an `ids` aggregate).
    pub array: bool,
    /// Nested fields (objects, nested join children).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<DocumentNode>,
}

/// Resolve `schema` into its mapping and document tree under `index`.
pub fn preview(schema: &IndexSchema, index: &IndexName) -> Preview {
    let mapping = schema.resolve(index.clone());
    let document = mapping.fields.iter().map(node).collect();
    Preview { mapping, document }
}

/// Synthesize an example document for `schema` from its declared types alone —
/// no database. Used to show the *shape* of a real document when the root table
/// has no rows to sample. Every field gets a type-appropriate placeholder.
pub fn example_document(schema: &IndexSchema, index: &IndexName) -> Value {
    Value::Object(object_from(&schema.resolve(index.clone()).fields))
}

fn object_from(fields: &[ResolvedField]) -> Map<String, Value> {
    fields
        .iter()
        .map(|f| (f.name.as_ref().to_owned(), example_value(f)))
        .collect()
}

fn example_value(field: &ResolvedField) -> Value {
    // A dynamic-key `map`: one illustrative key of the declared value type.
    if let Some(values) = &field.mapping.map_values {
        return Value::Object(Map::from_iter([(
            "example_key".to_owned(),
            example_scalar(values),
        )]));
    }
    if !field.children.is_empty() {
        let object = Value::Object(object_from(&field.children));
        // A `nested` join is an array of objects; a plain `object` is one object.
        return match field.mapping.mapping_type {
            MappingType::Nested => Value::Array(vec![object]),
            _ => object,
        };
    }
    let value = example_scalar(&field.mapping.mapping_type);
    // An `ids` aggregate is a flat array of keys.
    if field.array {
        Value::Array(vec![value.clone(), value])
    } else {
        value
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use schema_core::ParseFrom;
    use schema_index_yaml::SchemaYaml;

    use super::*;

    fn schema(yaml: &str) -> IndexSchema {
        let entity = SchemaYaml::try_parse(yaml).expect("parse");
        IndexSchema::try_from(entity).expect("convert")
    }

    #[test]
    fn example_document_has_type_appropriate_values_per_kind() {
        let schema = schema(
            "version: 1\n\
             table: t\n\
             primary_key: id\n\
             fields:\n\
             \x20 - integer: id\n\
             \x20   required: false\n\
             \x20 - keyword: code\n\
             \x20   required: false\n\
             \x20 - boolean: active\n\
             \x20   required: false\n\
             \x20 - object: meta\n\
             \x20   fields:\n\
             \x20     - text: note\n\
             \x20       required: false\n",
        );
        let index = IndexName::try_new("t").expect("index name");

        let doc = example_document(&schema, &index);
        let obj = doc.as_object().expect("top-level object");

        assert_eq!(obj.get("id"), Some(&json!(42)));
        assert_eq!(obj.get("code"), Some(&json!("example")));
        assert_eq!(obj.get("active"), Some(&json!(true)));
        assert_eq!(
            obj.get("meta").and_then(|m| m.get("note")),
            Some(&json!("example text")),
        );
    }
}

fn example_scalar(mapping_type: &MappingType) -> Value {
    match mapping_type {
        MappingType::Text => json!("example text"),
        MappingType::Keyword => json!("example"),
        MappingType::Boolean => json!(true),
        MappingType::Byte | MappingType::Short | MappingType::Integer | MappingType::Long => {
            json!(42)
        }
        MappingType::Float
        | MappingType::Double
        | MappingType::HalfFloat
        | MappingType::ScaledFloat => json!(42.5),
        MappingType::Date => json!("2024-01-15T09:30:00Z"),
        MappingType::Object => json!({ "key": "value" }),
        MappingType::Nested => json!([]),
        MappingType::Other(name) if name == "geo_point" => json!({ "lat": 40.71, "lon": -74.0 }),
        MappingType::Other(_) => Value::Null,
    }
}

fn node(field: &ResolvedField) -> DocumentNode {
    DocumentNode {
        name: field.name.as_ref().to_owned(),
        type_label: type_label(&field.mapping),
        nullable: field.nullable,
        array: field.array,
        children: field.children.iter().map(node).collect(),
    }
}

fn type_label(mapping: &Mapping) -> String {
    if let Some(values) = &mapping.map_values {
        return format!("map<{}>", mapping_type_name(values));
    }
    mapping_type_name(&mapping.mapping_type).to_owned()
}

fn mapping_type_name(mapping_type: &MappingType) -> &str {
    match mapping_type {
        MappingType::Text => "text",
        MappingType::Keyword => "keyword",
        MappingType::Boolean => "boolean",
        MappingType::Byte => "byte",
        MappingType::Short => "short",
        MappingType::Integer => "integer",
        MappingType::Long => "long",
        MappingType::Float => "float",
        MappingType::Double => "double",
        MappingType::HalfFloat => "half_float",
        MappingType::ScaledFloat => "scaled_float",
        MappingType::Date => "date",
        MappingType::Object => "object",
        MappingType::Nested => "nested",
        MappingType::Other(name) => name,
    }
}

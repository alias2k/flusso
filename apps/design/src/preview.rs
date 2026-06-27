//! Deriving the search document's shape from a schema, database-free.
//!
//! The whole point of a visual designer is seeing the document you're building.
//! [`preview`] takes an [`IndexSchema`] and returns its resolved
//! [`IndexMapping`] (the typed OpenSearch mapping flusso would create) plus a
//! friendlier [`DocumentNode`] tree for rendering — both from the declared
//! schema alone, exactly as `flusso check` derives a mapping without touching a
//! database.

use schema_core::{IndexMapping, IndexName, IndexSchema, Mapping, MappingType, ResolvedField};
use serde::Serialize;

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

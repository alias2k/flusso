//! Deriving an OpenSearch-style [`MappingType`] from a Postgres column type,
//! for fields a schema leaves without an explicit `mapping`.
//!
//! The input is the canonical name produced by `format_type` (see
//! [`column_type`](super::PgDocumentBuilder::column_type)): e.g. `bigint`,
//! `integer`, `numeric(10,2)`, `character varying(255)`, `timestamp with time
//! zone`, `integer[]`.

use schema_core::MappingType;

/// Map a canonical Postgres type name to the mapping type a search index should
/// use. Unrecognized types fall back to `keyword` (exact, aggregatable) — the
/// safe default for an identifier-like value.
pub(super) fn pg_type_to_mapping(sql_type: &str) -> MappingType {
    // Arrays map as their element type — OpenSearch fields are multi-valued
    // natively, so `integer[]` is just `integer`.
    let base = sql_type.trim().trim_end_matches("[]").trim();
    // Drop any precision/length/timezone modifier in parentheses:
    // `numeric(10,2)` -> `numeric`, `character varying(255)` -> `character varying`.
    let base = base.split('(').next().unwrap_or(base).trim();

    match base.to_ascii_lowercase().as_str() {
        "bigint" | "int8" | "bigserial" => MappingType::Long,
        "integer" | "int" | "int4" | "serial" => MappingType::Integer,
        "smallint" | "int2" | "smallserial" => MappingType::Short,
        "boolean" | "bool" => MappingType::Boolean,
        "real" | "float4" => MappingType::Float,
        "double precision" | "float8" => MappingType::Double,
        // Arbitrary-precision; double is the lossy-but-searchable default. Pin
        // an explicit `scaled_float` mapping in config when exactness matters.
        "numeric" | "decimal" | "money" => MappingType::Double,
        "timestamp with time zone"
        | "timestamp without time zone"
        | "timestamp"
        | "timestamptz"
        | "date"
        | "time with time zone"
        | "time without time zone"
        | "time"
        | "timetz" => MappingType::Date,
        "bytea" => MappingType::Other("binary".to_owned()),
        // Nested JSON keys are subject to the same `dynamic: strict` rule, so an
        // explicit mapping is usually wanted; `object` is the neutral default.
        "json" | "jsonb" => MappingType::Object,
        // text, character varying, character, char, varchar, bpchar, uuid, inet,
        // cidr, macaddr, citext, name, enums, and anything else token-like.
        _ => MappingType::Keyword,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_families_map_by_width() {
        assert_eq!(pg_type_to_mapping("bigint"), MappingType::Long);
        assert_eq!(pg_type_to_mapping("integer"), MappingType::Integer);
        assert_eq!(pg_type_to_mapping("smallint"), MappingType::Short);
    }

    #[test]
    fn strings_default_to_keyword() {
        assert_eq!(pg_type_to_mapping("text"), MappingType::Keyword);
        assert_eq!(pg_type_to_mapping("character varying(255)"), MappingType::Keyword);
        assert_eq!(pg_type_to_mapping("uuid"), MappingType::Keyword);
    }

    #[test]
    fn numeric_with_precision_is_double() {
        assert_eq!(pg_type_to_mapping("numeric(10,2)"), MappingType::Double);
        assert_eq!(pg_type_to_mapping("double precision"), MappingType::Double);
    }

    #[test]
    fn temporal_types_are_date() {
        assert_eq!(pg_type_to_mapping("timestamp with time zone"), MappingType::Date);
        assert_eq!(pg_type_to_mapping("date"), MappingType::Date);
    }

    #[test]
    fn array_suffix_maps_as_element_type() {
        assert_eq!(pg_type_to_mapping("integer[]"), MappingType::Integer);
        assert_eq!(pg_type_to_mapping("text[]"), MappingType::Keyword);
    }

    #[test]
    fn bytea_is_binary() {
        assert_eq!(pg_type_to_mapping("bytea"), MappingType::Other("binary".to_owned()));
    }

    #[test]
    fn unknown_falls_back_to_keyword() {
        assert_eq!(pg_type_to_mapping("some_custom_enum"), MappingType::Keyword);
    }
}

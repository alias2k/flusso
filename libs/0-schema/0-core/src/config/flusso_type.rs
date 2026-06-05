use serde::{Deserialize, Serialize};

use super::MappingType;

/// The declared type of a leaf field — the single vocabulary that bridges a
/// Postgres column type and an OpenSearch mapping type.
///
/// A self-describing schema names one of these per scalar field, so the document
/// shape (and the index mapping) is known without ever touching the database. A
/// variant pins both ends: which Postgres types satisfy it (for validation, when
/// a database is reachable) and which OpenSearch [`MappingType`] it emits. That
/// is what lets the two disagree on purpose — an `Enum` is stored as text in
/// Postgres but must be a `keyword` in OpenSearch.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlussoType {
    /// Analyzed full-text. PG `text` / `varchar` → OS `text`.
    Text,
    /// Exact, aggregatable token. PG `text` / `varchar` → OS `keyword`.
    Keyword,
    /// A closed set of string values — stored as text in Postgres (a `varchar`
    /// or a PG `enum`), indexed exactly as a `keyword`.
    Enum,
    /// PG `uuid` → OS `keyword`.
    Uuid,
    /// PG `boolean` → OS `boolean`.
    Boolean,
    /// PG `smallint` / `int2` → OS `short`.
    Short,
    /// PG `integer` / `int4` → OS `integer`.
    Integer,
    /// PG `bigint` / `int8` → OS `long`.
    Long,
    /// PG `real` / `float4` → OS `float`.
    Float,
    /// PG `double precision` / `float8` → OS `double`.
    Double,
    /// PG `numeric` / `decimal` / `money` → OS `double` (lossy but searchable;
    /// declare [`Custom`](Self::Custom) `scaled_float` when exactness matters).
    Decimal,
    /// PG `date` → OS `date`.
    Date,
    /// PG `timestamp` / `timestamptz` / `time` → OS `date`.
    Timestamp,
    /// PG `bytea` → OS `binary`.
    Binary,
    /// PG `json` / `jsonb` → OS `object`.
    Json,
    /// An escape hatch: an explicit OpenSearch type with the Postgres types it
    /// accepts, for anything the named variants don't cover (`geo_point`,
    /// `scaled_float`, …).
    Custom {
        postgres: Vec<String>,
        opensearch: String,
    },
}

impl FlussoType {
    /// The OpenSearch [`MappingType`] this declared type maps to.
    pub fn opensearch(&self) -> MappingType {
        match self {
            FlussoType::Text => MappingType::Text,
            FlussoType::Keyword | FlussoType::Enum | FlussoType::Uuid => MappingType::Keyword,
            FlussoType::Boolean => MappingType::Boolean,
            FlussoType::Short => MappingType::Short,
            FlussoType::Integer => MappingType::Integer,
            FlussoType::Long => MappingType::Long,
            FlussoType::Float => MappingType::Float,
            FlussoType::Double | FlussoType::Decimal => MappingType::Double,
            FlussoType::Date | FlussoType::Timestamp => MappingType::Date,
            FlussoType::Binary => MappingType::Other("binary".to_owned()),
            FlussoType::Json => MappingType::Object,
            FlussoType::Custom { opensearch, .. } => MappingType::from_name(opensearch),
        }
    }

    /// Whether `sql_type` (a Postgres type name as `format_type` reports it, e.g.
    /// `bigint`, `character varying(255)`, `numeric(10,2)`, `integer[]`) is a
    /// Postgres type this declared type accepts. Used to validate a declared
    /// schema against a live database; with no database, it is never consulted.
    pub fn accepts_pg(&self, sql_type: &str) -> bool {
        let base = normalize_pg_type(sql_type);
        match self {
            FlussoType::Text | FlussoType::Keyword | FlussoType::Enum => matches!(
                base.as_str(),
                "text"
                    | "character varying"
                    | "varchar"
                    | "character"
                    | "char"
                    | "bpchar"
                    | "citext"
                    | "name"
            ),
            FlussoType::Uuid => base == "uuid",
            FlussoType::Boolean => matches!(base.as_str(), "boolean" | "bool"),
            FlussoType::Short => matches!(base.as_str(), "smallint" | "int2" | "smallserial"),
            FlussoType::Integer => matches!(base.as_str(), "integer" | "int" | "int4" | "serial"),
            FlussoType::Long => matches!(base.as_str(), "bigint" | "int8" | "bigserial"),
            FlussoType::Float => matches!(base.as_str(), "real" | "float4"),
            FlussoType::Double => matches!(base.as_str(), "double precision" | "float8"),
            FlussoType::Decimal => matches!(base.as_str(), "numeric" | "decimal" | "money"),
            FlussoType::Date => base == "date",
            FlussoType::Timestamp => matches!(
                base.as_str(),
                "timestamp with time zone"
                    | "timestamp without time zone"
                    | "timestamp"
                    | "timestamptz"
                    | "time with time zone"
                    | "time without time zone"
                    | "time"
                    | "timetz"
            ),
            FlussoType::Binary => base == "bytea",
            FlussoType::Json => matches!(base.as_str(), "json" | "jsonb"),
            FlussoType::Custom { postgres, .. } => {
                postgres.iter().any(|t| normalize_pg_type(t) == base)
            }
        }
    }
}

/// Normalize a `format_type` Postgres type name to its bare base name: drop an
/// array `[]` suffix (OpenSearch fields are natively multi-valued) and any
/// `(precision)` / `(length)` modifier, then lowercase. Mirrors the
/// normalization the Postgres source uses when reading a column's type.
fn normalize_pg_type(sql_type: &str) -> String {
    let base = sql_type.trim().trim_end_matches("[]").trim();
    let base = base.split('(').next().unwrap_or(base).trim();
    base.to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_is_text_in_pg_keyword_in_opensearch() {
        let ty = FlussoType::Enum;
        assert_eq!(ty.opensearch(), MappingType::Keyword);
        assert!(ty.accepts_pg("character varying(32)"));
        assert!(ty.accepts_pg("text"));
        assert!(!ty.accepts_pg("integer"));
    }

    #[test]
    fn integer_families_match_by_width() {
        assert!(FlussoType::Long.accepts_pg("bigint"));
        assert!(FlussoType::Integer.accepts_pg("integer"));
        assert!(FlussoType::Short.accepts_pg("smallint"));
        assert!(!FlussoType::Integer.accepts_pg("bigint"));
    }

    #[test]
    fn arrays_and_modifiers_are_stripped() {
        assert!(FlussoType::Integer.accepts_pg("integer[]"));
        assert!(FlussoType::Decimal.accepts_pg("numeric(10,2)"));
        assert_eq!(FlussoType::Timestamp.opensearch(), MappingType::Date);
    }

    #[test]
    fn custom_carries_its_own_mapping_and_pg_set() {
        let ty = FlussoType::Custom {
            postgres: vec!["numeric".to_owned()],
            opensearch: "scaled_float".to_owned(),
        };
        assert_eq!(ty.opensearch(), MappingType::ScaledFloat);
        assert!(ty.accepts_pg("numeric(12,4)"));
        assert!(!ty.accepts_pg("text"));
    }
}

//! Whether an `ids` aggregate's `element_type` holds the keys it collects.
//!
//! An `ids` field is an array of the related table's primary keys, mapped as
//! the declared `element_type`. The sink takes each key as that type, so the
//! key's own type decides what lands: every value fits, some do, the values
//! change on the way in, or none fits and every document collecting a key is
//! rejected. The verdicts were measured on OpenSearch 2 (issue #299).
//!
//! ```text
//! fit(Long, "integer")  → Holds
//! fit(Integer, "bigint") → Some      (a key past 2^31 is rejected)
//! fit(Date, "bigint")   → Changes   (read as epoch milliseconds)
//! fit(Long, "uuid")     → Rejects
//! ```

use kernel::FlussoType;

/// What an `element_type` does with the keys of one key type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Fit {
    /// Every key indexes as itself.
    Holds,
    /// Some keys index; the sink rejects a document collecting another.
    Some,
    /// Every key indexes, as something else: how.
    Changes(&'static str),
    /// No key indexes; the sink rejects every document collecting one.
    Rejects,
}

/// A key type as the sink sees its values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Key {
    Int2,
    Int4,
    Int8,
    Numeric,
    Float4,
    Float8,
    Bool,
    Text,
    Uuid,
    Bytea,
    Date,
    Time,
    Timestamp,
    /// A type this check doesn't know: never reported.
    Other,
}

const ROUNDED_TO_FLOAT: &str = "a key past 2^24 is rounded to the nearest `float`";
const ROUNDED_TO_DOUBLE: &str = "a key past 2^53 is rounded to the nearest `double`";
const EPOCH_MILLIS: &str = "they are read as milliseconds since 1970";
const NOT_BYTES: &str = "they are indexed as written, not as bytes";

/// How `element` holds the keys of a column of `sql_type` (a type name as
/// the catalog reports it; a domain by its base type).
pub(super) fn fit(element: &FlussoType, sql_type: &str) -> Fit {
    use Key::*;
    let key = key(sql_type);
    match element {
        FlussoType::Text
        | FlussoType::Identifier
        | FlussoType::Keyword
        | FlussoType::Enum
        | FlussoType::Uuid
        | FlussoType::GeoPoint
        | FlussoType::Custom { .. } => Fit::Holds,
        FlussoType::Json | FlussoType::Map { .. } => Fit::Rejects,
        _ if key == Other => Fit::Holds,
        FlussoType::Binary => match key {
            Bytea => Fit::Holds,
            _ => Fit::Changes(NOT_BYTES),
        },
        FlussoType::Boolean => match key {
            Bool => Fit::Holds,
            Text => Fit::Some,
            _ => Fit::Rejects,
        },
        FlussoType::Short => match key {
            Int2 => Fit::Holds,
            Int4 | Int8 | Numeric | Float4 | Float8 | Text => Fit::Some,
            _ => Fit::Rejects,
        },
        FlussoType::Integer => match key {
            Int2 | Int4 => Fit::Holds,
            Int8 | Numeric | Float4 | Float8 | Text => Fit::Some,
            _ => Fit::Rejects,
        },
        FlussoType::Long => match key {
            Int2 | Int4 | Int8 => Fit::Holds,
            Numeric | Float4 | Float8 | Text => Fit::Some,
            _ => Fit::Rejects,
        },
        FlussoType::Float => match key {
            Int2 | Float4 => Fit::Holds,
            Int4 | Int8 => Fit::Changes(ROUNDED_TO_FLOAT),
            Numeric | Float8 | Text => Fit::Some,
            _ => Fit::Rejects,
        },
        FlussoType::Double | FlussoType::Decimal => match key {
            Int2 | Int4 | Numeric | Float4 | Float8 => Fit::Holds,
            Int8 => Fit::Changes(ROUNDED_TO_DOUBLE),
            Text => Fit::Some,
            _ => Fit::Rejects,
        },
        FlussoType::Date | FlussoType::Timestamp => match key {
            Date | Timestamp => Fit::Holds,
            Int2 | Int4 | Int8 => Fit::Changes(EPOCH_MILLIS),
            Numeric | Float4 | Float8 | Text => Fit::Some,
            _ => Fit::Rejects,
        },
    }
}

/// The type as a schema writes it, e.g. `long`.
pub(super) fn name(ty: &FlussoType) -> &'static str {
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
        FlussoType::Map { .. } => "map",
        FlussoType::GeoPoint => "geo_point",
        FlussoType::Custom { .. } => "custom",
    }
}

fn key(sql_type: &str) -> Key {
    let base = sql_type.trim();
    if base.ends_with("[]") {
        return Key::Other;
    }
    let base = base.split('(').next().unwrap_or(base).trim();
    match base.to_ascii_lowercase().as_str() {
        "smallint" | "int2" => Key::Int2,
        "integer" | "int" | "int4" => Key::Int4,
        "bigint" | "int8" => Key::Int8,
        "numeric" | "decimal" => Key::Numeric,
        "real" | "float4" => Key::Float4,
        "double precision" | "float8" => Key::Float8,
        "boolean" | "bool" => Key::Bool,
        "text" | "character varying" | "varchar" | "character" | "char" | "bpchar" | "name"
        | "citext" => Key::Text,
        "uuid" => Key::Uuid,
        "bytea" => Key::Bytea,
        "date" => Key::Date,
        "time" | "time without time zone" | "time with time zone" | "timetz" => Key::Time,
        "timestamp"
        | "timestamp without time zone"
        | "timestamp with time zone"
        | "timestamptz" => Key::Timestamp,
        _ => Key::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_type_with_a_typmod_or_in_another_spelling_is_recognised() {
        assert_eq!(fit(&FlussoType::Long, "character varying(36)"), Fit::Some);
        assert_eq!(fit(&FlussoType::Long, "INT8"), Fit::Holds);
        assert_eq!(
            fit(&FlussoType::Date, "timestamp(3) with time zone"),
            Fit::Holds
        );
    }

    #[test]
    fn an_unknown_key_type_is_never_reported_but_json_always_is() {
        for element in [FlussoType::Long, FlussoType::Boolean, FlussoType::Date] {
            assert_eq!(fit(&element, "inet"), Fit::Holds);
            assert_eq!(fit(&element, "integer[]"), Fit::Holds);
        }
        assert_eq!(fit(&FlussoType::Json, "inet"), Fit::Rejects);
    }

    /// A pair that holds every key exactly is never reported: a warning on
    /// every start of a correct deployment would be a regression.
    #[test]
    fn a_pair_that_holds_every_key_exactly_is_never_reported() {
        let textual = [
            "text",
            "character varying(36)",
            "character(3)",
            "name",
            "citext",
        ];
        let exact: Vec<(FlussoType, &str)> = [
            (FlussoType::Short, "smallint"),
            (FlussoType::Integer, "smallint"),
            (FlussoType::Integer, "integer"),
            (FlussoType::Long, "smallint"),
            (FlussoType::Long, "integer"),
            (FlussoType::Long, "bigint"),
            (FlussoType::Float, "smallint"),
            (FlussoType::Float, "real"),
            (FlussoType::Double, "smallint"),
            (FlussoType::Double, "integer"),
            (FlussoType::Double, "real"),
            (FlussoType::Double, "double precision"),
            (FlussoType::Decimal, "integer"),
            (FlussoType::Decimal, "numeric(10,2)"),
            (FlussoType::Boolean, "boolean"),
            (FlussoType::Uuid, "uuid"),
            (FlussoType::Keyword, "uuid"),
            (FlussoType::Binary, "bytea"),
            (FlussoType::Date, "date"),
            (FlussoType::Date, "timestamp with time zone"),
            (FlussoType::Timestamp, "timestamp with time zone"),
        ]
        .into_iter()
        .chain(textual.iter().flat_map(|key| {
            [
                FlussoType::Keyword,
                FlussoType::Text,
                FlussoType::Identifier,
                FlussoType::Enum,
            ]
            .map(|element| (element, *key))
        }))
        .collect();
        for (element, key) in exact {
            assert_eq!(
                fit(&element, key),
                Fit::Holds,
                "{} over {key}",
                name(&element)
            );
        }
    }

    #[test]
    fn the_verdicts_match_what_opensearch_did_with_each_pair() {
        let cases = [
            (FlussoType::Keyword, "uuid", Fit::Holds),
            (FlussoType::Uuid, "bigint", Fit::Holds),
            (FlussoType::Short, "smallint", Fit::Holds),
            (FlussoType::Short, "integer", Fit::Some),
            (FlussoType::Integer, "bigint", Fit::Some),
            (FlussoType::Long, "numeric", Fit::Some),
            (FlussoType::Long, "text", Fit::Some),
            (FlussoType::Long, "uuid", Fit::Rejects),
            (FlussoType::Long, "date", Fit::Rejects),
            (FlussoType::Float, "integer", Fit::Changes(ROUNDED_TO_FLOAT)),
            (
                FlussoType::Double,
                "bigint",
                Fit::Changes(ROUNDED_TO_DOUBLE),
            ),
            (FlussoType::Decimal, "numeric", Fit::Holds),
            (FlussoType::Boolean, "boolean", Fit::Holds),
            (FlussoType::Boolean, "text", Fit::Some),
            (FlussoType::Boolean, "integer", Fit::Rejects),
            (FlussoType::Date, "date", Fit::Holds),
            (FlussoType::Date, "integer", Fit::Changes(EPOCH_MILLIS)),
            (
                FlussoType::Timestamp,
                "time without time zone",
                Fit::Rejects,
            ),
            (FlussoType::Binary, "bytea", Fit::Holds),
            (FlussoType::Binary, "bigint", Fit::Changes(NOT_BYTES)),
            (FlussoType::Json, "integer", Fit::Rejects),
        ];
        for (element, key, expected) in cases {
            assert_eq!(
                fit(&element, key),
                expected,
                "{} over {key}",
                name(&element)
            );
        }
    }
}

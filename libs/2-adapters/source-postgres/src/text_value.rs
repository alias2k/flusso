//! Postgres text output → [`GenericValue`]: the one parser a key goes through.
//!
//! A key reaches flusso twice: in the WAL, as the text pgoutput renders, and
//! read back by a query (the backfill, the batched build, reverse resolution).
//! The engine matches the two by variant-exact equality, and the document id
//! is rendered from the value, so both sides must parse the same text the same
//! way (issue #295). Both go through [`typed_value`]: the WAL decoder on each
//! cell, the read-back on the key column's text output (see
//! [`document::value::decode_key`](crate::document::value)).
//!
//! The text depends on session settings, which a pooler may not pass on, so
//! neither side relies on a startup option (PgBouncer refuses any inside
//! `options`):
//!
//! - the **replication session** runs [`REPLICATION_SETTINGS`] as `SET`s over
//!   its own connection before `START_REPLICATION`, a failure stopping the
//!   start;
//! - a **query** renders each key in a form no setting changes
//!   ([`key_text`]), so a transaction-mode pooler can't change it either:
//!   `bytea` as hex, a float as its exact bits, and everything else through
//!   its output function, which for dates and times follows the `DateStyle`
//!   and `TimeZone` sqlx sends at startup (`ISO`, `UTC`; PgBouncer passes
//!   both in every pool mode).
//!
//! A value the parser can't represent (an infinite date, a numeric `NaN`)
//! stays the text it reads as, on both sides alike, and is bound back as text
//! cast in SQL.
//!
//! ```text
//! typed_value("2024-02-29", 1082)          → Date(2024-02-29)
//! typed_value("0044-03-15 BC", 1082)       → Date(-0043-03-15)
//! typed_value("infinity", 1082)            → String("infinity")
//! typed_value("\\x01ab", 17)               → Bytes([0x01, 0xab])
//! ```

use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use kernel::GenericValue;

/// What the replication session runs before streaming: the settings its text
/// output depends on, as the queries render it.
pub(crate) const REPLICATION_SETTINGS: &[&str] = &[
    "SET DateStyle = 'ISO'",
    "SET TimeZone = 'UTC'",
    "SET bytea_output = 'hex'",
    "SET extra_float_digits = 1",
];

/// The SQL rendering of `expr`, a key of base type `base_name`
/// (`pg_type.typname`), that [`read_back`] parses: the same value the WAL's
/// text gives, whatever the session's settings.
pub(crate) fn key_text(expr: &str, base_name: &str) -> String {
    match base_name {
        "bytea" => format!("'\\x' || encode({expr}, 'hex')"),
        "float4" => format!("encode(float4send({expr}), 'hex')"),
        "float8" => format!("encode(float8send({expr}), 'hex')"),
        _ => format!("format('%s', {expr})"),
    }
}

/// A key's [`key_text`] rendering, by the type OID the query reports for it,
/// as the [`GenericValue`] [`typed_value`] gives the WAL's text of it.
pub(crate) fn read_back(text: &str, type_oid: u32) -> GenericValue {
    let bits = || u64::from_str_radix(text, 16).ok();
    match type_oid {
        700 => bits()
            .and_then(|bits| u32::try_from(bits).ok())
            .map_or_else(
                || GenericValue::String(text.to_owned()),
                |bits| GenericValue::Float(f32::from_bits(bits)),
            ),
        701 => bits().map_or_else(
            || GenericValue::String(text.to_owned()),
            |bits| GenericValue::Double(f64::from_bits(bits)),
        ),
        _ => typed_value(text, type_oid),
    }
}

/// The type OIDs [`typed_value`] decodes into a typed variant; any other type
/// stays text.
const DECODED: &[u32] = &[
    16, 21, 23, 20, 26, 700, 701, 1700, 2950, 17, 1082, 1083, 1114, 1184,
];

/// The built-in text types a key may have: they read as their text.
const TEXT: &[u32] = &[25, 1043, 1042, 19];

/// Whether [`typed_value`] decodes values of `type_oid` (any other type stays
/// text).
pub(crate) fn is_decoded(type_oid: u32) -> bool {
    DECODED.contains(&type_oid)
}

/// Whether a key of the type named by `type_oid` (`type_name` for an
/// extension type, whose OID varies) reads back as a key. Every other type
/// reads as null, and `validate_indexes` refuses it as a root key
/// (`document::value::KEY_TYPES` lists the same types by name).
pub(crate) fn is_key_type(type_oid: u32, type_name: &str) -> bool {
    (is_decoded(type_oid) && type_oid != 26) || TEXT.contains(&type_oid) || type_name == "citext"
}

/// Interpret a Postgres text value by its type OID, as rendered under
/// [`REPLICATION_SETTINGS`]. Unknown or unparseable values (`infinity`, a numeric
/// `NaN`) stay the text itself.
pub(crate) fn typed_value(text: &str, type_oid: u32) -> GenericValue {
    let as_text = || GenericValue::String(text.to_owned());
    match type_oid {
        16 => match text {
            "t" => GenericValue::Bool(true),
            "f" => GenericValue::Bool(false),
            _ => as_text(),
        },
        21 => text
            .parse::<i16>()
            .map_or_else(|_| as_text(), GenericValue::SmallInt),
        23 => text
            .parse::<i32>()
            .map_or_else(|_| as_text(), GenericValue::Int),
        20 | 26 => text
            .parse::<i64>()
            .map_or_else(|_| as_text(), GenericValue::BigInt),
        700 => text
            .parse::<f32>()
            .map_or_else(|_| as_text(), GenericValue::Float),
        701 => text
            .parse::<f64>()
            .map_or_else(|_| as_text(), GenericValue::Double),
        1700 => rust_decimal::Decimal::from_str_exact(text)
            .map_or_else(|_| as_text(), GenericValue::Decimal),
        2950 => uuid::Uuid::parse_str(text).map_or_else(|_| as_text(), GenericValue::Uuid),
        17 => postgres_hex(text).map_or_else(as_text, GenericValue::Bytes),
        1082 => date(text).map_or_else(as_text, GenericValue::Date),
        1083 => NaiveTime::parse_from_str(text, "%H:%M:%S%.f")
            .map_or_else(|_| as_text(), GenericValue::Time),
        1114 => timestamp(text).map_or_else(as_text, GenericValue::Timestamp),
        1184 => timestamptz(text).map_or_else(as_text, GenericValue::TimestampTz),
        _ => as_text(),
    }
}

/// A `bytea` in its `hex` text output: `\x` followed by two hex digits per
/// byte. `None` for anything else.
pub(crate) fn postgres_hex(text: &str) -> Option<Vec<u8>> {
    let digits = text.strip_prefix("\\x")?.as_bytes();
    if digits.len() % 2 != 0 || !digits.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    digits
        .chunks(2)
        .map(|pair| {
            let pair = std::str::from_utf8(pair).ok()?;
            u8::from_str_radix(pair, 16).ok()
        })
        .collect()
}

/// The text before an ISO ` BC` suffix, and whether it had one. Postgres has
/// no year 0, so year `y` BC is chrono's proleptic year `1 - y`.
fn era(text: &str) -> (&str, bool) {
    match text.strip_suffix(" BC") {
        Some(rest) => (rest, true),
        None => (text, false),
    }
}

fn proleptic<T: Datelike>(value: T, bc: bool) -> Option<T> {
    if bc {
        value.with_year(1 - value.year())
    } else {
        Some(value)
    }
}

fn date(text: &str) -> Option<NaiveDate> {
    let (text, bc) = era(text);
    proleptic(NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()?, bc)
}

fn timestamp(text: &str) -> Option<NaiveDateTime> {
    let (text, bc) = era(text);
    proleptic(
        NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S%.f").ok()?,
        bc,
    )
}

/// A `timestamptz` as ISO renders it: local time, then the session time
/// zone's offset as `±HH`, `±HH:MM` or `±HH:MM:SS` (a historical local mean
/// time has seconds).
fn timestamptz(text: &str) -> Option<DateTime<Utc>> {
    let (text, bc) = era(text);
    let sign_at = text
        .char_indices()
        .skip(10)
        .filter(|(_, c)| *c == '+' || *c == '-')
        .map(|(at, _)| at)
        .last()?;
    let (local, offset) = text.split_at(sign_at);
    let local = proleptic(
        NaiveDateTime::parse_from_str(local, "%Y-%m-%d %H:%M:%S%.f").ok()?,
        bc,
    )?;
    let offset = fixed_offset(offset)?;
    Some(
        local
            .and_local_timezone(offset)
            .single()?
            .with_timezone(&Utc),
    )
}

fn fixed_offset(text: &str) -> Option<FixedOffset> {
    let (sign, rest) = match text.split_at_checked(1)? {
        ("+", rest) => (1, rest),
        ("-", rest) => (-1, rest),
        _ => return None,
    };
    let mut parts = rest.split(':');
    let mut seconds = 0;
    for unit in [3600, 60, 1] {
        if let Some(part) = parts.next() {
            if part.len() != 2 {
                return None;
            }
            seconds += part.parse::<i32>().ok()? * unit;
        }
    }
    if parts.next().is_some() {
        return None;
    }
    FixedOffset::east_opt(sign * seconds)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;

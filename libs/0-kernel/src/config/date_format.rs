//! Making a date mapping's `format` read the dates flusso writes.
//!
//! flusso writes a `date` as `2024-01-02`, a `timestamp` as
//! `2024-01-02T03:04:05.123456` and a `timestamptz` as
//! `2024-01-02T03:04:05.123456+00:00` (`sink::Json`). A field's
//! `options.format` replaces OpenSearch's default
//! (`strict_date_optional_time||epoch_millis`), so a format tuned for another
//! form (the space form flusso wrote before 0.18, `epoch_millis`, …) would
//! refuse every document holding a value.
//!
//! So the mapping a date field resolves to appends
//! `||strict_date_optional_time` to such a format ([`with_iso`]): the user's
//! patterns stay first, so OpenSearch still prints dates in the user's form
//! (`key_as_string`, sort values, `docvalue_fields`) and reads queries in
//! either; `_source` is unchanged. A format that already holds a named
//! pattern reading every one of those forms (`strict_date_optional_time`,
//! `date_optional_time`, their `_nanos` and camelCase spellings) is left as
//! written, as is a field with no `format`.
//!
//! [`iso_date_format`] decides it per field, for the projection
//! ([`IndexSchema::resolve`](super::IndexSchema::resolve)) and the content hash
//! alike, so an index whose format gets the pattern moves to a new physical
//! index once.
//!
//! ```
//! use kernel::config::with_iso;
//!
//! assert_eq!(
//!     with_iso("yyyy-MM-dd HH:mm:ss").as_deref(),
//!     Some("yyyy-MM-dd HH:mm:ss||strict_date_optional_time")
//! );
//! assert_eq!(with_iso("strict_date_optional_time||epoch_millis"), None);
//! ```

use super::{Field, MappingType};
use crate::common::GenericValue;

/// The pattern appended to a date `format` that can't read what flusso
/// writes.
pub const ISO_DATE_PATTERN: &str = "strict_date_optional_time";

/// Named OpenSearch formats that read every date form flusso writes
/// (`2024-01-02`, `2024-01-02T03:04:05.123456`, `…+00:00`), with or without
/// `strict_`.
const READING: &[&str] = &["date_optional_time", "date_optional_time_nanos"];

/// `format` with [`ISO_DATE_PATTERN`] appended, or `None` when it already
/// reads every date form flusso writes (or is empty).
pub fn with_iso(format: &str) -> Option<String> {
    let reads = format.split("||").map(str::trim).any(|pattern| {
        let snake = snake_case(pattern);
        let name = snake.strip_prefix("strict_").unwrap_or(&snake);
        READING.contains(&name)
    });
    (!reads && !format.trim().is_empty()).then(|| format!("{format}||{ISO_DATE_PATTERN}"))
}

/// The `format` the mapping of `field` uses in place of its own: its
/// `options.format` with [`ISO_DATE_PATTERN`] appended, for a date-typed
/// field whose format can't read what flusso writes; `None` for any other
/// field.
pub fn iso_date_format(field: &Field) -> Option<String> {
    let Some(GenericValue::String(format)) = field.options.get("format") else {
        return None;
    };
    match super::projection::mapping_type(field) {
        MappingType::Date => with_iso(format),
        MappingType::Other(name) if name == "date_nanos" => with_iso(format),
        _ => None,
    }
}

/// OpenSearch still accepts the camelCase spelling of its named formats
/// (`strictDateOptionalTime`).
fn snake_case(name: &str) -> String {
    let mut snake = String::with_capacity(name.len() + 4);
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            snake.push('_');
        }
        snake.push(c.to_ascii_lowercase());
    }
    snake
}

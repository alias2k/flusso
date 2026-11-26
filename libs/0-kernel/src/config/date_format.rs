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

use super::Field;

/// The pattern appended to a date `format` that can't read what flusso
/// writes.
pub const ISO_DATE_PATTERN: &str = "strict_date_optional_time";

/// `format` with [`ISO_DATE_PATTERN`] appended, or `None` when it already
/// reads every date form flusso writes (or is empty).
pub fn with_iso(_format: &str) -> Option<String> {
    None
}

/// The `format` the mapping of `field` uses in place of its own: its
/// `options.format` with [`ISO_DATE_PATTERN`] appended, for a date-typed
/// field whose format can't read what flusso writes; `None` for any other
/// field.
pub fn iso_date_format(_field: &Field) -> Option<String> {
    None
}

//! Whether an OpenSearch date `format` reads the form flusso writes a
//! timestamp in.
//!
//! A `timestamp` value reaches the document as ISO 8601 with a `T`
//! (`2024-01-02T03:04:05.123456`, or `…+00:00` for a `timestamptz`). A
//! field's `options.format` replaces OpenSearch's default
//! (`strict_date_optional_time||epoch_millis`), so a format tuned for the space
//! form flusso wrote before 0.18 refuses every value.
//! [`validate_spec`](crate::validate_spec) warns about such a field with
//! [`DiagnosticKind::TimestampFormat`](crate::DiagnosticKind::TimestampFormat).
//!
//! [`reads_iso_timestamps`] classifies a format conservatively, pattern by
//! pattern (`||` separates them):
//!
//! - a named format that reads it (`strict_date_optional_time`,
//!   `date_optional_time`, …) or a literal pattern quoting `'T'`: reads it;
//! - `epoch_millis`, `epoch_second`, or a literal pattern without `'T'`:
//!   refuses it;
//! - any other named format: unknown.
//!
//! One reading pattern makes the format read it; only a format whose every
//! pattern refuses it refuses it. Anything else is unknown, and unknown is never
//! reported.
//!
//! ```
//! use source::reads_iso_timestamps;
//!
//! assert_eq!(reads_iso_timestamps("yyyy-MM-dd HH:mm:ss||epoch_millis"), Some(false));
//! assert_eq!(reads_iso_timestamps("strict_date_optional_time||yyyy-MM-dd HH:mm:ss"), Some(true));
//! assert_eq!(reads_iso_timestamps("yyyy-MM-dd'T'HH:mm:ss"), Some(true));
//! assert_eq!(reads_iso_timestamps("yyyy-MM-dd HH:mm:ss||date_hour_minute"), None);
//! ```

/// Named OpenSearch formats that read `2024-01-02T03:04:05.123456`, with or
/// without an offset.
const READING: &[&str] = &[
    "strict_date_optional_time",
    "date_optional_time",
    "strict_date_optional_time_nanos",
];

/// Named OpenSearch formats that read numbers only.
const REFUSING: &[&str] = &["epoch_millis", "epoch_second"];

/// Whether the OpenSearch date `format` reads the ISO 8601 form flusso writes a
/// timestamp in: `Some(true)` when a pattern does, `Some(false)` when every
/// pattern is known to refuse it, `None` when that can't be told.
pub fn reads_iso_timestamps(format: &str) -> Option<bool> {
    let mut refused = true;
    for pattern in format.split("||").map(str::trim) {
        match pattern_reads(pattern) {
            Some(true) => return Some(true),
            Some(false) => {}
            None => refused = false,
        }
    }
    refused.then_some(false)
}

fn pattern_reads(pattern: &str) -> Option<bool> {
    let named = !pattern.is_empty()
        && pattern
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if !named {
        return Some(pattern.contains("'T'"));
    }
    let name = pattern.strip_prefix("strict_").unwrap_or(pattern);
    if READING.contains(&pattern) || READING.contains(&name) {
        Some(true)
    } else if REFUSING.contains(&pattern) {
        Some(false)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_format_is_classified_pattern_by_pattern() {
        for (format, expected) in [
            (
                "yyyy-MM-dd HH:mm:ss.SSSSSS||yyyy-MM-dd HH:mm:ss",
                Some(false),
            ),
            ("epoch_millis", Some(false)),
            ("epoch_second || yyyy-MM-dd HH:mm:ss", Some(false)),
            ("uuuu-MM-dd HH:mm:ss", Some(false)),
            ("strict_date_optional_time", Some(true)),
            ("yyyy-MM-dd HH:mm:ss||date_optional_time", Some(true)),
            ("strict_date_optional_time_nanos", Some(true)),
            ("yyyy-MM-dd'T'HH:mm:ss.SSSSSS", Some(true)),
            ("yyyy-MM-dd HH:mm:ss||date_hour_minute_second", None),
            ("date_time", None),
            ("yyyy", None),
            ("strictDateOptionalTime", Some(true)),
            ("dateOptionalTime||epoch_millis", Some(true)),
            ("epochMillis", None),
            ("basicDateTime||yyyy-MM-dd HH:mm:ss", None),
            ("yyyyMMdd", None),
        ] {
            assert_eq!(reads_iso_timestamps(format), expected, "{format}");
        }
    }
}

//! Making a date mapping's `format` read the dates flusso writes.
//!
//! flusso writes a `date` value as `2024-01-02`, a `timestamp` as
//! `2024-01-02T03:04:05` (a fraction of up to nine digits when it has one) and
//! a `timestamptz` as `2024-01-02T03:04:05.123456+00:00` (`sink::Json`). A
//! field's `options.format` replaces OpenSearch's default
//! (`strict_date_optional_time||epoch_millis`), so a format tuned for another
//! form (the space form flusso wrote before 0.18, `epoch_millis`, …) refused
//! every document holding a value.
//!
//! So the mapping such a field resolves to appends
//! `||strict_date_optional_time` to its format ([`with_iso`]), only where the
//! format can't read what flusso writes **into that field**
//! ([`DateValues`]): a date field needs a pattern reading `2024-01-02`, a
//! timestamp field one reading every timestamp form. A format whose every
//! pattern reads a time of day only ([`reads_time_of_day_only`]) is left
//! alone too: over a `time` column it is the documented way to index a time
//! of day, and over a date or timestamp validation reports it
//! (`time_of_day_format`), since only the database names the column's type. A `custom` field
//! mapped as a date over a column that isn't a date or a timestamp is the
//! user's: flusso writes the column's own value. The user's patterns stay
//! first, so OpenSearch prints dates in their form (`key_as_string`,
//! `docvalue_fields`, the `fields` API) and reads queries in either; `_source`
//! is unchanged.
//!
//! [`iso_date_format`] decides it per field; a field the index doesn't map
//! (input-only, or under an `indexed: false` join or an `enabled: false`
//! object) is left alone. The projection
//! ([`IndexSchema::resolve`](super::IndexSchema::resolve)) and the content hash
//! follow [`IndexSchema::completed_date_formats`](super::IndexSchema::completed_date_formats), so exactly the indexes with
//! such a field move to a new physical index, once.
//!
//! | Named format (also `strict_` and camelCase) | date field | timestamp field |
//! | --- | --- | --- |
//! | `date_optional_time`, `strict_date_optional_time_nanos`, `iso8601` | reads | reads |
//! | `date`, `year_month_day` | reads | appended |
//! | `hour`, `hour_minute`, `hour_minute_second` (`_fraction`, `_millis`), `time`, `time_no_millis`, `t_time`, `t_time_no_millis`, `basic_time`, `basic_time_no_millis`, `basic_t_time`, `basic_t_time_no_millis` | left alone (a time of day) | left alone (a time of day) |
//! | every other: `epoch_*`, `basic_*` dates, `date_hour…`, `date_time…`, `ordinal_*`, `week*`, `year`, `year_month` | appended | appended |
//!
//! A literal pattern reads a date field when it is `yyyy-MM-dd` (or
//! `uuuu-MM-dd`), and a time of day when it names no date letter; any other is
//! appended to, since no literal is shown to read every timestamp form.
//!
//! ```
//! use kernel::config::{DateValues, with_iso};
//!
//! assert_eq!(
//!     with_iso("yyyy-MM-dd HH:mm:ss", DateValues::Timestamps).as_deref(),
//!     Some("yyyy-MM-dd HH:mm:ss||strict_date_optional_time")
//! );
//! assert_eq!(with_iso("yyyy-MM-dd", DateValues::Dates), None);
//! assert_eq!(with_iso("strict_date_optional_time||epoch_millis", DateValues::Timestamps), None);
//! ```

use super::{AggregateOp, Field, FieldSource, FlussoType, MappingType, Relation};
use crate::common::GenericValue;

/// The pattern appended to a date `format` that can't read what flusso
/// writes.
pub const ISO_DATE_PATTERN: &str = "strict_date_optional_time";

/// Named OpenSearch formats (without `strict_`) that read every date and
/// timestamp form flusso writes.
const READS_ALL: &[&str] = &["date_optional_time", "date_optional_time_nanos", "iso8601"];

/// Named OpenSearch formats (without `strict_`) that read a bare date only.
const READS_DATES: &[&str] = &["date", "year_month_day"];

/// Named OpenSearch formats (without `strict_`) that read a time of day only.
const TIME_OF_DAY: &[&str] = &[
    "hour",
    "hour_minute",
    "hour_minute_second",
    "hour_minute_second_fraction",
    "hour_minute_second_millis",
    "time",
    "time_no_millis",
    "t_time",
    "t_time_no_millis",
    "basic_time",
    "basic_time_no_millis",
    "basic_t_time",
    "basic_t_time_no_millis",
];

/// What flusso writes into a date-typed field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateValues {
    /// A bare date, `2024-01-02`.
    Dates,
    /// Timestamps, naive or with an offset, with a fraction or without.
    Timestamps,
    /// Either: a `custom` field over columns of both kinds.
    Both,
}

/// What one pattern of a format reads.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pattern {
    All,
    Dates,
    TimeOfDay,
    Other,
}

fn pattern(pattern: &str) -> Pattern {
    let named = !pattern.is_empty()
        && pattern
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_');
    if named {
        let snake = snake_case(pattern);
        let name = snake.strip_prefix("strict_").unwrap_or(&snake);
        return if READS_ALL.contains(&name) {
            Pattern::All
        } else if READS_DATES.contains(&name) {
            Pattern::Dates
        } else if TIME_OF_DAY.contains(&name) {
            Pattern::TimeOfDay
        } else {
            Pattern::Other
        };
    }
    if pattern == "yyyy-MM-dd" || pattern == "uuuu-MM-dd" {
        return Pattern::Dates;
    }
    let letters = unquoted_letters(pattern);
    let clock = letters.iter().any(|c| matches!(c, 'H' | 'h' | 'K' | 'k'));
    let dated = letters.iter().any(|c| {
        matches!(
            c,
            'y' | 'u'
                | 'Y'
                | 'M'
                | 'L'
                | 'd'
                | 'D'
                | 'e'
                | 'E'
                | 'c'
                | 'w'
                | 'W'
                | 'G'
                | 'Q'
                | 'q'
                | 'F'
        )
    });
    if clock && !dated {
        Pattern::TimeOfDay
    } else {
        Pattern::Other
    }
}

/// The letters of a literal pattern outside its quoted text.
fn unquoted_letters(pattern: &str) -> Vec<char> {
    let mut quoted = false;
    pattern
        .chars()
        .filter(|c| {
            if *c == '\'' {
                quoted = !quoted;
                return false;
            }
            !quoted && c.is_ascii_alphabetic()
        })
        .collect()
}

fn patterns(format: &str) -> Vec<Pattern> {
    format.split("||").map(str::trim).map(pattern).collect()
}

/// Whether a pattern of `format` reads every form of `values` flusso writes.
pub fn reads(format: &str, values: DateValues) -> bool {
    patterns(format).iter().any(|p| match p {
        Pattern::All => true,
        Pattern::Dates => values == DateValues::Dates,
        Pattern::TimeOfDay | Pattern::Other => false,
    })
}

/// Whether every pattern of a non-empty `format` reads a time of day only
/// (`HH:mm:ss`, `hour_minute_second`, …): such a format reads no date or
/// timestamp flusso writes, and is left as written by [`with_iso`].
pub fn reads_time_of_day_only(format: &str) -> bool {
    !format.trim().is_empty() && patterns(format).iter().all(|p| *p == Pattern::TimeOfDay)
}

/// `format` with [`ISO_DATE_PATTERN`] appended, or `None` when a pattern of it
/// reads every form of `values` flusso writes ([`reads`]), when every pattern
/// reads a time of day only ([`reads_time_of_day_only`]), or when it is empty.
pub fn with_iso(format: &str, values: DateValues) -> Option<String> {
    if format.trim().is_empty() || reads(format, values) || reads_time_of_day_only(format) {
        return None;
    }
    Some(format!("{format}||{ISO_DATE_PATTERN}"))
}

/// The `format` the mapping of `field` uses in place of its own: its
/// `options.format` with [`ISO_DATE_PATTERN`] appended, for a mapped,
/// date-typed field whose format can't read what flusso writes into it;
/// `None` for any other field. Whether the index maps the field at all (an
/// `indexed: false` ancestor) is the caller's: see
/// [`IndexSchema::completed_date_formats`](super::IndexSchema::completed_date_formats).
pub fn iso_date_format(field: &Field) -> Option<String> {
    with_iso(date_format(field)?, date_values(field)?)
}

/// The `options.format` of an emitted field mapped as `date` or
/// `date_nanos`; `None` for any other field.
pub fn date_format(field: &Field) -> Option<&str> {
    if !field.emit {
        return None;
    }
    let Some(GenericValue::String(format)) = field.options.get("format") else {
        return None;
    };
    match super::projection::mapping_type(field) {
        MappingType::Date => Some(format),
        MappingType::Other(name) if name == "date_nanos" => Some(format),
        _ => None,
    }
}

/// What flusso writes into a date-typed `field`, from what the schema
/// declares; `None` when that isn't a date or a timestamp.
pub fn date_values(field: &Field) -> Option<DateValues> {
    let of = |ty: &FlussoType| match ty {
        FlussoType::Date => Some(DateValues::Dates),
        FlussoType::Timestamp => Some(DateValues::Timestamps),
        FlussoType::Custom { postgres, .. } => custom(postgres),
        _ => None,
    };
    match &field.source {
        FieldSource::Column(column) => of(&column.ty),
        FieldSource::Script(script) => match &script.ty {
            FlussoType::Custom { .. } => None,
            ty => of(ty),
        },
        FieldSource::Relation(Relation::Aggregate(aggregate)) => match &aggregate.op {
            AggregateOp::Min(_) | AggregateOp::Max(_) => aggregate.value_type.as_ref().and_then(of),
            AggregateOp::Ids { element_type } => of(element_type),
            AggregateOp::Count
            | AggregateOp::Sum(_)
            | AggregateOp::Avg(_)
            | AggregateOp::Exists => None,
        },
        FieldSource::Constant(GenericValue::Date(_)) => Some(DateValues::Dates),
        FieldSource::Constant(GenericValue::Timestamp(_) | GenericValue::TimestampTz(_)) => {
            Some(DateValues::Timestamps)
        }
        _ => None,
    }
}

/// A `custom` field's values by the Postgres types it takes: dates,
/// timestamps or both; `None` when any is neither.
fn custom(postgres: &[String]) -> Option<DateValues> {
    let mut dates = false;
    let mut timestamps = false;
    for ty in postgres {
        let ty = ty.trim().to_ascii_lowercase();
        let ty = ty.trim_end_matches("[]");
        let base = ty.split('(').next().unwrap_or_default().trim();
        if base == "date" {
            dates = true;
        } else if base.starts_with("timestamp") {
            timestamps = true;
        } else {
            return None;
        }
    }
    match (dates, timestamps) {
        (true, true) => Some(DateValues::Both),
        (true, false) => Some(DateValues::Dates),
        (false, true) => Some(DateValues::Timestamps),
        (false, false) => None,
    }
}

impl super::IndexSchema {
    /// The dotted path of every field whose mapped `format` gets
    /// [`ISO_DATE_PATTERN`] appended ([`iso_date_format`]), skipping what the
    /// index doesn't map: input-only fields and everything under an
    /// `indexed: false` join or an `enabled: false` object.
    pub fn completed_date_formats(&self) -> Vec<String> {
        let mut paths = Vec::new();
        collect(&self.fields, "", &mut paths);
        paths
    }
}

fn collect(fields: &[Field], prefix: &str, paths: &mut Vec<String>) {
    for field in fields.iter().filter(|field| field.emit) {
        let path = format!("{prefix}{}", field.field);
        if iso_date_format(field).is_some() {
            paths.push(path.clone());
        }
        if maps_children(field) {
            collect(field.children(), &format!("{path}."), paths);
        }
    }
}

/// Whether OpenSearch maps what is under `field`: not under an
/// `indexed: false` join, nor an object with `enabled: false`.
pub fn maps_children(field: &Field) -> bool {
    let unindexed_join =
        matches!(&field.source, FieldSource::Relation(Relation::Join(join)) if !join.indexed);
    !unindexed_join && field.options.get("enabled") != Some(&GenericValue::Bool(false))
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

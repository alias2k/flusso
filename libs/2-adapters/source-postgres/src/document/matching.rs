//! Matching the rows a batched document query returned to the ids it was
//! asked for.
//!
//! A row comes back keyed by the root key as the database reads it now; an
//! id carries the key as its change was decoded, perhaps before the column
//! changed type. The batched query finds a row for an id exactly when the
//! single-document query would (both compare `key = $n::<bound>::<key type>`),
//! so a row serves every requested id whose key, cast to the row's key type
//! as that query casts it, equals the row's key ([`cast`]). The document is
//! built under the requested id, as the single build writes it; only an id no
//! row serves is a tombstone.

use std::collections::HashMap;
use std::mem::{Discriminant, discriminant};

use chrono::{NaiveTime, TimeZone, Utc};
use kernel::GenericValue;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::{Decimal, RoundingStrategy};
use source::document::{BuildFailure, Document, DocumentId};

/// A row's assembled body, or why it could not be finished.
pub(super) type Outcome = Result<GenericValue, BuildFailure>;

/// Push one document per requested id onto `out`, in `requested`'s order,
/// each under its requested id. The first id a row serves takes its body; a
/// later one gets a copy.
///
/// An id is looked up by its key as decoded first, one hash lookup; only a
/// miss casts it into the other key kinds among the rows (as a key read
/// before its column changed type needs), one lookup each.
pub(super) fn match_rows(
    rows: Vec<(GenericValue, Outcome)>,
    requested: &[(&GenericValue, &DocumentId)],
    out: &mut Vec<Document>,
) {
    let (keys, outcomes): (Vec<GenericValue>, Vec<Outcome>) = rows.into_iter().unzip();
    let mut outcomes: Vec<Option<Outcome>> = outcomes.into_iter().map(Some).collect();
    let mut served_at: Vec<Option<usize>> = vec![None; keys.len()];
    let by_key: HashMap<&GenericValue, usize> = keys
        .iter()
        .enumerate()
        .map(|(row, key)| (key, row))
        .collect();
    let mut kinds: Vec<(Discriminant<GenericValue>, &GenericValue)> = Vec::new();
    for key in &keys {
        if !kinds.iter().any(|(kind, _)| *kind == discriminant(key)) {
            kinds.push((discriminant(key), key));
        }
    }
    for (value, id) in requested {
        let id = (*id).clone();
        let row = by_key.get(value).copied().or_else(|| {
            kinds
                .iter()
                .filter(|(kind, _)| *kind != discriminant(*value))
                .find_map(|(_, sample)| by_key.get(&cast(value, sample)?).copied())
        });
        let document = match row {
            None => Document::Delete { id },
            Some(row) => match outcomes.get_mut(row).and_then(Option::take) {
                Some(outcome) => {
                    if let Some(at) = served_at.get_mut(row) {
                        *at = Some(out.len());
                    }
                    match outcome {
                        Ok(body) => Document::Upsert { id, body },
                        Err(failure) => Document::Failed { id, failure },
                    }
                }
                None => match served_at
                    .get(row)
                    .copied()
                    .flatten()
                    .and_then(|at| out.get(at))
                {
                    Some(Document::Upsert { body, .. }) => Document::Upsert {
                        id,
                        body: body.clone(),
                    },
                    Some(Document::Failed { failure, .. }) => Document::Failed {
                        id,
                        failure: failure.clone(),
                    },
                    Some(Document::Delete { .. }) | None => Document::Delete { id },
                },
            },
        };
        out.push(document);
    }
}

/// `value` as the single-document query compares it with a key of `like`'s
/// kind: what `$n::<bound>::<key type>` makes of it in Postgres, the session
/// in UTC (sqlx sends `TimeZone=UTC` at startup). `None` when that cast fails
/// in Postgres (out of range, text it doesn't parse) or isn't mirrored here.
///
/// - Integers of every width and `numeric` compare as numbers: `numeric` →
///   integer rounds half away from zero, `float8` → integer half to even,
///   `float8` → `numeric` keeps 15 significant digits, a width the value
///   overflows fails.
/// - `date`, `timestamp` and `timestamptz` convert at UTC; a timestamp cast
///   to `date` drops its time.
/// - A row key of text compares with the value's text output.
/// - Text is parsed as the row key's type reads its own output
///   (`text_value::typed_value`); a `timestamp`/`timestamptz` also takes a
///   bare date.
/// - Any other pair (`real`, `uuid`, `bool`, `time`, `bytea`, a mismatch)
///   matches only within its own kind.
pub(super) fn cast(value: &GenericValue, like: &GenericValue) -> Option<GenericValue> {
    use GenericValue as V;
    if let V::String(text) = value
        && !matches!(like, V::String(_))
    {
        return parse(text, like);
    }
    Some(match like {
        V::SmallInt(_) => V::SmallInt(i16::try_from(integer(value)?).ok()?),
        V::Int(_) => V::Int(i32::try_from(integer(value)?).ok()?),
        V::BigInt(_) => V::BigInt(integer(value)?),
        V::Decimal(_) => V::Decimal(match value {
            V::Double(double) => Decimal::from_scientific(&format!("{double:.14e}")).ok()?,
            other => Decimal::from(integer_exact(other)?),
        }),
        V::Double(_) => V::Double(match value {
            V::Decimal(decimal) => decimal.to_string().parse().ok()?,
            other => integer_exact(other)? as f64,
        }),
        V::Date(_) => V::Date(match value {
            V::Timestamp(at) => at.date(),
            V::TimestampTz(at) => at.naive_utc().date(),
            _ => return None,
        }),
        V::Timestamp(_) => V::Timestamp(match value {
            V::Date(day) => day.and_time(NaiveTime::MIN),
            V::TimestampTz(at) => at.naive_utc(),
            _ => return None,
        }),
        V::TimestampTz(_) => V::TimestampTz(match value {
            V::Date(day) => Utc.from_utc_datetime(&day.and_time(NaiveTime::MIN)),
            V::Timestamp(at) => Utc.from_utc_datetime(at),
            _ => return None,
        }),
        V::String(_) => V::String(match value {
            V::SmallInt(_) | V::Int(_) | V::BigInt(_) | V::Decimal(_) | V::Date(_) => {
                source::document::key_part(value)
            }
            V::Double(double) => double.to_string(),
            V::Timestamp(at) => at.format("%Y-%m-%d %H:%M:%S%.f").to_string(),
            V::TimestampTz(at) => at.format("%Y-%m-%d %H:%M:%S%.f+00").to_string(),
            _ => return None,
        }),
        _ => return None,
    })
}

/// An integer or `numeric`, rounded to an integer as Postgres casts one.
fn integer(value: &GenericValue) -> Option<i64> {
    match value {
        GenericValue::Decimal(decimal) => decimal
            .round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero)
            .to_i64(),
        GenericValue::Double(double) => {
            let rounded = double.round_ties_even();
            (rounded.is_finite() && rounded.abs() < 9.2e18).then_some(rounded as i64)
        }
        other => integer_exact(other),
    }
}

/// An integer of any width.
fn integer_exact(value: &GenericValue) -> Option<i64> {
    match value {
        GenericValue::SmallInt(n) => Some(i64::from(*n)),
        GenericValue::Int(n) => Some(i64::from(*n)),
        GenericValue::BigInt(n) => Some(*n),
        _ => None,
    }
}

/// `text` as a key of `like`'s kind reads it.
fn parse(text: &str, like: &GenericValue) -> Option<GenericValue> {
    let oid = match like {
        GenericValue::SmallInt(_) => 21,
        GenericValue::Int(_) => 23,
        GenericValue::BigInt(_) => 20,
        GenericValue::Decimal(_) => 1700,
        GenericValue::Double(_) => 701,
        GenericValue::Date(_) => 1082,
        GenericValue::Timestamp(_) => 1114,
        GenericValue::TimestampTz(_) => 1184,
        _ => return None,
    };
    let parsed = crate::text_value::typed_value(text, oid);
    if discriminant(&parsed) == discriminant(like) {
        return Some(parsed);
    }
    match crate::text_value::typed_value(text, 1082) {
        day @ GenericValue::Date(_) => cast(&day, like),
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;

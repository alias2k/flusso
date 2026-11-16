//! Reading a date the way the index reads it.
//!
//! A `date` field keeps milliseconds. What a piece of text leaves unsaid, the
//! index fills in differently on each side of a comparison, so a date constant
//! is read as the stretch of time it covers: a bare date is its whole day,
//! `12:00` its whole minute, and anything written down to the second is one
//! instant.

use serde_json::Value;

const SECOND: i64 = 1_000;
const MINUTE: i64 = 60 * SECOND;
const DAY: i64 = 24 * 60 * MINUTE;

/// The milliseconds a date covers, both ends included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Span {
    pub(crate) first: i64,
    pub(crate) last: i64,
}

/// An ISO-8601 date or timestamp, or epoch milliseconds, as the time it covers.
///
/// Digits past the millisecond are dropped and a missing zone is UTC, as the
/// index does for both. `None` for anything else.
pub(crate) fn span(value: &Value) -> Option<Span> {
    match value {
        Value::Number(number) => number.as_i64().map(instant),
        Value::String(text) => parse(text),
        _ => None,
    }
}

fn instant(millis: i64) -> Span {
    Span {
        first: millis,
        last: millis,
    }
}

fn parse(text: &str) -> Option<Span> {
    let text = text.trim();
    if text.len() > 4 && text.bytes().all(|byte| byte.is_ascii_digit()) {
        return text.parse().ok().map(instant);
    }
    let year: i64 = text.get(0..4)?.parse().ok()?;
    let month: i64 = text.get(5..7)?.parse().ok()?;
    let day: i64 = text.get(8..10)?.parse().ok()?;
    if text.get(4..5)? != "-" || text.get(7..8)? != "-" {
        return None;
    }
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let midnight = days_from_civil(year, month, day) * DAY;
    let rest = text.get(10..)?;
    if rest.is_empty() {
        return Some(Span {
            first: midnight,
            last: midnight + DAY - 1,
        });
    }
    let rest = rest.strip_prefix('T').or_else(|| rest.strip_prefix(' '))?;
    let zone_at = rest.find(['Z', '+', '-']).unwrap_or(rest.len());
    let (time, zone) = rest.split_at(zone_at);
    let hour: i64 = time.get(0..2)?.parse().ok()?;
    let minute: i64 = time.get(3..5)?.parse().ok()?;
    if time.get(2..3)? != ":" || hour > 23 || minute > 59 {
        return None;
    }
    let (second, open) = match time.get(5..) {
        None | Some("") => (0, MINUTE),
        Some(seconds) => {
            if seconds.get(0..1)? != ":" {
                return None;
            }
            (seconds.get(1..3)?.parse::<i64>().ok()?, 1)
        }
    };
    if second > 59 {
        return None;
    }
    let millis = match time.get(8..) {
        None | Some("") => 0,
        Some(fraction) => {
            let digits = fraction.strip_prefix('.')?;
            if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            format!("{digits:0<3}").get(0..3)?.parse::<i64>().ok()?
        }
    };
    let first =
        midnight + (hour * 60 + minute - offset_minutes(zone)?) * MINUTE + second * SECOND + millis;
    Some(Span {
        first,
        last: first + open - 1,
    })
}

/// A zone suffix (`Z`, `+02:00`, `-0530`, `+02`) as minutes east of UTC.
fn offset_minutes(zone: &str) -> Option<i64> {
    let (sign, digits) = match zone {
        "" | "Z" => return Some(0),
        zone => match zone.split_at_checked(1)? {
            ("+", digits) => (1, digits),
            ("-", digits) => (-1, digits),
            _ => return None,
        },
    };
    let digits: String = digits.chars().filter(|ch| *ch != ':').collect();
    if !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let hours: i64 = digits.get(0..2)?.parse().ok()?;
    let minutes: i64 = match digits.len() {
        2 => 0,
        4 => digits.get(2..4)?.parse().ok()?,
        _ => return None,
    };
    Some(sign * (hours * 60 + minutes))
}

/// Days since 1970-01-01 of a proleptic Gregorian date.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let month_index = (month + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use serde_json::json;

    use super::*;

    const NOON: i64 = 1_794_830_400_000; // 2026-11-16T12:00:00Z

    fn read(text: &str) -> Span {
        span(&json!(text)).unwrap()
    }

    #[test]
    fn a_timestamp_to_the_second_is_one_instant() {
        for text in [
            "2026-11-16T12:00:00Z",
            "2026-11-16T12:00:00",
            "2026-11-16 12:00:00",
            "2026-11-16T12:00:00+00:00",
            "2026-11-16T14:00:00+02:00",
            "2026-11-16T06:30:00-0530",
            "2026-11-16T13:00:00+01",
            "2026-11-16T12:00:00.000000",
        ] {
            assert_eq!(read(text), instant(NOON), "{text}");
        }
    }

    #[test]
    fn digits_past_the_millisecond_are_dropped() {
        assert_eq!(read("2026-11-16T12:00:00.1239999Z"), instant(NOON + 123));
        assert_eq!(read("2026-11-16T12:00:00.5"), instant(NOON + 500));
        assert_eq!(read("1969-12-31T23:59:59.9999Z"), instant(-1));
    }

    #[test]
    fn what_the_text_leaves_open_is_covered() {
        let day = read("2026-11-16");
        assert_eq!(day.first, NOON - 12 * 60 * MINUTE);
        assert_eq!(day.last, day.first + DAY - 1);
        let minute = read("2026-11-16T12:00Z");
        assert_eq!((minute.first, minute.last), (NOON, NOON + MINUTE - 1));
    }

    #[test]
    fn epoch_milliseconds_read_as_text_or_number() {
        assert_eq!(read("1794830400000"), instant(NOON));
        assert_eq!(span(&json!(NOON)), Some(instant(NOON)));
    }

    #[test]
    fn anything_else_is_not_a_date() {
        for text in [
            "next tuesday",
            "2026-13-01",
            "2026-11-16T25:00:00Z",
            "2026/11/16",
            "2026-11-16T12:00:00.Z",
            "2026-11-16T12:00:00+2",
            "",
        ] {
            assert_eq!(span(&json!(text)), None, "{text}");
        }
        assert_eq!(span(&json!(true)), None);
    }
}

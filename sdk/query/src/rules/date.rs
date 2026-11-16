//! Reading a date the way the index reads it.
//!
//! A `date` field keeps milliseconds. What a piece of text leaves unsaid, the
//! index fills in differently on each side of a comparison, so a date constant
//! is read as the stretch of time it covers: a bare date is its whole day,
//! `12:00` its whole minute, and anything written down to the second is one
//! instant. A date the document holds is read from the start of its stretch,
//! as the index stores it.
//!
//! ```text
//! "2026-11-16T12:00:00.123456Z"  one instant, to the millisecond
//! "2026-11-16T12:00"             12:00:00.000 ..= 12:00:59.999
//! "2026-11-16"                   the whole day
//! "2026-11"                      the whole month
//! "1794830400000"                epoch milliseconds
//! ```

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

/// The forms the index's own date format reads: `yyyy`, `yyyy-MM`,
/// `yyyy-MM-dd`, then optionally `T` and `HH`, `HH:mm`, `HH:mm:ss` or
/// `HH:mm:ss.S…`, then optionally a zone. Longer runs of digits are epoch
/// milliseconds.
fn parse(text: &str) -> Option<Span> {
    let text = text.trim();
    if text.len() > 4 && text.bytes().all(|byte| byte.is_ascii_digit()) {
        return text.parse().ok().map(instant);
    }
    let (date, time) = match text.find(['T', ' ']) {
        Some(at) => (text.get(..at)?, Some(text.get(at + 1..)?)),
        None => (text, None),
    };
    let mut parts = date.split('-');
    let year = digits(parts.next()?, 4)?;
    let month = part(parts.next(), 2)?;
    let day = part(parts.next(), 2)?;
    if parts.next().is_some() {
        return None;
    }
    let first_month = month.unwrap_or(1);
    let first_day = day.unwrap_or(1);
    if !(1..=12).contains(&first_month) || !(1..=days_in(year, first_month)).contains(&first_day) {
        return None;
    }
    let midnight = days_from_civil(year, first_month, first_day) * DAY;
    let Some(time) = time else {
        // What the date leaves open runs to the start of the next one.
        let next = match (month, day) {
            (None, _) => days_from_civil(year + 1, 1, 1) * DAY,
            (Some(12), None) => days_from_civil(year + 1, 1, 1) * DAY,
            (Some(month), None) => days_from_civil(year, month + 1, 1) * DAY,
            (Some(_), Some(_)) => midnight + DAY,
        };
        return Some(Span {
            first: midnight,
            last: next - 1,
        });
    };
    // A time needs a whole date before it.
    day?;
    let zone_at = time.find(['Z', '+', '-']).unwrap_or(time.len());
    let (clock, zone) = time.split_at(zone_at);
    let (clock, fraction) = match clock.split_once('.') {
        Some((clock, fraction)) => (clock, Some(fraction)),
        None => (clock, None),
    };
    let mut parts = clock.split(':');
    let hour = digits(parts.next()?, 2)?;
    let minute = part(parts.next(), 2)?;
    let second = part(parts.next(), 2)?;
    if parts.next().is_some() || hour > 23 || minute.is_some_and(|minute| minute > 59) {
        return None;
    }
    if second.is_some_and(|second| second > 59) || (fraction.is_some() && second.is_none()) {
        return None;
    }
    let millis = match fraction {
        None => 0,
        Some(fraction) => {
            if fraction.is_empty() || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            format!("{fraction:0<3}").get(0..3)?.parse::<i64>().ok()?
        }
    };
    let open = match (minute, second) {
        (None, _) => 60 * MINUTE,
        (Some(_), None) => MINUTE,
        (Some(_), Some(_)) => 1,
    };
    let first = midnight
        + (hour * 60 + minute.unwrap_or(0) - offset_minutes(zone)?) * MINUTE
        + second.unwrap_or(0) * SECOND
        + millis;
    Some(Span {
        first,
        last: first + open - 1,
    })
}

/// An optional part: `Some(None)` when absent, `None` when present and not
/// `width` digits.
fn part(text: Option<&str>, width: usize) -> Option<Option<i64>> {
    match text {
        Some(text) => digits(text, width).map(Some),
        None => Some(None),
    }
}

/// `text` as a number, when it is exactly `width` digits.
fn digits(text: &str, width: usize) -> Option<i64> {
    (text.len() == width && text.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}

fn days_in(year: i64, month: i64) -> i64 {
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
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
    fn a_year_a_month_and_an_hour_cover_themselves() {
        let year = read("2026");
        assert_eq!(year.first, read("2026-01-01").first);
        assert_eq!(year.last, read("2026-12-31").last);
        let month = read("2026-02");
        assert_eq!(month.first, read("2026-02-01").first);
        assert_eq!(month.last, read("2026-02-28").last);
        assert_eq!(read("2026-12").last, year.last);
        assert_eq!(read("2024-02").last, read("2024-02-29").last);
        let hour = read("2026-11-16T12Z");
        assert_eq!((hour.first, hour.last), (NOON, NOON + 60 * MINUTE - 1));
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
            "2026-02-31",
            "2025-02-29",
            "2026-1-01",
            "2026-11T12:00",
            "2026-11-16T",
            "2026-11-16T12:00.5",
            "2026-11-16T12:00:00:00",
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

//! Typed dates in scripts: the four Rhai types a `date`, `time` or `timestamp` value becomes,
//! and every rule they follow.
//!
//! | Rhai type | Rust | Holds |
//! | --- | --- | --- |
//! | `date` | [`Date`] | a calendar day |
//! | `time` | [`Time`] | a time of day, to the nanosecond |
//! | `timestamp` | [`Timestamp`] | a wall-clock date and time, with or without a [`Zone`] |
//! | `duration` | [`Duration`] | an exact length of time: what `a - b` returns |
//!
//! A [`Timestamp`] keeps its **wall clock** and, when zoned, the offset in effect there, so
//! reading a part never looks up the zone database; only moving it (`in_zone`, a calendar
//! add, rounding) does.
//!
//! The rules, each in one place:
//!
//! - **Calendar units** (days, weeks, months, years, rounding) move the wall clock, then
//!   [`Zone::resolve`] turns it back into an instant: a time in a daylight-saving **gap** moves
//!   forward by the gap's length, a time in an **overlap** takes the earlier instant.
//! - **Clock units** (hours, minutes, seconds) add exact elapsed time to a zoned timestamp.
//! - **Months** clamp to the month's last day when added; [`Date::months_until`] counts whole
//!   months as Postgres's `age()` does, without clamping.
//! - **Comparing** needs two values of one kind; two zoned timestamps compare as instants.
//! - Nothing reads the clock: a script gives the same answer on every rebuild.
//!
//! The Rhai registration is [`library`]; the conversion from and to document values is in
//! [`crate::value`].
//!
//! ```
//! use chrono::NaiveDate;
//! use script::dates::{Date, Zone};
//!
//! let day = Date(NaiveDate::from_ymd_opt(2024, 1, 31).unwrap());
//! assert_eq!(day.add_months(1).unwrap().to_string(), "2024-02-29");
//! assert_eq!(Zone::parse("Europe/Rome").unwrap().to_string(), "Europe/Rome");
//! ```

use std::cmp::Ordering;
use std::fmt::{self, Write as _};
use std::str::FromStr;

use chrono::format::{Fixed, Item, StrftimeItems};
use chrono::{
    DateTime, Datelike, FixedOffset, Months, NaiveDate, NaiveDateTime, NaiveTime, Offset,
    SecondsFormat, TimeDelta, Timelike, Utc,
};
use jiff::tz::AmbiguousOffset;

pub mod library;

/// A script error: one line, shown with the script's line and column.
pub type Failure = String;

/// A calendar day: what a `date` value is in a script.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date(pub NaiveDate);

/// A time of day: what a `time` value is in a script.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Time(pub NaiveTime);

/// An exact length of time, what the difference of two dates, times or timestamps is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Duration(pub TimeDelta);

/// A wall-clock date and time, with or without a zone: what a `timestamp` value is.
#[derive(Debug, Clone)]
pub struct Timestamp {
    wall: NaiveDateTime,
    zone: Option<Zoned>,
}

/// The zone of a zoned timestamp and the offset in effect at its wall clock.
#[derive(Debug, Clone)]
struct Zoned {
    zone: Zone,
    offset: FixedOffset,
}

/// Where a timestamp's wall clock is read: UTC, a fixed offset, or an IANA zone.
#[derive(Debug, Clone)]
pub enum Zone {
    /// UTC: every `timestamptz` value.
    Utc,
    /// A fixed offset, as parsed from `+02:00`.
    Fixed(FixedOffset),
    /// A named zone from the IANA database bundled into flusso (jiff's).
    Named(jiff::tz::TimeZone),
}

/// A period the rounding functions round to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    Day,
    Week,
    Month,
    Quarter,
    Year,
}

fn out_of_range() -> Failure {
    "date out of range".to_owned()
}

fn timestamp_out_of_range() -> Failure {
    "timestamp out of range".to_owned()
}

fn months(n: i64) -> Result<i32, Failure> {
    i32::try_from(n).map_err(|_| out_of_range())
}

/// `day` moved by `n` months, clamped to the last day of the month it lands in.
fn add_months_to(day: NaiveDate, n: i64) -> Result<NaiveDate, Failure> {
    let n = months(n)?;
    let step = Months::new(n.unsigned_abs());
    if n >= 0 {
        day.checked_add_months(step)
    } else {
        day.checked_sub_months(step)
    }
    .ok_or_else(out_of_range)
}

fn delta(n: i64, unit: fn(i64) -> Option<TimeDelta>) -> Result<TimeDelta, Failure> {
    unit(n).ok_or_else(|| format!("{n} is out of range for this unit"))
}

fn quarter(day: NaiveDate) -> u32 {
    (day.month() - 1) / 3 + 1
}

/// The first day of the period holding `day`.
fn period_start(day: NaiveDate, period: Period) -> Option<NaiveDate> {
    match period {
        Period::Day => Some(day),
        Period::Week => day.checked_sub_signed(TimeDelta::days(i64::from(
            day.weekday().num_days_from_monday(),
        ))),
        Period::Month => day.with_day(1),
        Period::Quarter => NaiveDate::from_ymd_opt(day.year(), (quarter(day) - 1) * 3 + 1, 1),
        Period::Year => NaiveDate::from_ymd_opt(day.year(), 1, 1),
    }
}

/// The first day of the period after the one holding `day`.
fn next_period_start(day: NaiveDate, period: Period) -> Option<NaiveDate> {
    let start = period_start(day, period)?;
    match period {
        Period::Day => start.succ_opt(),
        Period::Week => start.checked_add_signed(TimeDelta::days(7)),
        Period::Month => start.checked_add_months(Months::new(1)),
        Period::Quarter => start.checked_add_months(Months::new(3)),
        Period::Year => start.checked_add_months(Months::new(12)),
    }
}

/// Whole months from `start` to `end`, as Postgres's `age()` and java.time count them: the
/// difference in calendar months, less one while the end's day of month and time of day
/// haven't reached the start's (in the direction of travel). No clamping: 31 January to
/// 28 February is no month, and so is the way back.
fn whole_months(start: NaiveDateTime, end: NaiveDateTime) -> i64 {
    let total = month_index(end.date()) - month_index(start.date());
    let rest = |at: NaiveDateTime| (at.day(), at.time());
    match total.cmp(&0) {
        Ordering::Greater if rest(end) < rest(start) => total - 1,
        Ordering::Less if rest(end) > rest(start) => total + 1,
        _ => total,
    }
}

fn month_index(day: NaiveDate) -> i64 {
    i64::from(day.year()) * 12 + i64::from(day.month0())
}

/// A strftime pattern, checked: an unknown specifier fails here, not while writing.
fn pattern(text: &str) -> Result<Vec<Item<'_>>, Failure> {
    StrftimeItems::new(text)
        .parse()
        .map_err(|_| format!("bad format pattern '{text}'"))
}

/// Write a value through a pattern; a pattern asking for a part the value doesn't have fails.
fn write_pattern(
    text: &str,
    kind: &str,
    write: impl FnOnce(&[Item<'_>], &mut String) -> fmt::Result,
) -> Result<String, Failure> {
    let items = pattern(text)?;
    let mut out = String::new();
    write(&items, &mut out)
        .map_err(|_| format!("pattern '{text}' asks for a part a {kind} doesn't have"))?;
    Ok(out)
}

fn unparsable(text: &str, what: &str, pattern: Option<&str>, reason: impl fmt::Display) -> Failure {
    match pattern {
        Some(pattern) => format!("'{text}' is not a {what} matching '{pattern}': {reason}"),
        None => format!("'{text}' is not a {what}: {reason}"),
    }
}

/// `message` with the Rust names of the date types replaced by the names scripts know them by.
/// Rhai maps registered names in most messages but not in all (a wrong index type names the
/// type it found as Rust spells it).
pub(crate) fn script_names(message: String) -> String {
    let names = [
        (std::any::type_name::<Timestamp>(), "timestamp"),
        (std::any::type_name::<Duration>(), "duration"),
        (std::any::type_name::<Date>(), "date"),
        (std::any::type_name::<Time>(), "time"),
    ];
    if !names.iter().any(|(rust, _)| message.contains(rust)) {
        return message;
    }
    names
        .iter()
        .fold(message, |message, (rust, name)| message.replace(rust, name))
}

// ── Date ─────────────────────────────────────────────────────────────────────

impl Date {
    /// `date(y, m, d)`.
    pub fn new(year: i64, month: i64, day: i64) -> Result<Self, Failure> {
        let ymd = (
            i32::try_from(year).ok(),
            u32::try_from(month).ok(),
            u32::try_from(day).ok(),
        );
        match ymd {
            (Some(y), Some(m), Some(d)) => NaiveDate::from_ymd_opt(y, m, d).map(Self),
            _ => None,
        }
        .ok_or_else(|| format!("no such date: {year}-{month:02}-{day:02}"))
    }

    /// `parse_date(text)`: `YYYY-MM-DD`.
    pub fn parse(text: &str) -> Result<Self, Failure> {
        NaiveDate::from_str(text)
            .map(Self)
            .map_err(|e| unparsable(text, "date", None, e))
    }

    /// `parse_date(text, pattern)`.
    pub fn parse_with(text: &str, with: &str) -> Result<Self, Failure> {
        pattern(with)?;
        NaiveDate::parse_from_str(text, with)
            .map(Self)
            .map_err(|e| unparsable(text, "date", Some(with), e))
    }

    pub fn add_days(self, n: i64) -> Result<Self, Failure> {
        self.add(delta(n, TimeDelta::try_days)?)
    }

    pub fn add_weeks(self, n: i64) -> Result<Self, Failure> {
        self.add(delta(n, TimeDelta::try_weeks)?)
    }

    pub fn add_months(self, n: i64) -> Result<Self, Failure> {
        add_months_to(self.0, n).map(Self)
    }

    pub fn add_years(self, n: i64) -> Result<Self, Failure> {
        self.add_months(n.checked_mul(12).ok_or_else(out_of_range)?)
    }

    fn add(self, by: TimeDelta) -> Result<Self, Failure> {
        self.0
            .checked_add_signed(by)
            .map(Self)
            .ok_or_else(out_of_range)
    }

    /// `a.months_until(b)`.
    pub fn months_until(self, end: Self) -> i64 {
        whole_months(
            self.0.and_time(NaiveTime::MIN),
            end.0.and_time(NaiveTime::MIN),
        )
    }

    /// The first day of `period`.
    pub fn start_of(self, period: Period) -> Result<Self, Failure> {
        period_start(self.0, period)
            .map(Self)
            .ok_or_else(out_of_range)
    }

    /// The last day of `period`.
    pub fn end_of(self, period: Period) -> Result<Self, Failure> {
        next_period_start(self.0, period)
            .and_then(|next| next.pred_opt())
            .map(Self)
            .ok_or_else(out_of_range)
    }

    /// `d.at_time(t)`: a timestamp without a zone.
    pub fn at_time(self, time: Time) -> Timestamp {
        Timestamp::plain(self.0.and_time(time.0))
    }

    pub fn format(self, with: &str) -> Result<String, Failure> {
        write_pattern(with, "date", |items, out| {
            write!(out, "{}", self.0.format_with_items(items.iter()))
        })
    }

    pub fn year(self) -> i64 {
        i64::from(self.0.year())
    }
    pub fn month(self) -> i64 {
        i64::from(self.0.month())
    }
    pub fn day(self) -> i64 {
        i64::from(self.0.day())
    }
    /// 1 Monday … 7 Sunday.
    pub fn weekday(self) -> i64 {
        i64::from(self.0.weekday().number_from_monday())
    }
    pub fn day_of_year(self) -> i64 {
        i64::from(self.0.ordinal())
    }
    pub fn iso_week(self) -> i64 {
        i64::from(self.0.iso_week().week())
    }
    pub fn iso_year(self) -> i64 {
        i64::from(self.0.iso_week().year())
    }
    pub fn quarter(self) -> i64 {
        i64::from(quarter(self.0))
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.format("%Y-%m-%d"))
    }
}

// ── Time ─────────────────────────────────────────────────────────────────────

impl Time {
    /// `time(h, m, s)`.
    pub fn new(hour: i64, minute: i64, second: i64) -> Result<Self, Failure> {
        let hms = (
            u32::try_from(hour).ok(),
            u32::try_from(minute).ok(),
            u32::try_from(second).ok(),
        );
        match hms {
            (Some(h), Some(m), Some(s)) => NaiveTime::from_hms_opt(h, m, s).map(Self),
            _ => None,
        }
        .ok_or_else(|| format!("no such time: {hour:02}:{minute:02}:{second:02}"))
    }

    /// `parse_time(text)`: `HH:MM`, `HH:MM:SS`, with a fraction.
    pub fn parse(text: &str) -> Result<Self, Failure> {
        NaiveTime::from_str(text)
            .or_else(|e| NaiveTime::parse_from_str(text, "%H:%M").map_err(|_| e))
            .map(Self)
            .map_err(|e| unparsable(text, "time", None, e))
    }

    /// `parse_time(text, pattern)`.
    pub fn parse_with(text: &str, with: &str) -> Result<Self, Failure> {
        pattern(with)?;
        NaiveTime::parse_from_str(text, with)
            .map(Self)
            .map_err(|e| unparsable(text, "time", Some(with), e))
    }

    /// Moved by `n` of `unit`, wrapping around midnight like a clock.
    pub fn add(self, n: i64, unit: fn(i64) -> Option<TimeDelta>) -> Result<Self, Failure> {
        Ok(Self(self.0.overflowing_add_signed(delta(n, unit)?).0))
    }

    pub fn format(self, with: &str) -> Result<String, Failure> {
        write_pattern(with, "time", |items, out| {
            write!(out, "{}", self.0.format_with_items(items.iter()))
        })
    }

    pub fn hour(self) -> i64 {
        i64::from(self.0.hour())
    }
    pub fn minute(self) -> i64 {
        i64::from(self.0.minute())
    }
    pub fn second(self) -> i64 {
        i64::from(self.0.second())
    }
    pub fn millisecond(self) -> i64 {
        i64::from(self.0.nanosecond() % 1_000_000_000 / 1_000_000)
    }
    pub fn nanosecond(self) -> i64 {
        i64::from(self.0.nanosecond() % 1_000_000_000)
    }
}

impl fmt::Display for Time {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.format("%H:%M:%S%.f"))
    }
}

// ── Zone ─────────────────────────────────────────────────────────────────────

/// A UTC instant as jiff spells it.
fn jiff_instant(utc: NaiveDateTime) -> Option<jiff::Timestamp> {
    let utc = utc.and_utc();
    let nanos = i32::try_from(utc.timestamp_subsec_nanos()).ok()?;
    jiff::Timestamp::new(utc.timestamp(), nanos).ok()
}

/// A wall clock as jiff spells it.
fn jiff_civil(wall: NaiveDateTime) -> Option<jiff::civil::DateTime> {
    let part = |n: u32| i8::try_from(n).ok();
    jiff::civil::DateTime::new(
        i16::try_from(wall.year()).ok()?,
        part(wall.month())?,
        part(wall.day())?,
        part(wall.hour())?,
        part(wall.minute())?,
        part(wall.second())?,
        i32::try_from(wall.nanosecond()).ok()?,
    )
    .ok()
}

fn fixed(offset: jiff::tz::Offset) -> Result<FixedOffset, Failure> {
    FixedOffset::east_opt(offset.seconds()).ok_or_else(timestamp_out_of_range)
}

impl Zone {
    /// An IANA name (`Europe/Rome`, `UTC`, any case) or a fixed offset (`+02:00`).
    pub fn parse(name: &str) -> Result<Self, Failure> {
        if name.eq_ignore_ascii_case("UTC") {
            return Ok(Self::Utc);
        }
        if let Ok(offset) = FixedOffset::from_str(name) {
            return Ok(Self::Fixed(offset));
        }
        jiff::tz::TimeZone::get(name).map(Self::Named).map_err(|_| {
            format!(
                "unknown time zone '{name}' (an IANA name like 'Europe/Rome', \
                     or an offset like '+02:00')"
            )
        })
    }

    /// The offset in effect at the UTC instant `utc`.
    fn offset_at(&self, utc: NaiveDateTime) -> Result<FixedOffset, Failure> {
        match self {
            Self::Utc => Ok(Utc.fix()),
            Self::Fixed(offset) => Ok(*offset),
            Self::Named(tz) => {
                fixed(tz.to_offset(jiff_instant(utc).ok_or_else(timestamp_out_of_range)?))
            }
        }
    }

    /// The instant `utc`, read in this zone.
    fn at_instant(&self, utc: NaiveDateTime) -> Result<Timestamp, Failure> {
        let offset = self.offset_at(utc)?;
        let wall = utc
            .checked_add_offset(offset)
            .ok_or_else(timestamp_out_of_range)?;
        Ok(Timestamp {
            wall,
            zone: Some(Zoned {
                zone: self.clone(),
                offset,
            }),
        })
    }

    /// The wall clock `wall` in this zone: in a gap, moved forward by the gap's length; in an
    /// overlap, the earlier instant.
    pub fn resolve(&self, wall: NaiveDateTime) -> Result<Timestamp, Failure> {
        let Self::Named(tz) = self else {
            return Ok(Timestamp {
                wall,
                zone: Some(Zoned {
                    zone: self.clone(),
                    offset: self.offset_at(wall)?,
                }),
            });
        };
        let civil = jiff_civil(wall).ok_or_else(timestamp_out_of_range)?;
        let offset = match tz.to_ambiguous_timestamp(civil).offset() {
            AmbiguousOffset::Unambiguous { offset }
            | AmbiguousOffset::Fold { before: offset, .. } => fixed(offset)?,
            AmbiguousOffset::Gap { before, .. } => {
                let utc = wall
                    .checked_sub_offset(fixed(before)?)
                    .ok_or_else(timestamp_out_of_range)?;
                return self.at_instant(utc);
            }
        };
        Ok(Timestamp {
            wall,
            zone: Some(Zoned {
                zone: self.clone(),
                offset,
            }),
        })
    }

    /// The zone's abbreviation at `utc` (`CET`), for a pattern's `%Z`.
    fn abbreviation(&self, utc: NaiveDateTime, offset: FixedOffset) -> String {
        match (self, jiff_instant(utc)) {
            (Self::Utc, _) => "UTC".to_owned(),
            (Self::Named(tz), Some(at)) => tz.to_offset_info(at).abbreviation().to_owned(),
            _ => offset.to_string(),
        }
    }
}

impl fmt::Display for Zone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Utc => f.write_str("UTC"),
            Self::Fixed(offset) => write!(f, "{offset}"),
            Self::Named(tz) => f.write_str(tz.iana_name().unwrap_or("?")),
        }
    }
}

/// An ISO date and time with an offset RFC 3339 doesn't take: Postgres's `::text` form
/// (`2024-05-01 10:00:00+02`) or a compact `+0200`, with a `T` or a space.
pub(crate) fn with_short_offset(text: &str) -> Option<DateTime<FixedOffset>> {
    ["%Y-%m-%d %H:%M:%S%.f%#z", "%Y-%m-%dT%H:%M:%S%.f%#z"]
        .into_iter()
        .find_map(|form| DateTime::parse_from_str(text, form).ok())
}

// ── Timestamp ────────────────────────────────────────────────────────────────

impl Timestamp {
    /// A wall clock with no zone: a Postgres `timestamp`.
    pub fn plain(wall: NaiveDateTime) -> Self {
        Self { wall, zone: None }
    }

    /// A UTC instant: a Postgres `timestamptz`.
    pub fn utc(instant: DateTime<Utc>) -> Self {
        Self {
            wall: instant.naive_utc(),
            zone: Some(Zoned {
                zone: Zone::Utc,
                offset: Utc.fix(),
            }),
        }
    }

    /// `timestamp(y, mo, d, h, mi, s)`.
    pub fn new(
        year: i64,
        month: i64,
        day: i64,
        hour: i64,
        minute: i64,
        second: i64,
    ) -> Result<Self, Failure> {
        let date = Date::new(year, month, day);
        let time = Time::new(hour, minute, second);
        match (date, time) {
            (Ok(date), Ok(time)) => Ok(date.at_time(time)),
            _ => Err(format!(
                "no such timestamp: {year}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}"
            )),
        }
    }

    /// `parse_timestamp(text)`: ISO with an offset (zoned) or without one (no zone).
    pub fn parse(text: &str) -> Result<Self, Failure> {
        if let Ok(parsed) = DateTime::parse_from_rfc3339(text) {
            return Ok(Self::with_offset(parsed, text.ends_with(['Z', 'z'])));
        }
        if let Some(parsed) = with_short_offset(text) {
            return Ok(Self::with_offset(parsed, false));
        }
        NaiveDateTime::from_str(text)
            .or_else(|e| NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S%.f").map_err(|_| e))
            .map(Self::plain)
            .map_err(|e| unparsable(text, "timestamp", None, e))
    }

    /// `parse_timestamp(text, pattern)`: zoned when the pattern reads an offset.
    pub fn parse_with(text: &str, with: &str) -> Result<Self, Failure> {
        pattern(with)?;
        if let Ok(parsed) = DateTime::parse_from_str(text, with) {
            return Ok(Self::with_offset(parsed, false));
        }
        NaiveDateTime::parse_from_str(text, with)
            .map(Self::plain)
            .map_err(|e| unparsable(text, "timestamp", Some(with), e))
    }

    fn with_offset(parsed: DateTime<FixedOffset>, utc: bool) -> Self {
        let zone = if utc {
            Zone::Utc
        } else {
            Zone::Fixed(*parsed.offset())
        };
        Self {
            wall: parsed.naive_local(),
            zone: Some(Zoned {
                zone,
                offset: *parsed.offset(),
            }),
        }
    }

    /// The wall clock: the parts a script reads.
    pub fn wall(&self) -> NaiveDateTime {
        self.wall
    }

    /// The UTC instant, for a zoned timestamp.
    pub fn instant(&self) -> Option<NaiveDateTime> {
        let zoned = self.zone.as_ref()?;
        self.wall.checked_sub_offset(zoned.offset)
    }

    /// The zone, or `None` for a wall clock alone.
    pub fn zone(&self) -> Option<&Zone> {
        self.zone.as_ref().map(|zoned| &zoned.zone)
    }

    pub fn date(&self) -> Date {
        Date(self.wall.date())
    }

    pub fn time(&self) -> Time {
        Time(self.wall.time())
    }

    fn utc_instant(&self, why: &str) -> Result<NaiveDateTime, Failure> {
        match (&self.zone, self.instant()) {
            (Some(_), Some(utc)) => Ok(utc),
            (Some(_), None) => Err(timestamp_out_of_range()),
            (None, _) => Err(format!(
                "{why}: this timestamp has no zone; say where its wall clock is with at_zone(…)"
            )),
        }
    }

    /// `ts.in_zone(zone)`: the same instant, read in `zone`.
    pub fn in_zone(&self, zone: &Zone) -> Result<Self, Failure> {
        zone.at_instant(self.utc_instant("in_zone")?)
    }

    /// `ts.at_zone(zone)`: this wall clock, in `zone`.
    pub fn at_zone(&self, zone: &Zone) -> Result<Self, Failure> {
        if let Some(zoned) = &self.zone {
            return Err(format!(
                "at_zone: this timestamp is already in {}; move it with in_zone(…)",
                zoned.zone
            ));
        }
        zone.resolve(self.wall)
    }

    /// `ts.without_zone()`: the wall clock alone.
    pub fn without_zone(&self) -> Self {
        Self::plain(self.wall)
    }

    /// Back into this timestamp's zone after moving its wall clock.
    fn rewall(&self, wall: NaiveDateTime) -> Result<Self, Failure> {
        match &self.zone {
            Some(zoned) => zoned.zone.resolve(wall),
            None => Ok(Self::plain(wall)),
        }
    }

    fn calendar(
        &self,
        move_day: impl FnOnce(Date) -> Result<Date, Failure>,
    ) -> Result<Self, Failure> {
        let day = move_day(self.date())?;
        self.rewall(day.0.and_time(self.wall.time()))
    }

    pub fn add_days(&self, n: i64) -> Result<Self, Failure> {
        self.calendar(|d| d.add_days(n))
    }
    pub fn add_weeks(&self, n: i64) -> Result<Self, Failure> {
        self.calendar(|d| d.add_weeks(n))
    }
    pub fn add_months(&self, n: i64) -> Result<Self, Failure> {
        self.calendar(|d| d.add_months(n))
    }
    pub fn add_years(&self, n: i64) -> Result<Self, Failure> {
        self.calendar(|d| d.add_years(n))
    }

    /// Exact elapsed time: hours, minutes, seconds.
    pub fn add(&self, n: i64, unit: fn(i64) -> Option<TimeDelta>) -> Result<Self, Failure> {
        self.shift(delta(n, unit)?)
    }

    fn shift(&self, by: TimeDelta) -> Result<Self, Failure> {
        match &self.zone {
            Some(zoned) => {
                let utc = self.instant().ok_or_else(timestamp_out_of_range)?;
                zoned.zone.at_instant(
                    utc.checked_add_signed(by)
                        .ok_or_else(timestamp_out_of_range)?,
                )
            }
            None => self
                .wall
                .checked_add_signed(by)
                .map(Self::plain)
                .ok_or_else(timestamp_out_of_range),
        }
    }

    /// The start of `period`, in this timestamp's zone.
    pub fn start_of(&self, period: Period) -> Result<Self, Failure> {
        let day = period_start(self.wall.date(), period).ok_or_else(timestamp_out_of_range)?;
        self.rewall(day.and_time(NaiveTime::MIN))
    }

    /// The last nanosecond before the next `period` starts, in this timestamp's zone.
    pub fn end_of(&self, period: Period) -> Result<Self, Failure> {
        let day = next_period_start(self.wall.date(), period).ok_or_else(timestamp_out_of_range)?;
        let next = self.rewall(day.and_time(NaiveTime::MIN))?;
        next.shift(TimeDelta::nanoseconds(-1))
    }

    /// How two timestamps can be compared: both zoned (as instants) or both plain.
    fn key(&self, other: &Self, done: &str) -> Result<(NaiveDateTime, NaiveDateTime), Failure> {
        match (self.zone.is_some(), other.zone.is_some()) {
            (true, true) => self
                .instant()
                .zip(other.instant())
                .ok_or_else(timestamp_out_of_range),
            (false, false) => Ok((self.wall, other.wall)),
            _ => Err(format!(
                "a timestamp with a zone and one without can't be {done}: place the plain one \
                 with at_zone(…), or drop the zone with without_zone()"
            )),
        }
    }

    pub fn compare(&self, other: &Self) -> Result<Ordering, Failure> {
        self.key(other, "compared").map(|(a, b)| a.cmp(&b))
    }

    /// `self - other`.
    pub fn since(&self, other: &Self) -> Result<Duration, Failure> {
        self.key(other, "subtracted").map(|(a, b)| Duration(a - b))
    }

    /// `a.months_until(b)`.
    pub fn months_until(&self, end: &Self) -> Result<i64, Failure> {
        self.key(end, "compared")?;
        let end_wall = match &self.zone {
            Some(zoned) => end.in_zone(&zoned.zone)?.wall,
            None => end.wall,
        };
        Ok(whole_months(self.wall, end_wall))
    }

    pub fn format(&self, with: &str) -> Result<String, Failure> {
        let kind = "timestamp";
        let (zoned, utc) = match (&self.zone, self.instant()) {
            (None, _) => {
                return write_pattern(with, kind, |items, out| {
                    write!(out, "{}", self.wall.format_with_items(items.iter()))
                });
            }
            (Some(_), None) => return Err(timestamp_out_of_range()),
            (Some(zoned), Some(utc)) => (zoned, utc),
        };
        let at = DateTime::<FixedOffset>::from_naive_utc_and_offset(utc, zoned.offset);
        let abbreviation = zoned.zone.abbreviation(utc, zoned.offset);
        write_pattern(with, kind, |items, out| {
            let named = items.iter().map(|item| match item {
                Item::Fixed(Fixed::TimezoneName) => {
                    Item::OwnedLiteral(abbreviation.as_str().into())
                }
                other => other.clone(),
            });
            write!(out, "{}", at.format_with_items(named))
        })
    }

    /// `debug(ts)`: the text, plus a named zone in brackets.
    pub fn debug(&self) -> String {
        match self.zone() {
            Some(zone @ Zone::Named(_)) => format!("{self}[{zone}]"),
            _ => self.to_string(),
        }
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.zone, self.instant()) {
            (None, _) => write!(f, "{}", self.wall.format("%Y-%m-%dT%H:%M:%S%.f")),
            (Some(zoned), Some(utc)) => f.write_str(
                &DateTime::<FixedOffset>::from_naive_utc_and_offset(utc, zoned.offset)
                    .to_rfc3339_opts(SecondsFormat::AutoSi, true),
            ),
            (Some(zoned), None) => write!(
                f,
                "{}{}",
                self.wall.format("%Y-%m-%dT%H:%M:%S%.f"),
                zoned.offset
            ),
        }
    }
}

// ── Duration ─────────────────────────────────────────────────────────────────

impl Duration {
    pub fn abs(self) -> Self {
        Self(self.0.abs())
    }
}

/// ISO 8601: `P30D`, `PT1H30M`, `-P1DT0.5S`; zero is `PT0S`.
impl fmt::Display for Duration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let length = self.0.abs();
        if self.0 < TimeDelta::zero() {
            f.write_str("-")?;
        }
        f.write_str("P")?;
        let days = length.num_days();
        if days != 0 {
            write!(f, "{days}D")?;
        }
        let rest = length - TimeDelta::days(days);
        if rest.is_zero() {
            return if days == 0 {
                f.write_str("T0S")
            } else {
                Ok(())
            };
        }
        f.write_str("T")?;
        let hours = rest.num_hours();
        let minutes = rest.num_minutes() % 60;
        let seconds = rest.num_seconds() % 60;
        let nanos = rest.subsec_nanos();
        if hours != 0 {
            write!(f, "{hours}H")?;
        }
        if minutes != 0 {
            write!(f, "{minutes}M")?;
        }
        if seconds != 0 || nanos != 0 {
            write!(f, "{seconds}")?;
            if nanos != 0 {
                let fraction = format!("{nanos:09}");
                write!(f, ".{}", fraction.trim_end_matches('0'))?;
            }
            f.write_str("S")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;

//! The date functions a script can call, registered into the sandbox's one module (a second
//! module would slow every call; see [`crate::sandbox`]).
//!
//! Parts are getters (`d.year`), everything else a function or method. Each operator a
//! script can write between two date values is registered, including the mismatches: Rhai
//! answers a comparison between two different types with a silent `false`, and
//! `this.createdAt > "2024-01-01"` worked on strings before dates were typed, so a mismatch
//! fails with what to write instead.
//!
//! ```
//! let engine = script::sandbox::engine();
//! let days: i64 = engine.eval("(date(2024, 3, 1) - date(2024, 1, 31)).days").unwrap();
//! assert_eq!(days, 30);
//! ```

use std::cmp::Ordering;

use chrono::TimeDelta;
use rhai::{Dynamic, EvalAltResult, ImmutableString, Module};

use super::{Date, Duration, Failure, Period, Time, Timestamp, Zone};

type Outcome<T> = Result<T, Box<EvalAltResult>>;

/// What Rhai accepts as a value: its `Variant`, which it doesn't export.
trait Value: Clone + Send + Sync + 'static {}
impl<T: Clone + Send + Sync + 'static> Value for T {}

fn lift<T>(result: Result<T, Failure>) -> Outcome<T> {
    result.map_err(Into::into)
}

/// An operator and what it says about an ordering.
type Comparison = (&'static str, fn(Ordering) -> bool);
/// A duration property and the total it reads.
type Total = (&'static str, fn(&TimeDelta) -> i64);
/// A clock-unit method and its unit.
type ClockUnit = (&'static str, fn(i64) -> Option<TimeDelta>);
/// Moving a value by `n` of a calendar unit.
type Move<T> = fn(T, i64) -> Result<T, Failure>;

const COMPARISONS: [Comparison; 6] = [
    ("==", Ordering::is_eq),
    ("!=", Ordering::is_ne),
    ("<", Ordering::is_lt),
    ("<=", Ordering::is_le),
    (">", Ordering::is_gt),
    (">=", Ordering::is_ge),
];

const PERIODS: [(&str, Period); 4] = [
    ("week", Period::Week),
    ("month", Period::Month),
    ("quarter", Period::Quarter),
    ("year", Period::Year),
];

/// Register every date type, property, function and operator into `module`.
pub fn register(module: &mut Module) {
    module.set_custom_type::<Date>("date");
    module.set_custom_type::<Time>("time");
    module.set_custom_type::<Timestamp>("timestamp");
    module.set_custom_type::<Duration>("duration");

    constructors(module);
    date_parts(module);
    time_parts(module);
    arithmetic(module);
    rounding(module);
    zones(module);
    text(module);
    comparisons(module);
    mismatches(module);
}

fn constructors(module: &mut Module) {
    module.set_native_fn("date", |y: i64, m: i64, d: i64| lift(Date::new(y, m, d)));
    module.set_native_fn("time", |h: i64, m: i64, s: i64| lift(Time::new(h, m, s)));
    module.set_native_fn(
        "timestamp",
        |y: i64, mo: i64, d: i64, h: i64, mi: i64, s: i64| lift(Timestamp::new(y, mo, d, h, mi, s)),
    );
    module.set_native_fn("parse_date", |s: ImmutableString| lift(Date::parse(&s)));
    module.set_native_fn("parse_date", |s: ImmutableString, p: ImmutableString| {
        lift(Date::parse_with(&s, &p))
    });
    module.set_native_fn("parse_time", |s: ImmutableString| lift(Time::parse(&s)));
    module.set_native_fn("parse_time", |s: ImmutableString, p: ImmutableString| {
        lift(Time::parse_with(&s, &p))
    });
    module.set_native_fn("parse_timestamp", |s: ImmutableString| {
        lift(Timestamp::parse(&s))
    });
    module.set_native_fn(
        "parse_timestamp",
        |s: ImmutableString, p: ImmutableString| lift(Timestamp::parse_with(&s, &p)),
    );
}

/// A part getter on a date and on a timestamp (read from its wall clock).
fn date_part(module: &mut Module, name: &str, part: fn(Date) -> i64) {
    module.set_getter_fn(name, move |d: &mut Date| Ok(part(*d)));
    module.set_getter_fn(name, move |t: &mut Timestamp| Ok(part(t.date())));
}

/// A part getter on a time and on a timestamp.
fn time_part(module: &mut Module, name: &str, part: fn(Time) -> i64) {
    module.set_getter_fn(name, move |t: &mut Time| Ok(part(*t)));
    module.set_getter_fn(name, move |t: &mut Timestamp| Ok(part(t.time())));
}

fn date_parts(module: &mut Module) {
    date_part(module, "year", Date::year);
    date_part(module, "month", Date::month);
    date_part(module, "day", Date::day);
    date_part(module, "weekday", Date::weekday);
    date_part(module, "day_of_year", Date::day_of_year);
    date_part(module, "iso_week", Date::iso_week);
    date_part(module, "iso_year", Date::iso_year);
    date_part(module, "quarter", Date::quarter);
}

fn time_parts(module: &mut Module) {
    time_part(module, "hour", Time::hour);
    time_part(module, "minute", Time::minute);
    time_part(module, "second", Time::second);
    time_part(module, "millisecond", Time::millisecond);
    time_part(module, "nanosecond", Time::nanosecond);
    module.set_getter_fn("zone", |t: &mut Timestamp| {
        Ok(t.zone()
            .map_or(Dynamic::UNIT, |zone| zone.to_string().into()))
    });

    let total: [Total; 6] = [
        ("weeks", TimeDelta::num_weeks),
        ("days", TimeDelta::num_days),
        ("hours", TimeDelta::num_hours),
        ("minutes", TimeDelta::num_minutes),
        ("seconds", TimeDelta::num_seconds),
        ("milliseconds", TimeDelta::num_milliseconds),
    ];
    for (name, unit) in total {
        module.set_getter_fn(name, move |d: &mut Duration| Ok(unit(&d.0)));
    }
}

fn arithmetic(module: &mut Module) {
    let calendar: [(&str, Move<Date>, Move<Timestamp>); 4] = [
        ("add_days", Date::add_days, Timestamp::add_days),
        ("add_weeks", Date::add_weeks, Timestamp::add_weeks),
        ("add_months", Date::add_months, Timestamp::add_months),
        ("add_years", Date::add_years, Timestamp::add_years),
    ];
    for (name, on_date, on_timestamp) in calendar {
        module.set_native_fn(name, move |d: Date, n: i64| lift(on_date(d, n)));
        module.set_native_fn(name, move |t: Timestamp, n: i64| lift(on_timestamp(t, n)));
    }
    let clock: [ClockUnit; 3] = [
        ("add_hours", TimeDelta::try_hours),
        ("add_minutes", TimeDelta::try_minutes),
        ("add_seconds", TimeDelta::try_seconds),
    ];
    for (name, unit) in clock {
        module.set_native_fn(name, move |t: Time, n: i64| lift(t.add(n, unit)));
        module.set_native_fn(name, move |t: Timestamp, n: i64| lift(t.add(n, unit)));
    }

    module.set_native_fn("-", |a: Date, b: Date| Ok(Duration(a.0 - b.0)));
    module.set_native_fn("-", |a: Time, b: Time| Ok(Duration(a.0 - b.0)));
    module.set_native_fn("-", |a: Timestamp, b: Timestamp| lift(a.since(b)));
    module.set_native_fn("abs", |d: Duration| Ok(d.abs()));

    module.set_native_fn("months_until", |a: Date, b: Date| lift(a.months_until(b)));
    module.set_native_fn("years_until", |a: Date, b: Date| {
        lift(a.months_until(b)).map(|months| months / 12)
    });
    module.set_native_fn("months_until", |a: Timestamp, b: Timestamp| {
        lift(a.months_until(b))
    });
    module.set_native_fn("years_until", |a: Timestamp, b: Timestamp| {
        lift(a.months_until(b)).map(|months| months / 12)
    });
}

fn rounding(module: &mut Module) {
    for (name, period) in PERIODS {
        module.set_native_fn(format!("start_of_{name}"), move |d: Date| {
            lift(d.start_of(period))
        });
        module.set_native_fn(format!("end_of_{name}"), move |d: Date| {
            lift(d.end_of(period))
        });
    }
    for (name, period) in PERIODS.into_iter().chain([("day", Period::Day)]) {
        module.set_native_fn(format!("start_of_{name}"), move |t: Timestamp| {
            lift(t.start_of(period))
        });
        module.set_native_fn(format!("end_of_{name}"), move |t: Timestamp| {
            lift(t.end_of(period))
        });
    }
}

fn zones(module: &mut Module) {
    module.set_native_fn("at_time", |d: Date, t: Time| Ok(d.at_time(t)));
    module.set_native_fn("date", |t: Timestamp| Ok(t.date()));
    module.set_native_fn("time", |t: Timestamp| Ok(t.time()));
    module.set_native_fn("in_zone", |t: Timestamp, zone: ImmutableString| {
        lift(Zone::parse(&zone).and_then(|zone| t.in_zone(zone)))
    });
    module.set_native_fn("at_zone", |t: Timestamp, zone: ImmutableString| {
        lift(Zone::parse(&zone).and_then(|zone| t.at_zone(zone)))
    });
    module.set_native_fn("without_zone", |t: Timestamp| Ok(t.without_zone()));
}

fn text(module: &mut Module) {
    fn shown<T: Value + std::fmt::Display>(module: &mut Module) {
        module.set_native_fn("to_string", |v: &mut T| Ok(v.to_string()));
        module.set_native_fn("to_debug", |v: &mut T| Ok(v.to_string()));
    }
    shown::<Date>(module);
    shown::<Time>(module);
    shown::<Timestamp>(module);
    shown::<Duration>(module);
    module.set_native_fn("to_debug", |t: &mut Timestamp| Ok(t.debug()));

    module.set_native_fn("format", |d: Date, p: ImmutableString| lift(d.format(&p)));
    module.set_native_fn("format", |t: Time, p: ImmutableString| lift(t.format(&p)));
    module.set_native_fn("format", |t: Timestamp, p: ImmutableString| {
        lift(t.format(&p))
    });
}

/// The six comparison operators between two `T`s, ordered by `compare`.
fn comparable<T: Value>(module: &mut Module, compare: fn(&T, &T) -> Result<Ordering, Failure>) {
    for (op, holds) in COMPARISONS {
        module.set_native_fn(op, move |a: T, b: T| lift(compare(&a, &b).map(holds)));
    }
}

fn comparisons(module: &mut Module) {
    comparable::<Date>(module, |a, b| Ok(a.cmp(b)));
    comparable::<Time>(module, |a, b| Ok(a.cmp(b)));
    comparable::<Duration>(module, |a, b| Ok(a.cmp(b)));
    comparable::<Timestamp>(module, |a, b| a.compare(*b));
}

/// The six comparison operators between an `A` and a `B`, each failing with `message`.
fn refused<A: Value, B: Value>(module: &mut Module, message: &'static str) {
    for (op, _) in COMPARISONS {
        module.set_native_fn(op, move |_: A, _: B| -> Outcome<bool> {
            Err(message.into())
        });
    }
}

fn mismatches(module: &mut Module) {
    const DATE_AND_TIMESTAMP: &str = "a date and a timestamp can't be compared: compare the \
        timestamp's day with ts.date(), or give the date a time with d.at_time(…)";
    refused::<Date, Timestamp>(module, DATE_AND_TIMESTAMP);
    refused::<Timestamp, Date>(module, DATE_AND_TIMESTAMP);

    const DATE_AND_STRING: &str =
        "a date and a string can't be compared: parse the string with parse_date(…)";
    refused::<Date, ImmutableString>(module, DATE_AND_STRING);
    refused::<ImmutableString, Date>(module, DATE_AND_STRING);

    const TIME_AND_STRING: &str =
        "a time and a string can't be compared: parse the string with parse_time(…)";
    refused::<Time, ImmutableString>(module, TIME_AND_STRING);
    refused::<ImmutableString, Time>(module, TIME_AND_STRING);

    const TIMESTAMP_AND_STRING: &str =
        "a timestamp and a string can't be compared: parse the string with parse_timestamp(…)";
    refused::<Timestamp, ImmutableString>(module, TIMESTAMP_AND_STRING);
    refused::<ImmutableString, Timestamp>(module, TIMESTAMP_AND_STRING);
}

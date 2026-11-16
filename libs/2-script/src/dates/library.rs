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

use std::any::TypeId;
use std::cmp::Ordering;

use chrono::TimeDelta;
use rhai::{Array, Dynamic, EvalAltResult, ImmutableString, Map, Module, Position};
use rust_decimal::Decimal;

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
/// Moving a timestamp by `n` of a calendar unit.
type MoveRef<T> = fn(&T, i64) -> Result<T, Failure>;

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
    ordering(module);
    nulls(module);
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
    let calendar: [(&str, Move<Date>, MoveRef<Timestamp>); 4] = [
        ("add_days", Date::add_days, Timestamp::add_days),
        ("add_weeks", Date::add_weeks, Timestamp::add_weeks),
        ("add_months", Date::add_months, Timestamp::add_months),
        ("add_years", Date::add_years, Timestamp::add_years),
    ];
    for (name, on_date, on_timestamp) in calendar {
        module.set_native_fn(name, move |d: Date, n: i64| lift(on_date(d, n)));
        module.set_native_fn(name, move |t: Timestamp, n: i64| lift(on_timestamp(&t, n)));
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
    module.set_native_fn("-", |a: Timestamp, b: Timestamp| lift(a.since(&b)));
    module.set_native_fn("abs", |d: Duration| Ok(d.abs()));

    module.set_native_fn("months_until", |a: Date, b: Date| Ok(a.months_until(b)));
    module.set_native_fn("years_until", |a: Date, b: Date| Ok(a.months_until(b) / 12));
    module.set_native_fn("months_until", |a: Timestamp, b: Timestamp| {
        lift(a.months_until(&b))
    });
    module.set_native_fn("years_until", |a: Timestamp, b: Timestamp| {
        lift(a.months_until(&b)).map(|months| months / 12)
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
        lift(Zone::parse(&zone).and_then(|zone| t.in_zone(&zone)))
    });
    module.set_native_fn("at_zone", |t: Timestamp, zone: ImmutableString| {
        lift(Zone::parse(&zone).and_then(|zone| t.at_zone(&zone)))
    });
    module.set_native_fn("without_zone", |t: Timestamp| Ok(t.without_zone()));
}

fn text(module: &mut Module) {
    fn shown<T: Value + std::fmt::Display>(module: &mut Module) {
        module.set_native_fn("to_string", |v: &mut T| Ok(v.to_string()));
        module.set_native_fn("to_debug", |v: &mut T| Ok(format!("{:?}", v.to_string())));
    }
    shown::<Date>(module);
    shown::<Time>(module);
    shown::<Timestamp>(module);
    shown::<Duration>(module);
    module.set_native_fn("to_debug", |t: &mut Timestamp| {
        Ok(format!("{:?}", t.debug()))
    });
    module.set_native_fn("to_json", |map: &mut Map| Ok(json(map)));

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
    comparable::<Timestamp>(module, |a, b| a.compare(b));
}

/// How a value is named in a message about comparing it.
fn kind_of(value: &Dynamic) -> &'static str {
    if value.is::<Date>() {
        "a date"
    } else if value.is::<Time>() {
        "a time"
    } else if value.is::<Timestamp>() {
        "a timestamp"
    } else if value.is::<Duration>() {
        "a duration"
    } else if value.is_string() {
        "a string"
    } else if value.is_int() || value.is_float() || value.is_decimal() {
        "a number"
    } else if value.is_unit() {
        "()"
    } else if value.is_array() {
        "an array"
    } else if value.is_map() {
        "an object"
    } else if value.is_blob() {
        "a blob"
    } else if value.is_bool() {
        "a bool"
    } else if value.is_char() {
        "a char"
    } else {
        "a value of another kind"
    }
}

/// What to write instead of comparing a date kind `kind` with `other`.
fn instead(kind: &str, other: &str) -> &'static str {
    match (kind, other) {
        (_, "()") => "it is null: test it with == (), or give it a value with ??",
        ("a duration", "a number") => "read a unit: (b - a).days > 30",
        ("a duration", "a string") => "read a unit, (b - a).days, or write it with .to_string()",
        ("a duration", _) | (_, "a duration") => {
            "a duration is a length of time: compare it with another duration, or read a unit, (b - a).days"
        }
        (_, "a number") => "read a part, like d.year, or compare with a value of the same kind",
        ("a date", "a string") => "parse the string with parse_date(…)",
        ("a time", "a string") => "parse the string with parse_time(…)",
        ("a timestamp", "a string") => "parse the string with parse_timestamp(…)",
        ("a date", "a timestamp") | ("a timestamp", "a date") => {
            "compare the timestamp's day, ts.date(), or give the date a time, d.at_time(…)"
        }
        ("a time", "a timestamp") | ("a timestamp", "a time") => {
            "compare the timestamp's time of day, ts.time()"
        }
        _ => "compare values of the same kind",
    }
}

/// The error for comparing `a` with `b`, one of them a date value.
fn not_comparable(a: &Dynamic, b: &Dynamic) -> Box<EvalAltResult> {
    let (a, b) = (kind_of(a), kind_of(b));
    let (kind, other) = if ["a date", "a time", "a timestamp", "a duration"].contains(&a) {
        (a, b)
    } else {
        (b, a)
    };
    format!("{a} and {b} can't be compared: {}", instead(kind, other)).into()
}

/// Every comparison between the date kind `K` and a value of any other kind fails with what to
/// write. One function per operator and side, on `Dynamic`: Rhai tries it only once the exact
/// signature (`K` with `K`) is missing, and caches what it found. `d == ()` and `d != ()` stay
/// the null test.
fn refuse_others<K: Value>(module: &mut Module) {
    for op in ["==", "!=", "<", "<=", ">", ">="] {
        let null_answer = match op {
            "==" => Some(false),
            "!=" => Some(true),
            _ => None,
        };
        let answer = move |a: Dynamic, b: Dynamic| -> Outcome<bool> {
            match null_answer {
                Some(answer) if a.is_unit() || b.is_unit() => Ok(answer),
                _ => Err(not_comparable(&a, &b)),
            }
        };
        module.set_native_fn(op, move |a: K, b: Dynamic| answer(Dynamic::from(a), b));
        module.set_native_fn(op, move |a: Dynamic, b: K| answer(a, Dynamic::from(b)));
    }
}

fn mismatches(module: &mut Module) {
    refuse_others::<Date>(module);
    refuse_others::<Time>(module);
    refuse_others::<Timestamp>(module);
    refuse_others::<Duration>(module);
}

/// `min`, `max`, and `sort` / `sort_desc` with no comparator, for arrays of one date kind.
fn ordering(module: &mut Module) {
    fn extremes<T: Value>(module: &mut Module, compare: fn(&T, &T) -> Result<Ordering, Failure>) {
        module.set_native_fn("min", move |a: T, b: T| {
            lift(compare(&a, &b)).map(|o| if o.is_le() { a } else { b })
        });
        module.set_native_fn("max", move |a: T, b: T| {
            lift(compare(&a, &b)).map(|o| if o.is_ge() { a } else { b })
        });
    }
    extremes::<Date>(module, |a, b| Ok(a.cmp(b)));
    extremes::<Time>(module, |a, b| Ok(a.cmp(b)));
    extremes::<Duration>(module, |a, b| Ok(a.cmp(b)));
    extremes::<Timestamp>(module, |a, b| a.compare(b));

    fn mixed<K: Value>(module: &mut Module) {
        for name in ["min", "max"] {
            module.set_native_fn(name, |a: K, b: Dynamic| -> Outcome<Dynamic> {
                Err(not_comparable(&Dynamic::from(a), &b))
            });
            module.set_native_fn(name, |a: Dynamic, b: K| -> Outcome<Dynamic> {
                Err(not_comparable(&a, &Dynamic::from(b)))
            });
        }
    }
    mixed::<Date>(module);
    mixed::<Time>(module);
    mixed::<Timestamp>(module);
    mixed::<Duration>(module);

    module.set_native_fn("sort", |array: &mut Array| sort(array, false));
    module.set_native_fn("sort_desc", |array: &mut Array| sort(array, true));
}

/// Rhai's `sort` / `sort_desc` with no comparator (same rules, same messages), plus arrays of
/// one date kind. Replacing it is the only way in: the standard one refuses a custom type.
fn sort(array: &mut Array, descending: bool) -> Outcome<()> {
    let Some(first) = array.first().filter(|_| array.len() > 1) else {
        return Ok(());
    };
    let type_id = first.type_id();
    if array.iter().any(|item| item.type_id() != type_id) {
        return Err(EvalAltResult::ErrorFunctionNotFound(
            "elements of different types cannot be sorted".into(),
            Position::NONE,
        )
        .into());
    }
    let order = |ordering: Ordering| {
        if descending {
            ordering.reverse()
        } else {
            ordering
        }
    };
    macro_rules! by {
        ($read:expr) => {{
            array.sort_by(|a, b| match ($read(a), $read(b)) {
                (Some(a), Some(b)) => order(a.cmp(&b)),
                _ => Ordering::Equal,
            });
            return Ok(());
        }};
    }
    if type_id == TypeId::of::<i64>() {
        by!(|v: &Dynamic| v.as_int().ok())
    }
    if type_id == TypeId::of::<char>() {
        by!(|v: &Dynamic| v.as_char().ok())
    }
    if type_id == TypeId::of::<f64>() {
        array.sort_by(|a, b| match (a.as_float(), b.as_float()) {
            (Ok(a), Ok(b)) => order(a.partial_cmp(&b).unwrap_or(Ordering::Equal)),
            _ => Ordering::Equal,
        });
        return Ok(());
    }
    if type_id == TypeId::of::<ImmutableString>() {
        by!(|v: &Dynamic| v.clone().into_immutable_string().ok())
    }
    if type_id == TypeId::of::<Decimal>() {
        by!(|v: &Dynamic| v.as_decimal().ok())
    }
    if type_id == TypeId::of::<bool>() {
        by!(|v: &Dynamic| v.as_bool().ok())
    }
    if type_id == TypeId::of::<()>() {
        return Ok(());
    }
    if type_id == TypeId::of::<Date>() {
        by!(|v: &Dynamic| v.clone().try_cast::<Date>())
    }
    if type_id == TypeId::of::<Time>() {
        by!(|v: &Dynamic| v.clone().try_cast::<Time>())
    }
    if type_id == TypeId::of::<Duration>() {
        by!(|v: &Dynamic| v.clone().try_cast::<Duration>())
    }
    if type_id == TypeId::of::<Timestamp>() {
        let stamps: Vec<Timestamp> = array.iter().filter_map(|v| v.clone().try_cast()).collect();
        if let Some(first) = stamps.first() {
            for stamp in &stamps {
                lift(first.compare(stamp))?;
            }
        }
        array.sort_by(|a, b| {
            match (
                a.clone().try_cast::<Timestamp>(),
                b.clone().try_cast::<Timestamp>(),
            ) {
                (Some(a), Some(b)) => order(a.compare(&b).unwrap_or(Ordering::Equal)),
                _ => Ordering::Equal,
            }
        });
        return Ok(());
    }
    Err(EvalAltResult::ErrorFunctionNotFound(
        format!("elements of {} cannot be sorted", first.type_name()),
        Position::NONE,
    )
    .into())
}

/// Arithmetic on a null date fails with how to guard it, not Rhai's bare "function not found".
fn nulls(module: &mut Module) {
    fn null_operand<T: Value>(module: &mut Module, kind: &str) {
        let message = format!(
            "{kind} and () can't be subtracted: a null field is (); guard it with ?. or ??, \
             like (this.end ?? this.start) - this.start"
        );
        let other = message.clone();
        module.set_native_fn("-", move |_: T, _: ()| -> Outcome<Duration> {
            Err(message.clone().into())
        });
        module.set_native_fn("-", move |_: (), _: T| -> Outcome<Duration> {
            Err(other.clone().into())
        });
    }
    null_operand::<Date>(module, "a date");
    null_operand::<Time>(module, "a time");
    null_operand::<Timestamp>(module, "a timestamp");
}

/// A map's `to_json()`, with each date value written as its text (what `to_string()` gives,
/// and what 0.17 handed scripts), so the result is valid JSON.
fn json(map: &Map) -> String {
    if map.values().any(holds_a_date) {
        rhai::format_map_as_json(&dates_as_text(map))
    } else {
        rhai::format_map_as_json(map)
    }
}

fn is_date(value: &Dynamic) -> bool {
    value.is::<Date>() || value.is::<Time>() || value.is::<Timestamp>() || value.is::<Duration>()
}

fn holds_a_date(value: &Dynamic) -> bool {
    if is_date(value) {
        return true;
    }
    if let Some(map) = value.read_lock::<Map>() {
        return map.values().any(holds_a_date);
    }
    value
        .read_lock::<Array>()
        .is_some_and(|items| items.iter().any(holds_a_date))
}

fn dates_as_text(map: &Map) -> Map {
    map.iter()
        .map(|(key, value)| (key.clone(), date_as_text(value)))
        .collect()
}

fn date_as_text(value: &Dynamic) -> Dynamic {
    if let Some(text) = date_text(value) {
        return text.into();
    }
    if let Some(map) = value.read_lock::<Map>() {
        return Dynamic::from_map(dates_as_text(&map));
    }
    if let Some(items) = value.read_lock::<Array>() {
        return Dynamic::from_array(items.iter().map(date_as_text).collect());
    }
    value.clone()
}

fn date_text(value: &Dynamic) -> Option<String> {
    let value = value.clone();
    let value = match value.try_cast_result::<Date>() {
        Ok(d) => return Some(d.to_string()),
        Err(value) => value,
    };
    let value = match value.try_cast_result::<Time>() {
        Ok(t) => return Some(t.to_string()),
        Err(value) => value,
    };
    let value = match value.try_cast_result::<Timestamp>() {
        Ok(t) => return Some(t.to_string()),
        Err(value) => value,
    };
    value.try_cast::<Duration>().map(|d| d.to_string())
}

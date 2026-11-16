//! Typed dates in scripts (issue #364): every property and function, the edge rules the
//! manual states (month-end, leap days, daylight-saving gaps and overlaps, comparison across
//! kinds, out of range, nulls, strings), and how a date result is checked against its field.
//! Each case runs a real script through [`Scripts`], so it covers registration, conversion in
//! and conversion out together.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    unused_crate_dependencies
)]

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use config::yaml::SchemaYaml;
use kernel::{GenericValue, IndexName, IndexSchema, ParseFrom};
use script::Scripts;
use source::SourceSpec;
use source::document::BuildFailure;

fn index() -> IndexName {
    IndexName::try_new("events").unwrap()
}

fn compile(fields: &str) -> Scripts {
    let yaml = format!("version: 1\ntable: events\nprimary_key: id\nfields:\n{fields}");
    let schema = IndexSchema::try_from(SchemaYaml::try_parse(&yaml).unwrap()).unwrap();
    Scripts::compile(&SourceSpec::new(BTreeMap::from([(index(), schema)])))
        .unwrap_or_else(|errors| panic!("{errors}"))
}

fn date(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

fn naive(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> NaiveDateTime {
    date(y, mo, d).and_hms_opt(h, mi, s).unwrap()
}

fn utc(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> DateTime<Utc> {
    Utc.from_utc_datetime(&naive(y, mo, d, h, mi, s))
}

const INPUTS: &str = "  \
  - date: start\n    required: true\n  \
  - date: end\n    required: true\n  \
  - date: none\n    required: false\n  \
  - timestamp: at\n    required: true\n  \
  - timestamp: local\n    required: true\n  \
  - timestamp: clock\n    required: true\n";

/// The document every [`eval`] runs over.
fn inputs() -> BTreeMap<String, GenericValue> {
    BTreeMap::from([
        ("start".to_owned(), GenericValue::Date(date(2024, 1, 31))),
        ("end".to_owned(), GenericValue::Date(date(2024, 3, 1))),
        ("none".to_owned(), GenericValue::Null),
        (
            "at".to_owned(),
            GenericValue::TimestampTz(utc(2024, 3, 31, 0, 30, 0)),
        ),
        (
            "local".to_owned(),
            GenericValue::Timestamp(naive(2024, 5, 1, 10, 0, 0)),
        ),
        (
            "clock".to_owned(),
            GenericValue::Time(NaiveTime::from_hms_milli_opt(10, 15, 30, 250).unwrap()),
        ),
    ])
}

/// Run `script` as a `ty` field over [`inputs`]: its value, or the failure.
fn run(ty: &str, script: &str) -> Result<GenericValue, BuildFailure> {
    let body = script.replace('\n', "\n      ");
    let scripts = compile(&format!(
        "{INPUTS}  - {ty}: out\n    required: false\n    script: |\n      {body}\n"
    ));
    let mut doc = inputs();
    doc.insert("out".to_owned(), GenericValue::Null);
    let mut doc = GenericValue::Map(doc);
    scripts.apply(&index(), &mut doc)?;
    let GenericValue::Map(mut map) = doc else {
        unreachable!()
    };
    Ok(map.remove("out").unwrap())
}

fn ok(ty: &str, script: &str) -> GenericValue {
    run(ty, script).unwrap_or_else(|failure| panic!("`{script}`: {failure}"))
}

/// A script's result as text, through `to_string()` where it isn't already a string.
fn text(script: &str) -> String {
    match ok("keyword", script) {
        GenericValue::String(s) => s,
        other => panic!("`{script}` gave {other:?}"),
    }
}

fn int(script: &str) -> i64 {
    match ok("long", script) {
        GenericValue::BigInt(i) => i,
        other => panic!("`{script}` gave {other:?}"),
    }
}

fn boolean(script: &str) -> bool {
    match ok("boolean", script) {
        GenericValue::Bool(b) => b,
        other => panic!("`{script}` gave {other:?}"),
    }
}

/// The failure message of a script that must fail.
fn fails(ty: &str, script: &str) -> String {
    match run(ty, script) {
        Ok(value) => panic!("`{script}` should fail, gave {value:?}"),
        Err(failure) => {
            assert!(
                !failure.message.contains("script::"),
                "`{script}` leaks a Rust type name: {}",
                failure.message
            );
            failure.message
        }
    }
}

fn assert_contains(message: &str, part: &str) {
    assert!(
        message.contains(part),
        "`{message}` should mention `{part}`"
    );
}

// ── the owner's example ───────────────────────────────────────────────────────

#[test]
fn days_active_is_the_difference_in_days() {
    let scripts = compile(
        "  \
  - date: start_date\n    required: true\n  \
  - date: end_date\n    required: true\n  \
  - integer: days_active\n    required: true\n    script: (this.end_date - this.start_date).days\n",
    );
    let mut doc = GenericValue::Map(BTreeMap::from([
        (
            "start_date".to_owned(),
            GenericValue::Date(date(2024, 1, 15)),
        ),
        ("end_date".to_owned(), GenericValue::Date(date(2024, 3, 15))),
        ("days_active".to_owned(), GenericValue::Null),
    ]));
    scripts.apply(&index(), &mut doc).unwrap();
    let GenericValue::Map(map) = doc else {
        unreachable!()
    };
    assert_eq!(map["days_active"], GenericValue::Int(60));
}

// ── types and text ────────────────────────────────────────────────────────────

#[test]
fn each_kind_arrives_as_its_own_type() {
    assert_eq!(text("type_of(this.start)"), "date");
    assert_eq!(text("type_of(this.at)"), "timestamp");
    assert_eq!(text("type_of(this.local)"), "timestamp");
    assert_eq!(text("type_of(this.clock)"), "time");
    assert_eq!(text("type_of(this.end - this.start)"), "duration");
    assert_eq!(text("type_of(this.none)"), "()");
}

#[test]
fn to_string_and_interpolation_write_what_scripts_read_before() {
    assert_eq!(text("this.start.to_string()"), "2024-01-31");
    assert_eq!(text("`${this.at}`"), "2024-03-31T00:30:00Z");
    assert_eq!(text("`${this.local}`"), "2024-05-01T10:00:00");
    assert_eq!(text("`${this.clock}`"), "10:15:30.250");
    assert_eq!(text(r#""on " + this.start"#), "on 2024-01-31");
    assert_eq!(
        text(r#"this.at.in_zone("Europe/Rome").to_string()"#),
        "2024-03-31T01:30:00+01:00"
    );
}

#[test]
fn debug_names_the_zone() {
    let printed = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let sink = std::sync::Arc::clone(&printed);
    let yaml = format!(
        "version: 1\ntable: events\nprimary_key: id\nfields:\n{INPUTS}  \
         - keyword: out\n    required: false\n    script: |\n      \
         debug(this.at.in_zone(\"Europe/Rome\")); ()\n"
    );
    let schema = IndexSchema::try_from(SchemaYaml::try_parse(&yaml).unwrap()).unwrap();
    let scripts = Scripts::compile_with_printer(
        &SourceSpec::new(BTreeMap::from([(index(), schema)])),
        Some(std::sync::Arc::new(
            move |_: &script::ScriptContext<'_>, line: &str| {
                sink.lock().unwrap().push(line.to_owned());
            },
        )),
    )
    .unwrap();
    let mut doc = GenericValue::Map(inputs());
    scripts.apply(&index(), &mut doc).unwrap();
    assert_eq!(
        printed.lock().unwrap().as_slice(),
        [r#""2024-03-31T01:30:00+01:00[Europe/Rome]""#]
    );
}

// ── parts ─────────────────────────────────────────────────────────────────────

#[test]
fn a_date_has_its_calendar_parts() {
    assert_eq!(int("this.start.year"), 2024);
    assert_eq!(int("this.start.month"), 1);
    assert_eq!(int("this.start.day"), 31);
    assert_eq!(int("this.start.weekday"), 3, "a Wednesday, Monday is 1");
    assert_eq!(int("date(2024, 5, 5).weekday"), 7, "Sunday is 7");
    assert_eq!(int("date(2024, 12, 31).day_of_year"), 366);
    assert_eq!(int("date(2023, 12, 31).day_of_year"), 365);
    assert_eq!(int("date(2021, 1, 1).iso_week"), 53);
    assert_eq!(int("date(2021, 1, 1).iso_year"), 2020);
    assert_eq!(int("date(2024, 12, 30).iso_year"), 2025);
    assert_eq!(int("this.start.quarter"), 1);
    assert_eq!(int("date(2024, 5, 1).quarter"), 2);
    assert_eq!(int("date(2024, 9, 30).quarter"), 3);
    assert_eq!(int("date(2024, 10, 1).quarter"), 4);
}

#[test]
fn a_time_has_its_clock_parts() {
    assert_eq!(int("this.clock.hour"), 10);
    assert_eq!(int("this.clock.minute"), 15);
    assert_eq!(int("this.clock.second"), 30);
    assert_eq!(int("this.clock.millisecond"), 250);
    assert_eq!(int("this.clock.nanosecond"), 250_000_000);
}

#[test]
fn a_timestamp_has_every_part_taken_in_its_zone() {
    assert_eq!(int("this.local.year"), 2024);
    assert_eq!(int("this.local.hour"), 10);
    assert_eq!(int("this.at.day"), 31);
    assert_eq!(int("this.at.hour"), 0);
    assert_eq!(int("this.at.minute"), 30);
    assert_eq!(int(r#"this.at.in_zone("Europe/Rome").hour"#), 1);
    assert_eq!(int("this.at.weekday"), 7);
    assert_eq!(int("this.at.quarter"), 1);
    assert_eq!(int("this.at.day_of_year"), 91);
    assert_eq!(int("this.at.iso_week"), 13);
    assert_eq!(int("this.at.second"), 0);
    assert_eq!(int("this.at.millisecond"), 0);
    assert_eq!(text("this.at.zone"), "UTC");
    assert_eq!(ok("keyword", "this.local.zone"), GenericValue::Null);
}

#[test]
fn a_zone_that_changes_the_calendar_day_moves_every_date_part() {
    let tokyo = r#"parse_timestamp("2024-03-31T23:30:00Z").in_zone("Asia/Tokyo")"#;
    assert_eq!(text(&format!("{tokyo}.date().to_string()")), "2024-04-01");
    assert_eq!(int(&format!("{tokyo}.month")), 4);
    assert_eq!(int(&format!("{tokyo}.quarter")), 2);
    assert_eq!(int(&format!("{tokyo}.hour")), 8);
    let la = r#"parse_timestamp("2024-01-01T03:00:00Z").in_zone("America/Los_Angeles")"#;
    assert_eq!(int(&format!("{la}.year")), 2023);
    assert_eq!(int(&format!("{la}.day")), 31);
}

// ── construction and parsing ──────────────────────────────────────────────────

#[test]
fn constructors_build_and_refuse_what_does_not_exist() {
    assert_eq!(text("date(2024, 2, 29).to_string()"), "2024-02-29");
    assert_eq!(text("time(23, 59, 59).to_string()"), "23:59:59");
    assert_eq!(
        text("timestamp(2024, 5, 1, 9, 30, 0).to_string()"),
        "2024-05-01T09:30:00"
    );
    assert_contains(&fails("keyword", "date(2023, 2, 29)"), "2023-02-29");
    assert_contains(&fails("keyword", "date(2024, 13, 1)"), "no such date");
    assert_contains(&fails("keyword", "time(24, 0, 0)"), "no such time");
    assert_contains(
        &fails("keyword", "timestamp(2024, 4, 31, 0, 0, 0)"),
        "no such",
    );
}

#[test]
fn parsing_reads_iso_text_and_patterns() {
    assert_eq!(
        text(r#"parse_date("2024-05-01").to_string()"#),
        "2024-05-01"
    );
    assert_eq!(
        text(r#"parse_date("01/05/2024", "%d/%m/%Y").to_string()"#),
        "2024-05-01"
    );
    assert_eq!(text(r#"parse_time("09:30").to_string()"#), "09:30:00");
    assert_eq!(
        text(r#"parse_time("9:30 PM", "%I:%M %p").to_string()"#),
        "21:30:00"
    );
    assert_eq!(int(r#"parse_time("10:00:00.250").millisecond"#), 250);
    let offset = r#"parse_timestamp("2024-05-01T23:00:00+02:00")"#;
    assert_eq!(
        int(&format!("{offset}.day")),
        1,
        "parts at the written offset"
    );
    assert_eq!(text(&format!("{offset}.zone")), "+02:00");
    assert_eq!(
        text(&format!("{offset}.to_string()")),
        "2024-05-01T23:00:00+02:00"
    );
    assert_eq!(
        ok("keyword", r#"parse_timestamp("2024-05-01T23:00:00").zone"#),
        GenericValue::Null,
        "no offset, no zone"
    );
    assert_eq!(
        text(r#"parse_timestamp("2024-05-01 23:00:00").to_string()"#),
        "2024-05-01T23:00:00"
    );
    assert_eq!(
        text(r#"parse_timestamp("01.05.2024 23:00", "%d.%m.%Y %H:%M").to_string()"#),
        "2024-05-01T23:00:00"
    );
    assert_eq!(
        text(r#"parse_timestamp("01.05.2024 23:00 +0200", "%d.%m.%Y %H:%M %z").zone"#),
        "+02:00"
    );
}

#[test]
fn text_that_does_not_parse_fails_with_the_text_and_the_pattern() {
    let message = fails("keyword", r#"parse_date("2024-5-1x")"#);
    assert_contains(&message, "2024-5-1x");
    let message = fails("keyword", r#"parse_date("May 1", "%d/%m/%Y")"#);
    assert_contains(&message, "May 1");
    assert_contains(&message, "%d/%m/%Y");
    assert_contains(&fails("keyword", r#"parse_time("25:00")"#), "25:00");
    assert_contains(
        &fails("keyword", r#"parse_timestamp("yesterday")"#),
        "yesterday",
    );
}

// ── formatting ────────────────────────────────────────────────────────────────

#[test]
fn format_writes_a_pattern() {
    assert_eq!(
        text(r#"date(2024, 5, 1).format("%d %b %Y")"#),
        "01 May 2024"
    );
    assert_eq!(text(r#"this.start.format("%Y-%m")"#), "2024-01");
    assert_eq!(text(r#"this.clock.format("%H.%M")"#), "10.15");
    assert_eq!(
        text(r#"this.at.in_zone("Europe/Rome").format("%H:%M %Z")"#),
        "01:30 CET"
    );
    assert_eq!(text(r#"this.local.format("%A %H:%M")"#), "Wednesday 10:00");
}

#[test]
fn a_bad_pattern_fails_instead_of_panicking() {
    assert_contains(&fails("keyword", r#"this.start.format("%Q")"#), "%Q");
    assert_contains(
        &fails("keyword", r#"this.start.format("%H:%M")"#),
        "pattern",
    );
    assert_contains(&fails("keyword", r#"this.clock.format("%Y")"#), "pattern");
    assert_contains(&fails("keyword", r#"this.local.format("%z")"#), "pattern");
    assert_contains(&fails("keyword", r#"parse_date("2024", "%Q")"#), "%Q");
}

// ── arithmetic ────────────────────────────────────────────────────────────────

#[test]
fn adding_calendar_units_clamps_to_the_month_end() {
    assert_eq!(
        text("date(2024, 1, 31).add_months(1).to_string()"),
        "2024-02-29"
    );
    assert_eq!(
        text("date(2023, 1, 31).add_months(1).to_string()"),
        "2023-02-28"
    );
    assert_eq!(
        text("date(2024, 3, 31).add_months(-1).to_string()"),
        "2024-02-29"
    );
    assert_eq!(
        text("date(2024, 2, 29).add_years(1).to_string()"),
        "2025-02-28"
    );
    assert_eq!(
        text("date(2024, 2, 29).add_years(4).to_string()"),
        "2028-02-29"
    );
    assert_eq!(
        text("date(2024, 12, 31).add_days(1).to_string()"),
        "2025-01-01"
    );
    assert_eq!(
        text("date(2024, 3, 1).add_days(-1).to_string()"),
        "2024-02-29"
    );
    assert_eq!(
        text("date(2023, 3, 1).add_days(-1).to_string()"),
        "2023-02-28"
    );
    assert_eq!(
        text("date(2024, 1, 1).add_weeks(2).to_string()"),
        "2024-01-15"
    );
    assert_eq!(
        text("this.local.add_months(10).to_string()"),
        "2025-03-01T10:00:00"
    );
}

#[test]
fn leap_years_follow_the_gregorian_calendar() {
    assert_eq!(
        text("date(2000, 2, 28).add_days(1).to_string()"),
        "2000-02-29"
    );
    assert_eq!(
        text("date(1900, 2, 28).add_days(1).to_string()"),
        "1900-03-01"
    );
    assert_contains(&fails("keyword", "date(2100, 2, 29)"), "2100-02-29");
}

#[test]
fn adding_clock_units() {
    assert_eq!(
        text("this.local.add_hours(15).to_string()"),
        "2024-05-02T01:00:00"
    );
    assert_eq!(
        text("this.local.add_minutes(-61).to_string()"),
        "2024-05-01T08:59:00"
    );
    assert_eq!(
        text("this.local.add_seconds(3600 * 24).to_string()"),
        "2024-05-02T10:00:00"
    );
    assert_eq!(
        text("time(23, 0, 0).add_hours(2).to_string()"),
        "01:00:00",
        "wraps"
    );
    assert_eq!(
        text("time(0, 0, 30).add_seconds(-60).to_string()"),
        "23:59:30"
    );
    assert_eq!(
        text("time(1, 0, 0).add_minutes(30).to_string()"),
        "01:30:00"
    );
}

#[test]
fn a_difference_is_a_duration_read_in_any_unit() {
    assert_eq!(int("(this.end - this.start).days"), 30);
    assert_eq!(int("(this.start - this.end).days"), -30, "negative");
    assert_eq!(int("(this.end - this.start).weeks"), 4);
    assert_eq!(int("(this.end - this.start).hours"), 720);
    assert_eq!(int("(time(10, 0, 0) - time(9, 30, 0)).minutes"), 30);
    assert_eq!(int("(time(9, 0, 0) - time(10, 0, 0)).seconds"), -3600);
    assert_eq!(
        int("(timestamp(2024, 5, 2, 12, 0, 0) - this.local).hours"),
        26
    );
    assert_eq!(
        int("(timestamp(2024, 5, 1, 10, 0, 1) - this.local).milliseconds"),
        1000
    );
    assert_eq!(
        int("(timestamp(2024, 4, 30, 0, 0, 0) - this.local).days"),
        -1,
        "toward zero: -34 hours is -1 day"
    );
    assert_eq!(int("(this.start - this.end).abs().days"), 30);
}

#[test]
fn a_difference_across_a_daylight_saving_change_is_elapsed_time() {
    let midnight =
        |day: u32| format!(r#"timestamp(2024, 3, {day}, 0, 0, 0).at_zone("Europe/Rome")"#);
    let (a, b) = (
        midnight(31),
        "timestamp(2024, 4, 1, 0, 0, 0).at_zone(\"Europe/Rome\")",
    );
    assert_eq!(int(&format!("({b} - {a}).hours")), 23);
    assert_eq!(int(&format!("({b} - {a}).days")), 0);
    assert_eq!(int(&format!("({b}.date() - {a}.date()).days")), 1);
    assert_eq!(
        int(&format!("({} - {}).hours", midnight(31), midnight(30))),
        24
    );
    let utc = r#"parse_timestamp("2024-03-31T12:00:00Z")"#;
    let rome = r#"parse_timestamp("2024-03-31T12:00:00+02:00")"#;
    assert_eq!(
        int(&format!("({utc} - {rome}).hours")),
        2,
        "zones may differ"
    );
}

#[test]
fn a_month_counts_once_the_end_reaches_the_starts_day_of_month() {
    assert_eq!(int("date(2024, 1, 31).months_until(date(2024, 2, 29))"), 0);
    assert_eq!(int("date(2024, 1, 31).months_until(date(2024, 3, 1))"), 1);
    assert_eq!(int("date(2023, 1, 31).months_until(date(2023, 2, 28))"), 0);
    assert_eq!(int("date(2023, 2, 28).months_until(date(2023, 1, 31))"), 0);
    assert_eq!(int("date(2024, 1, 15).months_until(date(2024, 3, 14))"), 1);
    assert_eq!(int("date(2024, 1, 15).months_until(date(2024, 3, 15))"), 2);
    assert_eq!(int("date(2024, 3, 31).months_until(date(2024, 2, 29))"), -1);
    assert_eq!(int("date(2024, 3, 15).months_until(date(2024, 1, 16))"), -1);
    assert_eq!(int("date(2024, 3, 15).months_until(date(2024, 1, 15))"), -2);
    assert_eq!(
        int("date(2000, 2, 29).years_until(date(2001, 2, 28))"),
        0,
        "a 29 February birthday is a year old on 1 March"
    );
    assert_eq!(int("date(2000, 2, 29).years_until(date(2001, 3, 1))"), 1);
    assert_eq!(int("date(2000, 6, 1).years_until(date(2024, 5, 31))"), 23);
    assert_eq!(int("date(2024, 5, 31).years_until(date(2000, 6, 1))"), -23);
    assert_eq!(
        int("timestamp(2024, 1, 1, 12, 0, 0).months_until(timestamp(2024, 2, 1, 11, 0, 0))"),
        0,
        "a timestamp counts the time too"
    );
    assert_eq!(
        int("timestamp(2024, 1, 1, 12, 0, 0).months_until(timestamp(2024, 2, 1, 12, 0, 0))"),
        1
    );
}

/// `months_until` agrees with Postgres's `age()` (years × 12 + months) on every pair Postgres 16
/// answered in `fixtures/months_from_postgres.tsv`: random dates, month ends, timestamps.
#[test]
fn whole_months_agree_with_postgres_age() {
    let fixture = include_str!("fixtures/months_from_postgres.tsv");
    let mut cases = 0;
    for line in fixture.lines() {
        let [kind, start, end, months] = line.split('\t').collect::<Vec<_>>()[..] else {
            panic!("bad fixture line `{line}`");
        };
        let parse = if kind == "date" {
            "parse_date"
        } else {
            "parse_timestamp"
        };
        let script = format!(r#"{parse}("{start}").months_until({parse}("{end}"))"#);
        assert_eq!(
            int(&script),
            months.parse::<i64>().unwrap(),
            "{start} → {end}"
        );
        cases += 1;
    }
    assert_eq!(cases, 408);
}

#[test]
fn calendar_units_keep_the_wall_clock_and_clock_units_are_exact() {
    let before = r#"timestamp(2024, 3, 30, 12, 0, 0).at_zone("Europe/Rome")"#;
    assert_eq!(
        text(&format!("{before}.add_days(1).to_string()")),
        "2024-03-31T12:00:00+02:00"
    );
    assert_eq!(
        text(&format!("{before}.add_hours(24).to_string()")),
        "2024-03-31T13:00:00+02:00"
    );
    assert_eq!(int(&format!("({before}.add_days(1) - {before}).hours")), 23);
}

// ── rounding ──────────────────────────────────────────────────────────────────

#[test]
fn a_date_rounds_to_the_start_and_end_of_its_periods() {
    let d = "date(2024, 5, 15)";
    for (call, want) in [
        ("start_of_week", "2024-05-13"),
        ("end_of_week", "2024-05-19"),
        ("start_of_month", "2024-05-01"),
        ("end_of_month", "2024-05-31"),
        ("start_of_quarter", "2024-04-01"),
        ("end_of_quarter", "2024-06-30"),
        ("start_of_year", "2024-01-01"),
        ("end_of_year", "2024-12-31"),
    ] {
        assert_eq!(text(&format!("{d}.{call}().to_string()")), want, "{call}");
    }
    assert_eq!(
        text("date(2024, 2, 10).end_of_month().to_string()"),
        "2024-02-29"
    );
}

#[test]
fn a_timestamp_rounds_in_its_zone() {
    assert_eq!(
        text("this.local.start_of_day().to_string()"),
        "2024-05-01T00:00:00"
    );
    assert_eq!(
        text("this.local.end_of_day().to_string()"),
        "2024-05-01T23:59:59.999999999"
    );
    assert_eq!(
        text("this.local.end_of_month().to_string()"),
        "2024-05-31T23:59:59.999999999"
    );
    let rome = r#"this.at.in_zone("Europe/Rome")"#;
    assert_eq!(
        text(&format!("{rome}.start_of_day().to_string()")),
        "2024-03-31T00:00:00+01:00"
    );
    assert_eq!(
        text(&format!("{rome}.end_of_day().to_string()")),
        "2024-03-31T23:59:59.999999999+02:00",
        "a 23-hour day"
    );
    assert_eq!(
        text(&format!("{rome}.start_of_quarter().to_string()")),
        "2024-01-01T00:00:00+01:00"
    );
    assert_eq!(
        text(&format!("{rome}.start_of_week().to_string()")),
        "2024-03-25T00:00:00+01:00"
    );
    assert_eq!(
        text("this.at.end_of_year().to_string()"),
        "2024-12-31T23:59:59.999999999Z"
    );
}

#[test]
fn the_start_of_a_day_that_begins_in_a_gap_is_the_first_instant_after_it() {
    // Santiago springs forward at midnight: 2024-09-08 00:00 does not exist.
    assert_eq!(
        text(
            r#"parse_timestamp("2024-09-08T12:00:00Z").in_zone("America/Santiago").start_of_day().to_string()"#
        ),
        "2024-09-08T01:00:00-03:00"
    );
}

// ── time zones ────────────────────────────────────────────────────────────────

#[test]
fn in_zone_keeps_the_instant_and_at_zone_keeps_the_wall_clock() {
    assert_eq!(
        text(r#"this.local.at_zone("Europe/Rome").to_string()"#),
        "2024-05-01T10:00:00+02:00"
    );
    assert_eq!(
        text(r#"this.local.at_zone("Europe/Rome").in_zone("UTC").to_string()"#),
        "2024-05-01T08:00:00Z"
    );
    assert_eq!(
        text(r#"this.at.in_zone("+05:30").to_string()"#),
        "2024-03-31T06:00:00+05:30"
    );
    assert_eq!(
        text(r#"this.at.in_zone("Europe/Rome").without_zone().to_string()"#),
        "2024-03-31T01:30:00"
    );
    assert_eq!(
        text(r#"this.at.in_zone("Europe/Rome").zone"#),
        "Europe/Rome"
    );
    assert_eq!(
        text(r#"this.at.in_zone("Europe/Rome").time().to_string()"#),
        "01:30:00"
    );
}

#[test]
fn a_wall_clock_in_a_gap_moves_forward_and_in_an_overlap_takes_the_earlier() {
    assert_eq!(
        text(r#"timestamp(2024, 3, 31, 2, 30, 0).at_zone("Europe/Rome").to_string()"#),
        "2024-03-31T03:30:00+02:00"
    );
    assert_eq!(
        text(r#"timestamp(2024, 10, 27, 2, 30, 0).at_zone("Europe/Rome").to_string()"#),
        "2024-10-27T02:30:00+02:00"
    );
    assert_eq!(
        text(r#"timestamp(2024, 3, 30, 2, 30, 0).at_zone("Europe/Rome").add_days(1).to_string()"#),
        "2024-03-31T03:30:00+02:00",
        "a calendar add into a gap"
    );
}

#[test]
fn a_zone_must_exist_and_fit_the_timestamp() {
    assert_contains(
        &fails("keyword", r#"this.at.in_zone("Europe/Roma")"#),
        "unknown time zone 'Europe/Roma'",
    );
    assert_contains(
        &fails("keyword", r#"this.local.in_zone("Europe/Rome")"#),
        "at_zone",
    );
    assert_contains(
        &fails("keyword", r#"this.at.at_zone("Europe/Rome")"#),
        "in_zone",
    );
}

// ── comparison ────────────────────────────────────────────────────────────────

#[test]
fn values_of_one_kind_compare() {
    assert!(boolean("this.end > this.start"));
    assert!(boolean("this.start <= this.start"));
    assert!(boolean("this.start != this.end"));
    assert!(boolean("time(9, 0, 0) < this.clock"));
    assert!(boolean("this.local == timestamp(2024, 5, 1, 10, 0, 0)"));
    assert!(
        boolean(r#"this.at == this.at.in_zone("Asia/Tokyo")"#),
        "same instant"
    );
    assert!(boolean(
        r#"parse_timestamp("2024-05-01T12:00:00+02:00") < parse_timestamp("2024-05-01T11:00:00Z")"#
    ));
    assert!(boolean(
        "(this.end - this.start) > (date(2024, 1, 2) - date(2024, 1, 1))"
    ));
    assert!(boolean("this.start in [date(2024, 1, 31)]"));
    assert!(!boolean("this.start == ()"));
    assert!(boolean("this.none == ()"));
}

#[test]
fn comparing_across_kinds_or_with_a_string_fails_instead_of_answering_false() {
    assert_contains(&fails("boolean", "this.start < this.local"), "date()");
    assert_contains(&fails("boolean", "this.local == this.start"), "date()");
    assert_contains(
        &fails("boolean", r#"this.start > "2024-01-01""#),
        "parse_date",
    );
    assert_contains(
        &fails("boolean", r#""2024-01-01" == this.start"#),
        "parse_date",
    );
    assert_contains(
        &fails("boolean", r#"this.at > "2024-01-01""#),
        "parse_timestamp",
    );
    assert_contains(&fails("boolean", r#"this.clock != "10:00""#), "parse_time");
    assert_contains(&fails("boolean", "this.at > this.local"), "zone");
}

// ── nulls, strings, range ─────────────────────────────────────────────────────

#[test]
fn a_null_date_is_unit_and_no_function_takes_it() {
    assert_eq!(int("this.none?.year ?? 0"), 0);
    assert_eq!(ok("date", "this.none"), GenericValue::Null);
    assert_contains(&fails("long", "this.none.add_days(1)"), "add_days");
    assert_contains(&fails("long", "this.none - this.start"), "-");
}

#[test]
fn a_string_is_not_a_date() {
    assert_contains(&fails("keyword", r#""2024-05-01".add_days(1)"#), "add_days");
    assert_contains(
        &fails("keyword", "this.start.sub_string(0, 4)"),
        "sub_string",
    );
}

#[test]
fn a_result_out_of_range_fails_the_document() {
    assert_contains(
        &fails("keyword", "this.start.add_years(300000)"),
        "out of range",
    );
    assert_contains(
        &fails("keyword", "this.start.add_days(9223372036854775807)"),
        "out of range",
    );
    assert_contains(
        &fails("keyword", "this.start.add_months(4294967296)"),
        "out of range",
    );
    assert_contains(
        &fails("keyword", "this.local.add_seconds(9223372036854775807)"),
        "out of range",
    );
    assert_contains(
        &fails("keyword", "date(2024, 1, 4294967297)"),
        "no such date",
    );
}

#[test]
fn an_error_carries_its_line_and_column() {
    let failure = run(
        "keyword",
        "let d = this.start;\nd.add_days(1);\nd.in_zone(\"x\")",
    )
    .unwrap_err();
    assert_eq!(failure.field, "out");
    let position = failure.position.unwrap();
    assert_eq!(position.line, 3);
    assert_eq!(position.column, 3);
    assert_eq!(position.excerpt, "d.in_zone(\"x\")");

    let failure = run("keyword", "this.at.in_zone(\"Mars/Base\")").unwrap_err();
    assert_contains(&failure.message, "Mars/Base");
    assert_eq!(failure.position.unwrap().line, 1);
}

// ── results ───────────────────────────────────────────────────────────────────

#[test]
fn a_date_result_is_written_as_the_declared_type() {
    assert_eq!(
        ok("date", "this.start.add_days(1)"),
        GenericValue::Date(date(2024, 2, 1))
    );
    assert_eq!(
        ok("timestamp", "this.local.add_hours(1)"),
        GenericValue::Timestamp(naive(2024, 5, 1, 11, 0, 0))
    );
    assert_eq!(
        ok("timestamp", r#"this.local.at_zone("Europe/Rome")"#),
        GenericValue::TimestampTz(utc(2024, 5, 1, 8, 0, 0)),
        "a zoned result is its UTC instant"
    );
    assert_eq!(
        ok("timestamp", "this.clock"),
        GenericValue::Time(NaiveTime::from_hms_milli_opt(10, 15, 30, 250).unwrap())
    );
    assert_eq!(
        ok("timestamp", "this.start.at_time(time(9, 0, 0))"),
        GenericValue::Timestamp(naive(2024, 1, 31, 9, 0, 0))
    );
    assert_eq!(
        ok("timestamp", "this.start"),
        GenericValue::Date(date(2024, 1, 31)),
        "a date is written as a date column under a timestamp field is"
    );
}

/// A `date` column declared `timestamp` builds a date; a script that hands it back unchanged
/// writes what the column path writes.
#[test]
fn a_date_column_under_a_timestamp_field_passes_through_a_script() {
    let scripts = compile(
        "  \
  - timestamp: day\n    required: true\n    emit: false\n  \
  - timestamp: copy\n    required: true\n    script: this.day\n",
    );
    let mut doc = GenericValue::Map(BTreeMap::from([
        ("day".to_owned(), GenericValue::Date(date(2024, 5, 1))),
        ("copy".to_owned(), GenericValue::Null),
    ]));
    scripts.apply(&index(), &mut doc).unwrap();
    let GenericValue::Map(map) = doc else {
        unreachable!()
    };
    assert_eq!(map["copy"], GenericValue::Date(date(2024, 5, 1)));
}

#[test]
fn a_mismatched_date_result_is_refused_with_what_to_do() {
    let message = fails("date", "this.local");
    assert_contains(&message, "returned a timestamp, expected a date");
    assert_contains(&message, ".date()");
    let message = fails("date", r#""2024-05-01""#);
    assert_contains(&message, "returned a string, expected a date");
    assert_contains(&message, "parse_date");
    assert_contains(
        &fails("timestamp", r#""2024-05-01T00:00:00Z""#),
        "parse_timestamp",
    );
    assert_contains(&fails("keyword", "this.start"), "to_string");
    assert_contains(&fails("date", "this.clock"), "returned a time");
    assert_contains(
        &fails("long", "this.end - this.start"),
        "returned a duration",
    );
}

#[test]
fn an_array_of_dates_is_built_and_checked_per_element() {
    let scripts = compile(
        "  \
  - date: start\n    required: true\n  \
  - date: weeks\n    required: true\n    array: true\n    script: |\n      let start = this.start;\n      [0, 1, 2].map(|n| start.add_weeks(n))\n  \
  - date: bad\n    required: false\n    array: true\n    script: '[this.start, this.start.to_string()]'\n",
    );
    let mut doc = GenericValue::Map(BTreeMap::from([
        ("start".to_owned(), GenericValue::Date(date(2024, 1, 1))),
        ("weeks".to_owned(), GenericValue::Null),
        ("bad".to_owned(), GenericValue::Null),
    ]));
    let failure = scripts.apply(&index(), &mut doc).unwrap_err();
    assert_eq!(failure.field, "bad");
    assert_contains(&failure.message, "element 1");

    let scripts = compile(
        "  \
  - date: start\n    required: true\n  \
  - date: weeks\n    required: true\n    array: true\n    script: |\n      let start = this.start;\n      [0, 1, 2].map(|n| start.add_weeks(n))\n",
    );
    let mut doc = GenericValue::Map(BTreeMap::from([
        ("start".to_owned(), GenericValue::Date(date(2024, 1, 1))),
        ("weeks".to_owned(), GenericValue::Null),
    ]));
    scripts.apply(&index(), &mut doc).unwrap();
    let GenericValue::Map(map) = doc else {
        unreachable!()
    };
    assert_eq!(
        map["weeks"],
        GenericValue::Array(vec![
            GenericValue::Date(date(2024, 1, 1)),
            GenericValue::Date(date(2024, 1, 8)),
            GenericValue::Date(date(2024, 1, 15)),
        ])
    );
}

// ── where dates arrive ────────────────────────────────────────────────────────

#[test]
fn dates_inside_a_has_many_and_an_array_column_arrive_typed() {
    let scripts = compile(
        "  \
  - date: holidays\n    required: true\n    array: true\n  \
  - integer: holidayMonths\n    required: true\n    array: true\n    script: this.holidays.map(|d| d.month)\n  \
  - has_many: stays\n    table: stays\n    primary_key: id\n    foreign_key: event_id\n    fields:\n      \
    - date: from\n        required: true\n      \
    - date: to\n        required: true\n      \
    - integer: nights\n        required: true\n        script: (this.to - this.from).days\n      \
    - has_many: visits\n        table: visits\n        primary_key: id\n        foreign_key: stay_id\n        fields:\n          \
      - timestamp: at\n            required: true\n          \
      - integer: hour\n            required: true\n            script: this.at.in_zone(\"Europe/Rome\").hour\n",
    );
    let stay = |from: NaiveDate, to: NaiveDate| {
        GenericValue::Map(BTreeMap::from([
            ("from".to_owned(), GenericValue::Date(from)),
            ("to".to_owned(), GenericValue::Date(to)),
            ("nights".to_owned(), GenericValue::Null),
            (
                "visits".to_owned(),
                GenericValue::Array(vec![GenericValue::Map(BTreeMap::from([
                    (
                        "at".to_owned(),
                        GenericValue::TimestampTz(utc(2024, 7, 1, 22, 0, 0)),
                    ),
                    ("hour".to_owned(), GenericValue::Null),
                ]))]),
            ),
        ]))
    };
    let mut doc = GenericValue::Map(BTreeMap::from([
        (
            "holidays".to_owned(),
            GenericValue::Array(vec![
                GenericValue::Date(date(2024, 1, 6)),
                GenericValue::Date(date(2024, 8, 15)),
            ]),
        ),
        ("holidayMonths".to_owned(), GenericValue::Null),
        (
            "stays".to_owned(),
            GenericValue::Array(vec![
                stay(date(2024, 2, 27), date(2024, 3, 2)),
                stay(date(2023, 2, 27), date(2023, 3, 2)),
            ]),
        ),
    ]));
    scripts.apply(&index(), &mut doc).unwrap();
    let GenericValue::Map(map) = &doc else {
        unreachable!()
    };
    assert_eq!(
        map["holidayMonths"],
        GenericValue::Array(vec![GenericValue::Int(1), GenericValue::Int(8)])
    );
    let GenericValue::Array(stays) = &map["stays"] else {
        panic!()
    };
    let field = |value: &GenericValue, key: &str| match value {
        GenericValue::Map(m) => m[key].clone(),
        _ => panic!(),
    };
    assert_eq!(
        field(&stays[0], "nights"),
        GenericValue::Int(4),
        "a leap year"
    );
    assert_eq!(field(&stays[1], "nights"), GenericValue::Int(3));
    let GenericValue::Array(visits) = field(&stays[0], "visits") else {
        panic!()
    };
    assert_eq!(
        field(&visits[0], "hour"),
        GenericValue::Int(0),
        "next day in Rome"
    );
}

#[test]
fn a_date_map_reaches_a_script_typed_and_reaches_the_sink_unchanged() {
    let scripts = compile(
        "  \
  - map: milestones\n    values: date\n    required: true\n  \
  - map: seen\n    values: timestamp\n    required: true\n  \
  - integer: launchYear\n    required: true\n    script: this.milestones.launch.year\n  \
  - keyword: odd\n    required: true\n    script: type_of(this.milestones.someday)\n  \
  - integer: seenHour\n    required: true\n    script: this.seen.first.in_zone(\"Asia/Tokyo\").hour\n  \
  - has_many: parts\n    table: parts\n    primary_key: id\n    foreign_key: event_id\n    fields:\n      \
    - map: due\n        values: date\n        required: true\n      \
    - integer: dueDay\n        required: true\n        script: root.milestones.launch.day + this.due.en.day\n",
    );
    let milestones = GenericValue::Map(BTreeMap::from([
        (
            "launch".to_owned(),
            GenericValue::String("2024-05-01".to_owned()),
        ),
        (
            "someday".to_owned(),
            GenericValue::String("soon".to_owned()),
        ),
    ]));
    let seen = GenericValue::Map(BTreeMap::from([(
        "first".to_owned(),
        GenericValue::String("2024-05-01T23:00:00+02:00".to_owned()),
    )]));
    let mut doc = GenericValue::Map(BTreeMap::from([
        ("milestones".to_owned(), milestones.clone()),
        ("seen".to_owned(), seen.clone()),
        ("launchYear".to_owned(), GenericValue::Null),
        ("odd".to_owned(), GenericValue::Null),
        ("seenHour".to_owned(), GenericValue::Null),
        (
            "parts".to_owned(),
            GenericValue::Array(vec![GenericValue::Map(BTreeMap::from([
                (
                    "due".to_owned(),
                    GenericValue::Map(BTreeMap::from([(
                        "en".to_owned(),
                        GenericValue::String("2024-06-10".to_owned()),
                    )])),
                ),
                ("dueDay".to_owned(), GenericValue::Null),
            ]))]),
        ),
    ]));
    scripts.apply(&index(), &mut doc).unwrap();
    let GenericValue::Map(map) = &doc else {
        unreachable!()
    };
    assert_eq!(map["launchYear"], GenericValue::Int(2024));
    assert_eq!(
        map["odd"],
        GenericValue::String("string".to_owned()),
        "a value that doesn't parse stays text"
    );
    assert_eq!(map["seenHour"], GenericValue::Int(6));
    assert_eq!(map["milestones"], milestones, "the document keeps the text");
    assert_eq!(map["seen"], seen);
    let GenericValue::Array(parts) = &map["parts"] else {
        panic!()
    };
    let GenericValue::Map(part) = &parts[0] else {
        panic!()
    };
    assert_eq!(part["dueDay"], GenericValue::Int(11));
}

#[test]
fn an_input_only_date_is_read_then_stripped() {
    let scripts = compile(
        "  \
  - date: born\n    required: true\n    emit: false\n  \
  - date: joined\n    required: true\n  \
  - integer: ageAtJoining\n    required: true\n    script: this.born.years_until(this.joined)\n",
    );
    let mut doc = GenericValue::Map(BTreeMap::from([
        ("born".to_owned(), GenericValue::Date(date(1990, 6, 15))),
        ("joined".to_owned(), GenericValue::Date(date(2024, 6, 14))),
        ("ageAtJoining".to_owned(), GenericValue::Null),
    ]));
    scripts.apply(&index(), &mut doc).unwrap();
    assert_eq!(
        doc,
        GenericValue::Map(BTreeMap::from([
            ("joined".to_owned(), GenericValue::Date(date(2024, 6, 14))),
            ("ageAtJoining".to_owned(), GenericValue::Int(33)),
        ]))
    );
}

#[test]
fn a_date_no_script_touches_passes_through_unchanged() {
    let scripts = compile(
        "  \
  - date: day\n    required: true\n  \
  - timestamp: at\n    required: true\n  \
  - timestamp: local\n    required: true\n  \
  - timestamp: clock\n    required: true\n  \
  - keyword: label\n    required: true\n    script: '\"fixed\"'\n",
    );
    let fields = BTreeMap::from([
        ("day".to_owned(), GenericValue::Date(date(2024, 2, 29))),
        (
            "at".to_owned(),
            GenericValue::TimestampTz(
                DateTime::parse_from_rfc3339("2024-05-01T10:00:00.123456Z")
                    .unwrap()
                    .to_utc(),
            ),
        ),
        (
            "local".to_owned(),
            GenericValue::Timestamp(naive(2024, 5, 1, 10, 0, 0)),
        ),
        (
            "clock".to_owned(),
            GenericValue::Time(NaiveTime::from_hms_micro_opt(1, 2, 3, 4).unwrap()),
        ),
    ]);
    let mut doc = fields.clone();
    doc.insert("label".to_owned(), GenericValue::Null);
    let mut doc = GenericValue::Map(doc);
    scripts.apply(&index(), &mut doc).unwrap();
    let mut want = fields;
    want.insert("label".to_owned(), GenericValue::String("fixed".to_owned()));
    assert_eq!(doc, GenericValue::Map(want));
}

// ── no silent answers (review of #365) ────────────────────────────────────────

/// One value of each kind a script can compare a date value with, by name.
const OPERANDS: [(&str, &str); 15] = [
    ("date", "this.start"),
    ("time", "this.clock"),
    ("zoned", "this.at"),
    ("plain", "this.local"),
    ("duration", "(this.end - this.start)"),
    ("string", r#""2024-01-31""#),
    ("int", "30"),
    ("float", "30.0"),
    ("decimal", r#"parse_decimal("30")"#),
    ("bool", "true"),
    ("char", "'a'"),
    ("unit", "()"),
    ("array", "[1]"),
    ("map", "#{a: 1}"),
    ("blob", "blob(1)"),
];

const DATE_KINDS: [&str; 5] = ["date", "time", "zoned", "plain", "duration"];

/// What comparing two operands must do: answer, or fail with a message.
fn comparison_answers(a: &str, b: &str, op: &str) -> bool {
    let both_dates = DATE_KINDS.contains(&a) && DATE_KINDS.contains(&b);
    if both_dates {
        return a == b;
    }
    (a == "unit" || b == "unit") && (op == "==" || op == "!=")
}

#[test]
fn every_comparison_with_a_date_value_answers_by_a_rule_or_fails() {
    for (a, left) in OPERANDS {
        for (b, right) in OPERANDS {
            if !DATE_KINDS.contains(&a) && !DATE_KINDS.contains(&b) {
                continue;
            }
            for op in ["==", "!=", "<", "<=", ">", ">="] {
                let script = format!("{left} {op} {right}");
                if comparison_answers(a, b, op) {
                    assert!(run("boolean", &script).is_ok(), "`{script}` should answer");
                } else {
                    let message = fails("boolean", &script);
                    assert_contains(&message, "can't be compared");
                }
            }
        }
    }
}

#[test]
fn a_duration_compared_with_a_number_says_to_read_a_unit() {
    let message = fails("boolean", "(this.end - this.start) > 30");
    assert_contains(&message, "a duration and a number can't be compared");
    assert_contains(&message, ".days");
    assert_contains(&fails("boolean", "30 == (this.end - this.start)"), ".days");
    assert!(boolean("(this.end - this.start).days > 29"));
}

#[test]
fn a_null_date_is_still_tested_with_equality() {
    assert!(boolean("this.none == ()"));
    assert!(boolean("this.start != ()"));
    assert_contains(&fails("boolean", "this.start < ()"), "can't be compared");
}

#[test]
fn to_json_writes_dates_as_the_text_scripts_read() {
    let json = text(
        "#{d: this.start, t: this.at, l: this.local, c: this.clock, n: [this.end, 1], s: \"x\"}.to_json()",
    );
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        value,
        serde_json::json!({
            "d": "2024-01-31",
            "t": "2024-03-31T00:30:00Z",
            "l": "2024-05-01T10:00:00",
            "c": "10:15:30.250",
            "n": ["2024-03-01", 1],
            "s": "x",
        })
    );
    let whole: serde_json::Value = serde_json::from_str(&text("this.to_json()")).unwrap();
    assert_eq!(whole["start"], "2024-01-31");
    assert_eq!(whole["none"], serde_json::Value::Null);
    let duration: serde_json::Value =
        serde_json::from_str(&text("#{d: this.end - this.start}.to_json()")).unwrap();
    assert_eq!(duration["d"], "P30D");
    assert_eq!(
        text("#{a: 1, b: [1.5, \"x\"]}.to_json()"),
        r#"{"a":1,"b":[1.5,"x"]}"#
    );
}

#[test]
fn a_container_of_dates_prints_each_one_quoted_as_strings_were() {
    assert_eq!(
        text("`${[this.start, this.end]}`"),
        r#"["2024-01-31", "2024-03-01"]"#
    );
    assert_eq!(
        text("[this.start, this.end].to_string()"),
        r#"["2024-01-31", "2024-03-01"]"#
    );
    assert_eq!(
        text("#{d: this.start}.to_string()"),
        r#"#{"d": "2024-01-31"}"#
    );
    assert_eq!(text("this.start.to_debug()"), r#""2024-01-31""#);
    assert_eq!(text("`${this.start}`"), "2024-01-31");
    assert_eq!(text("this.start.to_string()"), "2024-01-31");
}

#[test]
fn a_switch_on_a_date_takes_the_default_arm() {
    assert_eq!(
        text("switch this.start { \"2024-01-31\" => \"matched\", _ => \"default\" }"),
        "default",
        "documented: a switch can't match a date; use if"
    );
    assert_eq!(
        text("switch this.start.to_string() { \"2024-01-31\" => \"matched\", _ => \"default\" }"),
        "matched"
    );
}

#[test]
fn an_array_of_one_kind_sorts_and_two_values_have_a_min_and_max() {
    assert_eq!(
        text("let a = [this.end, this.start, date(2024, 2, 1)]; a.sort(); a.to_string()"),
        r#"["2024-01-31", "2024-02-01", "2024-03-01"]"#
    );
    assert_eq!(
        text("let a = [this.start, this.end]; a.sort_desc(); a.to_string()"),
        r#"["2024-03-01", "2024-01-31"]"#
    );
    assert_eq!(
        text(
            r#"let a = [this.at, this.at.add_hours(-1).in_zone("Asia/Tokyo")]; a.sort(); a[0].zone"#
        ),
        "Asia/Tokyo"
    );
    assert_eq!(
        text("let a = [time(9, 0, 0), this.clock, time(1, 0, 0)]; a.sort(); a.to_string()"),
        r#"["01:00:00", "09:00:00", "10:15:30.250"]"#
    );
    assert_eq!(
        text("let a = [3, 1, 2]; a.sort(); a.to_string()"),
        "[1, 2, 3]"
    );
    assert_eq!(
        text(r#"let a = ["b", "a"]; a.sort(); a.to_string()"#),
        r#"["a", "b"]"#
    );
    assert_contains(
        &fails(
            "keyword",
            "let a = [this.start, this.local]; a.sort(); a.to_string()",
        ),
        "cannot be sorted",
    );
    assert_contains(
        &fails(
            "keyword",
            "let a = [this.at, this.local]; a.sort(); a.to_string()",
        ),
        "zone",
    );
    assert_eq!(text("max(this.start, this.end).to_string()"), "2024-03-01");
    assert_eq!(text("min(this.start, this.end).to_string()"), "2024-01-31");
    assert_eq!(
        text("min(this.clock, time(1, 0, 0)).to_string()"),
        "01:00:00"
    );
    assert_eq!(int("max(this.at, this.at.add_hours(1)).hour"), 1);
    assert_contains(
        &fails("keyword", "max(this.start, this.local)"),
        "can't be compared",
    );
}

#[test]
fn a_map_is_indexed_by_a_dates_text() {
    assert_eq!(
        int("let m = #{}; m[this.start.to_string()] = 1; m[\"2024-01-31\"]"),
        1
    );
    let message = fails("long", "let m = #{}; m[this.start] = 1; 1");
    assert_contains(&message, "date");
}

#[test]
fn messages_name_the_operation_and_the_kind() {
    assert_contains(&fails("long", "(this.at - this.local).hours"), "subtracted");
    let message = fails("keyword", "this.clock.add_seconds(9223372036854775807)");
    assert!(!message.contains("date out of range"), "{message}");
    assert_contains(&message, "out of range");
}

#[test]
fn zone_names_are_read_regardless_of_case() {
    assert_eq!(text(r#"this.at.in_zone("utc").zone"#), "UTC");
    assert_eq!(
        text(r#"this.at.in_zone("europe/rome").zone"#),
        "Europe/Rome"
    );
    assert_eq!(
        text(r#"this.at.in_zone("AMERICA/NEW_YORK").zone"#),
        "America/New_York"
    );
}

#[test]
fn parse_timestamp_reads_postgres_text_offsets() {
    for text_form in [
        "2024-05-01 10:00:00+02",
        "2024-05-01 10:00:00+0200",
        "2024-05-01T10:00:00+0200",
        "2024-05-01 10:00:00.5+02:00",
    ] {
        let script = format!(r#"parse_timestamp("{text_form}").in_zone("UTC").hour"#);
        assert_eq!(int(&script), 8, "{text_form}");
        let zone = format!(r#"parse_timestamp("{text_form}").zone"#);
        assert_eq!(text(&zone), "+02:00", "{text_form}");
    }
}

/// The "Write instead" cells of the upgrade page's table of script changes, read from the page
/// itself: every complete snippet in them (no `…`, not a bare `.method()` or word), with each row's
/// snippets as written.
fn upgrade_replacements() -> Vec<(String, Vec<String>)> {
    let page = include_str!("../../../docs/src/operate/upgrade-to-0-18.md");
    let section = page
        .split("## A date reaches a script as a date")
        .nth(1)
        .unwrap();
    let section = section.split("\n## ").next().unwrap();
    let mut rows = Vec::new();
    for line in section.lines().filter(|l| l.starts_with("| ")) {
        let cells = split_cells(line);
        if cells.len() != 3 || cells[0] == "0.17 script" || cells[0].starts_with("---") {
            continue;
        }
        let snippets = code_spans(&cells[2])
            .into_iter()
            .filter(|s| {
                !s.contains('…')
                    && !s.starts_with('.')
                    && !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            })
            .collect();
        rows.push((cells[0].clone(), snippets));
    }
    rows
}

/// A markdown table row's cells, `\|` read as a pipe inside a cell.
fn split_cells(line: &str) -> Vec<String> {
    let mut cells = vec![String::new()];
    let mut chars = line.trim().trim_start_matches('|').chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'|') => {
                cells.last_mut().unwrap().push('|');
                chars.next();
            }
            '|' => cells.push(String::new()),
            other => cells.last_mut().unwrap().push(other),
        }
    }
    cells.pop();
    cells.into_iter().map(|c| c.trim().to_owned()).collect()
}

/// The single-backtick code spans of a cell.
fn code_spans(cell: &str) -> Vec<String> {
    cell.split('`')
        .enumerate()
        .filter(|(i, part)| i % 2 == 1 && !part.is_empty())
        .map(|(_, part)| part.to_owned())
        .collect()
}

/// Every replacement on the upgrade page runs, on a `timestamptz` and on a `timestamp`
/// `createdAt`: the page is read, not copied, so a replacement edited there is what runs here.
#[test]
fn every_replacement_on_the_upgrade_page_runs() {
    let rows = upgrade_replacements();
    assert!(rows.len() >= 12, "the table was not found: {rows:?}");
    let mut ran = 0;
    for (old, snippets) in &rows {
        assert!(
            !snippets.is_empty() || old.contains("unchanged") || old.contains("${"),
            "row `{old}` has no runnable replacement"
        );
        for snippet in snippets {
            for created in [
                GenericValue::TimestampTz(utc(2024, 5, 1, 10, 0, 0)),
                GenericValue::Timestamp(naive(2024, 5, 1, 10, 0, 0)),
            ] {
                let body = format!("let m = #{{}};\nlet r = {{ {snippet} }};\ntype_of(r)");
                let yaml = format!(
                    "  \
  - timestamp: createdAt\n    required: true\n  \
  - date: day\n    required: true\n  \
  - date: days\n    required: true\n    array: true\n  \
  - map: due\n    values: date\n    required: true\n  \
  - keyword: out\n    required: false\n    script: |\n      {}\n",
                    body.replace('\n', "\n      ")
                );
                let scripts = compile(&yaml);
                let mut doc = GenericValue::Map(BTreeMap::from([
                    ("createdAt".to_owned(), created.clone()),
                    ("day".to_owned(), GenericValue::Date(date(2024, 5, 1))),
                    (
                        "days".to_owned(),
                        GenericValue::Array(vec![
                            GenericValue::Date(date(2024, 5, 2)),
                            GenericValue::Date(date(2024, 5, 1)),
                        ]),
                    ),
                    (
                        "due".to_owned(),
                        GenericValue::Map(BTreeMap::from([(
                            "en".to_owned(),
                            GenericValue::String("2024-06-10".to_owned()),
                        )])),
                    ),
                    ("out".to_owned(), GenericValue::Null),
                ]));
                if let Err(failure) = scripts.apply(&index(), &mut doc) {
                    panic!("`{snippet}` (row `{old}`) fails on {created:?}: {failure}");
                }
                ran += 1;
            }
        }
    }
    assert!(ran >= 30, "only {ran} replacements ran");
}

/// The upgrade table's row for containers: each of these fails, though it looks for no date.
#[test]
fn searching_a_container_that_mixes_dates_with_other_kinds_fails() {
    for script in [
        "[1, this.start].contains(this.start)",
        "[this.start, 1].contains(1)",
        "[this.start] == [1]",
        r#"#{a: this.start} == #{a: "2024-01-01"}"#,
    ] {
        assert_contains(&fails("boolean", script), "can't be compared");
    }
}

// ── the edges of the calendar (review of #365, round two) ─────────────────────

#[test]
fn a_named_zone_reaches_as_far_as_a_fixed_offset() {
    for (zone, hour) in [("Europe/Rome", 1), ("+01:00", 1)] {
        let edge = format!(r#"parse_timestamp("9999-12-31T23:59:59Z").in_zone("{zone}")"#);
        assert_eq!(int(&format!("{edge}.year")), 10000, "{zone}");
        assert_eq!(int(&format!("{edge}.hour")), 0, "{zone}");
        let start = format!(r#"parse_timestamp("9999-12-31T00:00:00Z").in_zone("{zone}")"#);
        assert_eq!(int(&format!("{start}.hour")), hour, "{zone}");
        assert_eq!(int(&format!("{start}.add_hours(48).day")), 2, "{zone}");
        assert_eq!(int(&format!("{start}.add_days(400).year")), 10001, "{zone}");
        assert_eq!(int(&format!("{start}.end_of_year().year")), 9999, "{zone}");
        assert_eq!(int(&format!("{start}.start_of_month().day")), 1, "{zone}");
        assert_eq!(
            int(&format!(
                r#"timestamp(10000, 7, 1, 0, 0, 0).at_zone("{zone}").in_zone("UTC").hour"#
            )),
            23,
            "{zone}: the last offset holds past the database"
        );
        assert_eq!(
            int(&format!(
                r#"timestamp(9999, 12, 31, 23, 0, 0).at_zone("{zone}").add_hours(2).year"#
            )),
            10000,
            "{zone}"
        );
        for year in [-9999, -50000] {
            let far = year;
            let past =
                format!(r#"timestamp({year}, 6, 15, 12, 0, 0).at_zone("UTC").in_zone("{zone}")"#);
            assert!(run("long", &format!("{past}.year")).is_ok(), "{zone} {far}");
            assert!(
                run("keyword", &format!("{past}.to_string()")).is_ok(),
                "{zone} {far}"
            );
            assert!(
                run("keyword", &format!("{past}.start_of_year().to_string()")).is_ok(),
                "{zone} {far}"
            );
            assert!(
                run("keyword", &format!("{past}.add_hours(-48).to_string()")).is_ok(),
                "{zone} {far}"
            );
            assert!(
                run(
                    "keyword",
                    &format!(r#"timestamp(-9999, 1, 1, 0, 0, 0).at_zone("{zone}").to_string()"#)
                )
                .is_ok(),
                "{zone}"
            );
        }
    }
    assert_eq!(
        text(r#"parse_timestamp("9999-12-31T00:00:00Z").in_zone("Europe/Rome").format("%Z")"#),
        "CET"
    );
    assert!(
        run(
            "keyword",
            r#"timestamp(-9999, 1, 1, 0, 0, 0).at_zone("UTC").in_zone("Europe/Rome").format("%Z %z")"#
        )
        .is_ok()
    );
}

#[test]
fn a_leap_second_reads_in_a_named_zone() {
    let leap = r#"parse_timestamp("2016-12-31T23:59:60Z")"#;
    assert_eq!(int(&format!(r#"{leap}.in_zone("Europe/Rome").hour"#)), 0);
    assert_eq!(int(&format!(r#"{leap}.in_zone("Europe/Rome").day"#)), 1);
    assert_eq!(int(&format!(r#"{leap}.in_zone("+01:00").hour"#)), 0);
    assert_eq!(
        int(r#"timestamp(2016, 12, 31, 23, 59, 59).add_seconds(1).at_zone("Europe/Rome").hour"#),
        0
    );
}

#[test]
fn a_duration_compared_with_null_gets_the_null_hint() {
    let message = fails("boolean", "(this.end - this.start) < ()");
    assert_contains(&message, "null");
}

#[test]
fn a_users_own_error_text_is_left_as_written() {
    let failure = run("keyword", r#"throw "my script::dates::Date thing""#).unwrap_err();
    assert_contains(&failure.message, "my script::dates::Date thing");
}

#[test]
fn adding_months_from_the_anchor_keeps_month_ends() {
    assert_eq!(
        text("let d = date(2024, 1, 31); for i in 0..3 { d = d.add_months(1); } d.to_string()"),
        "2024-04-29",
        "stepping drifts"
    );
    assert_eq!(
        text("date(2024, 1, 31).add_months(3).to_string()"),
        "2024-04-30"
    );
}

// ── properties ────────────────────────────────────────────────────────────────

mod properties {
    use proptest::prelude::*;

    use super::*;

    /// One script of each kind over inputs `a` (a date), `t` (a zoned timestamp) and `n`.
    fn scripts() -> Scripts {
        compile(
            "  \
  - date: a\n    required: true\n  \
  - timestamp: t\n    required: true\n  \
  - timestamp: u\n    required: true\n  \
  - long: n\n    required: true\n  \
  - date: roundTrip\n    required: true\n    script: this.a.add_days(this.n).add_days(-this.n)\n  \
  - long: dayDiff\n    required: true\n    script: (this.a.add_days(this.n) - this.a).days\n  \
  - date: b\n    required: true\n  \
  - long: monthsForward\n    required: true\n    script: this.a.months_until(this.b)\n  \
  - long: monthsBack\n    required: true\n    script: this.b.months_until(this.a)\n  \
  - boolean: notPast\n    required: true\n    script: |\n      let n = this.a.months_until(this.b);\n      let moved = this.a.add_months(n);\n      if n >= 0 { moved <= this.b } else { moved >= this.b }\n  \
  - timestamp: hourTrip\n    required: true\n    script: this.t.in_zone(\"America/New_York\").add_hours(this.n).add_hours(-this.n)\n  \
  - long: seconds\n    required: true\n    script: (this.u.in_zone(\"Europe/Rome\") - this.t).seconds\n",
        )
    }

    fn day() -> impl Strategy<Value = NaiveDate> {
        (-700_000i32..1_400_000).prop_map(|n| NaiveDate::from_num_days_from_ce_opt(n).unwrap())
    }

    fn instant() -> impl Strategy<Value = DateTime<Utc>> {
        (-5_000_000_000i64..10_000_000_000).prop_map(|s| DateTime::from_timestamp(s, 0).unwrap())
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn arithmetic_inverts_and_whole_months_are_symmetric(
            a in day(),
            b in day(),
            t in instant(),
            u in instant(),
            n in -2_000i64..2_000,
        ) {
            let scripts = scripts();
            let mut doc = GenericValue::Map(BTreeMap::from([
                ("a".to_owned(), GenericValue::Date(a)),
                ("b".to_owned(), GenericValue::Date(b)),
                ("t".to_owned(), GenericValue::TimestampTz(t)),
                ("u".to_owned(), GenericValue::TimestampTz(u)),
                ("n".to_owned(), GenericValue::BigInt(n)),
            ]));
            scripts.apply(&index(), &mut doc).unwrap();
            let GenericValue::Map(map) = doc else { unreachable!() };
            prop_assert_eq!(&map["roundTrip"], &GenericValue::Date(a));
            prop_assert_eq!(&map["dayDiff"], &GenericValue::BigInt(n));
            let GenericValue::BigInt(forward) = map["monthsForward"] else { unreachable!() };
            prop_assert_eq!(&map["monthsBack"], &GenericValue::BigInt(-forward), "sign symmetry");
            prop_assert_eq!(&map["notPast"], &GenericValue::Bool(true));
            prop_assert_eq!(&map["hourTrip"], &GenericValue::TimestampTz(t));
            prop_assert_eq!(&map["seconds"], &GenericValue::BigInt((u - t).num_seconds()));
        }
    }
}

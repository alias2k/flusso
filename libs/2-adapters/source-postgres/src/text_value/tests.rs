use chrono::{NaiveDate, TimeZone, Utc};

use super::*;

fn text(value: &str) -> GenericValue {
    GenericValue::String(value.to_owned())
}

/// A BC year is chrono's proleptic year, as sqlx's binary decode of the same
/// value gave the backfill before keys were read as text.
#[test]
fn a_bc_year_is_the_proleptic_year() {
    let ides = NaiveDate::from_ymd_opt(-43, 3, 15).unwrap();
    assert_eq!(typed_value("0044-03-15 BC", 1082), GenericValue::Date(ides));
    assert_eq!(
        typed_value("0044-03-15 10:00:00.5 BC", 1114),
        GenericValue::Timestamp(ides.and_hms_milli_opt(10, 0, 0, 500).unwrap())
    );
    assert_eq!(
        typed_value("0044-03-15 12:00:00+02 BC", 1184),
        GenericValue::TimestampTz(Utc.from_utc_datetime(&ides.and_hms_opt(10, 0, 0).unwrap()))
    );
}

/// The offset is the session time zone's; a local mean time has seconds.
#[test]
fn a_timestamptz_offset_may_carry_minutes_and_seconds() {
    let at = |h, m, s| {
        GenericValue::TimestampTz(
            Utc.from_utc_datetime(
                &NaiveDate::from_ymd_opt(1900, 1, 1)
                    .unwrap()
                    .and_hms_opt(h, m, s)
                    .unwrap(),
            ),
        )
    };
    assert_eq!(typed_value("1900-01-01 10:00:00+00", 1184), at(10, 0, 0));
    assert_eq!(
        typed_value("1900-01-01 10:00:00-03:30", 1184),
        at(13, 30, 0)
    );
    assert_eq!(
        typed_value("1900-01-01 10:53:28+00:53:28", 1184),
        at(10, 0, 0)
    );
    assert_eq!(
        typed_value("1900-01-01 10:00:00+1", 1184),
        text("1900-01-01 10:00:00+1")
    );
}

/// What chrono or `Decimal` can't hold stays the text Postgres rendered.
#[test]
fn a_value_out_of_range_stays_text() {
    for (value, oid) in [
        ("infinity", 1082),
        ("-infinity", 1114),
        ("infinity", 1184),
        ("NaN", 1700),
        ("Infinity", 1700),
        ("\\001\\253", 17),
    ] {
        assert_eq!(typed_value(value, oid), text(value), "{oid} {value}");
    }
}

/// A float key's text parses to the stored value, infinities and NaN too.
#[test]
fn a_float_key_parses_every_value_postgres_renders() {
    assert_eq!(typed_value("-0", 701), GenericValue::Double(-0.0));
    assert_eq!(
        typed_value("Infinity", 701),
        GenericValue::Double(f64::INFINITY)
    );
    assert_eq!(
        typed_value("-Infinity", 700),
        GenericValue::Float(f32::NEG_INFINITY)
    );
    assert!(matches!(typed_value("NaN", 701), GenericValue::Double(nan) if nan.is_nan()));
}

#[test]
fn a_key_type_is_a_decoded_or_text_type() {
    for oid in [23, 20, 1700, 17, 1082, 1184, 25, 1042] {
        assert!(is_key_type(oid, ""), "{oid}");
    }
    assert!(is_key_type(91_234, "citext"));
    for (oid, name) in [
        (26, "oid"),
        (3802, "jsonb"),
        (18, "\"char\""),
        (869, "inet"),
    ] {
        assert!(!is_key_type(oid, name), "{name}");
    }
}

/// A key is rendered in a form no session setting changes: `bytea` as hex,
/// a float as its bits; the rest through its output function.
#[test]
fn a_key_renders_independently_of_session_settings() {
    assert_eq!(key_text("k", "bytea"), r"'\x' || encode(k, 'hex')");
    assert_eq!(key_text("k", "float8"), "encode(float8send(k), 'hex')");
    assert_eq!(key_text("k", "float4"), "encode(float4send(k), 'hex')");
    assert_eq!(key_text("k", "numeric"), "format('%s', k)");
    assert_eq!(key_text("k", "bpchar"), "format('%s', k)");
}

/// A float read back from its bits is the value its shortest text parses to.
#[test]
fn a_float_reads_back_from_its_bits_as_its_text_parses() {
    for value in [0.0, -0.0, 1.5, 1.0 / 3.0, f64::INFINITY, f64::MIN_POSITIVE] {
        let bits = format!("{:016x}", value.to_bits());
        assert_eq!(
            read_back(&bits, 701),
            typed_value(&value.to_string(), 701),
            "{value}"
        );
    }
    let third = 1.0_f32 / 3.0;
    assert_eq!(
        read_back(&format!("{:08x}", third.to_bits()), 700),
        GenericValue::Float(third)
    );
    assert_eq!(
        read_back("\\x01ab", 17),
        GenericValue::Bytes(vec![0x01, 0xab])
    );
}

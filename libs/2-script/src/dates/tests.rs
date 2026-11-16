#![allow(clippy::unwrap_used)]

use super::*;

#[test]
fn a_duration_writes_iso_8601() {
    let cases = [
        (TimeDelta::zero(), "PT0S"),
        (TimeDelta::days(30), "P30D"),
        (TimeDelta::minutes(90), "PT1H30M"),
        (
            TimeDelta::days(1) + TimeDelta::milliseconds(500),
            "P1DT0.5S",
        ),
        (-TimeDelta::hours(36), "-P1DT12H"),
        (
            TimeDelta::seconds(59) + TimeDelta::nanoseconds(1),
            "PT59.000000001S",
        ),
    ];
    for (delta, text) in cases {
        assert_eq!(Duration(delta).to_string(), text);
    }
}

#[test]
fn a_zone_is_a_name_or_an_offset() {
    assert_eq!(Zone::parse("UTC").unwrap(), Zone::Utc);
    assert_eq!(
        Zone::parse("Europe/Rome").unwrap().to_string(),
        "Europe/Rome"
    );
    assert_eq!(Zone::parse("+05:30").unwrap().to_string(), "+05:30");
    assert!(Zone::parse("Europe/Roma").is_err());
    assert!(Zone::parse("").is_err());
}

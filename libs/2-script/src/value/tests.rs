#![allow(clippy::unwrap_used)]

use std::str::FromStr;

use rust_decimal::Decimal;

use super::*;

fn out(ty: &FlussoType) -> Output<'_> {
    Output {
        ty,
        nullable: false,
        array: false,
        variants: &[],
    }
}

#[test]
fn dates_cross_typed_and_uuids_as_strings() {
    let date = chrono::NaiveDate::from_ymd_opt(2024, 5, 1).unwrap();
    let rhai = to_dynamic(&GenericValue::Date(date));
    assert!(rhai.is::<Date>());
    let back = from_dynamic(rhai, &out(&FlussoType::Date));
    assert_eq!(back, Ok(GenericValue::Date(date)));

    let id = uuid::Uuid::nil();
    let back = from_dynamic(to_dynamic(&GenericValue::Uuid(id)), &out(&FlussoType::Uuid));
    assert_eq!(back, Ok(GenericValue::Uuid(id)));
}

#[test]
fn a_decimal_round_trips_exactly() {
    let price = Decimal::from_str("19.99").unwrap();
    let back = from_dynamic(
        to_dynamic(&GenericValue::Decimal(price)),
        &out(&FlussoType::Decimal),
    );
    assert_eq!(
        back,
        Ok(GenericValue::BigDecimal(
            kernel::BigDecimal::parse("19.99").unwrap()
        ))
    );
}

fn negative_zero(scale: u32) -> Decimal {
    let mut zero = Decimal::new(0, scale);
    zero.set_sign_negative(true);
    zero
}

fn big(text: &str) -> GenericValue {
    GenericValue::BigDecimal(kernel::BigDecimal::parse(text).unwrap())
}

/// A declared `decimal` reaches a script as a decimal wherever `Decimal` can
/// hold its magnitude (rounded to what `Decimal` holds, an exponent read),
/// as before issue #327; only a magnitude past `Decimal`'s range is a string of
/// its digits.
#[test]
fn a_declared_decimal_crosses_as_a_decimal_unless_past_the_range() {
    for (text, want) in [
        ("1.000000000000000000000000000000", Decimal::ONE),
        ("12.50", Decimal::new(1250, 2)),
        ("1e2", Decimal::new(100, 0)),
        ("-1.5E+3", Decimal::new(-1500, 0)),
    ] {
        let rhai = to_dynamic(&big(text));
        assert_eq!(rhai.as_decimal(), Ok(want), "{text}");
    }
    assert_eq!(
        to_dynamic(&big("1000000000000000000000000000000"))
            .into_string()
            .unwrap(),
        "1000000000000000000000000000000"
    );
}

/// A declared decimal with more than 28 fractional digits and no exponent
/// reaches a script as the nearest `Decimal`, 28 fractional digits, like its
/// neighbour with 28; only an exponent or a zero takes `main`'s `f64` path.
#[test]
fn a_decimal_past_28_fractional_digits_crosses_as_the_nearest_decimal() {
    for (text, want) in [
        (
            "1.123456789012345678901234567891",
            "1.1234567890123456789012345679",
        ),
        (
            "3.141592653589793238462643383279",
            "3.1415926535897932384626433833",
        ),
        (
            "1.000000000000000000000000000000",
            "1.0000000000000000000000000000",
        ),
        (
            "-0.123456789012345678901234567890",
            "-0.1234567890123456789012345679",
        ),
    ] {
        let rhai = to_dynamic(&big(text));
        assert_eq!(
            rhai.as_decimal().map(|d| d.to_string()),
            Ok(want.to_owned()),
            "{text}"
        );
    }
}

/// A zero `decimal` result is written without a sign: `-0` reaches a script
/// from a tiny negative value, and `main` wrote such a zero as `0.0`.
#[test]
fn a_zero_decimal_result_is_written_without_a_sign() {
    let out = out(&FlussoType::Decimal);
    for (zero, want) in [(negative_zero(0), "0"), (negative_zero(3), "0.000")] {
        assert_eq!(
            from_dynamic(Dynamic::from_decimal(zero), &out),
            Ok(big(want)),
            "{zero}"
        );
    }
}

/// A `decimal` script field refuses any string, as on `main`, a string
/// holding a decimal number included: a value past `Decimal`'s range reaches a
/// script as a string, and arithmetic on it concatenates.
#[test]
fn a_decimal_field_refuses_a_string() {
    let out = out(&FlussoType::Decimal);
    for text in ["1000000000000000000000000000000", "12.5", "abc", "NaN"] {
        assert_eq!(
            from_dynamic(Dynamic::from(text.to_owned()), &out),
            Err("returned a string, expected a decimal (or an integer)".to_owned()),
            "{text}"
        );
    }
}

fn number_text() -> impl proptest::strategy::Strategy<Value = (String, bool, String)> {
    use proptest::prelude::*;
    let digits = "[0-9]{0,40}".prop_map(|d: String| d.trim_start_matches('0').to_owned());
    (
        any::<bool>(),
        digits,
        "[0-9]{0,30}",
        prop_oneof![Just(String::new()), "[eE][+-]?[0-9]{1,26}"],
    )
        .prop_map(|(negative, whole, fraction, exponent)| {
            let whole = if whole.is_empty() {
                "0".to_owned()
            } else {
                whole
            };
            let mut text = String::new();
            if negative {
                text.push('-');
            }
            text.push_str(&whole);
            if !fraction.is_empty() {
                text.push('.');
                text.push_str(&fraction);
            }
            let all_zero = whole.chars().chain(fraction.chars()).all(|c| c == '0');
            (format!("{text}{exponent}"), all_zero, exponent)
        })
}

/// The power of ten of `text`'s first significant digit, from its digits and
/// exponent alone; `None` for an exponent too long to matter (its sign then
/// decides).
fn order_of(text: &str, exponent: &str) -> Result<i128, bool> {
    let mantissa = text.trim_start_matches('-');
    let mantissa = mantissa.split(['e', 'E']).next().unwrap();
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let first = whole
        .trim_start_matches('0')
        .len()
        .checked_sub(1)
        .map(|n| n as i128)
        .unwrap_or_else(|| {
            -((fraction.len() - fraction.trim_start_matches('0').len()) as i128) - 1
        });
    let exponent = exponent.trim_start_matches(['e', 'E']);
    if exponent.is_empty() {
        return Ok(first);
    }
    let negative = exponent.starts_with('-');
    let digits = exponent
        .trim_start_matches(['+', '-'])
        .trim_start_matches('0');
    if digits.len() > 20 {
        return Err(negative);
    }
    let value: i128 = if digits.is_empty() {
        0
    } else {
        digits.parse().unwrap()
    };
    Ok(first + if negative { -value } else { value })
}

proptest::proptest! {
    /// Any number a declared decimal can hold reaches a script without a panic:
    /// zero when its digits are zero or its magnitude is below what `Decimal`
    /// holds, a decimal in between, and a string only past ±7.9e28, as a
    /// reference that works from the digits and exponent alone says (a band at
    /// each edge, where rounding decides, takes either side).
    #[test]
    fn a_declared_decimal_crosses_as_zero_a_decimal_or_a_string(
        (text, all_zero, exponent) in number_text()
    ) {
        let value = to_dynamic(&GenericValue::BigDecimal(
            kernel::BigDecimal::parse(&text).unwrap(),
        ));
        let seen = match value.as_decimal() {
            Ok(d) if d.is_zero() => "zero",
            Ok(_) => "decimal",
            Err(_) => {
                proptest::prop_assert!(value.is_string(), "{text}: {value:?}");
                "string"
            }
        };
        let want: &[&str] = if all_zero {
            &["zero"]
        } else {
            match order_of(&text, &exponent) {
                Err(true) => &["zero"],
                Err(false) => &["string"],
                Ok(order) if order >= 29 => &["string"],
                Ok(28) => &["decimal", "string"],
                Ok(order) if order >= -28 => &["decimal"],
                Ok(-29) => &["zero", "decimal"],
                Ok(_) => &["zero"],
            }
        };
        proptest::prop_assert!(want.contains(&seen), "{text}: {seen}, want one of {want:?}");
    }
}

#[test]
fn an_integer_widens_to_a_number_but_not_the_reverse() {
    assert_eq!(
        from_dynamic(Dynamic::from_int(2), &out(&FlussoType::Double)),
        Ok(GenericValue::Double(2.0))
    );
    let err = from_dynamic(Dynamic::from_float(2.5), &out(&FlussoType::Long)).unwrap_err();
    assert_eq!(err, "returned a float, expected an integer");
}

#[test]
fn a_mismatch_names_what_came_back() {
    let err = from_dynamic(Dynamic::from_int(1), &out(&FlussoType::Keyword)).unwrap_err();
    assert_eq!(err, "returned an integer, expected a string");
    let err = from_dynamic(Dynamic::from_int(70_000), &out(&FlussoType::Short)).unwrap_err();
    assert_eq!(err, "returned 70000, out of range for a short");
}

#[test]
fn unit_is_null_only_when_the_field_is_nullable() {
    let ty = FlussoType::Keyword;
    let err = from_dynamic(Dynamic::UNIT, &out(&ty)).unwrap_err();
    assert!(err.contains("`required: true`"), "{err}");
    let nullable = Output {
        nullable: true,
        ..out(&ty)
    };
    assert_eq!(
        from_dynamic(Dynamic::UNIT, &nullable),
        Ok(GenericValue::Null)
    );
}

#[test]
fn an_array_field_takes_an_array_of_its_leaf() {
    let ty = FlussoType::Keyword;
    let array = Output {
        array: true,
        ..out(&ty)
    };
    let value = Dynamic::from_array(vec![Dynamic::from("a".to_owned())]);
    assert_eq!(
        from_dynamic(value, &array),
        Ok(GenericValue::Array(vec![GenericValue::String(
            "a".to_owned()
        )]))
    );
    let err = from_dynamic(Dynamic::from("a".to_owned()), &array).unwrap_err();
    assert_eq!(err, "returned a string, expected an array");
    let err = from_dynamic(Dynamic::from_array(vec![Dynamic::UNIT]), &array).unwrap_err();
    assert!(err.starts_with("element 0"), "{err}");
}

#[test]
fn an_enum_must_return_a_declared_variant() {
    let variants = vec!["gold".to_owned(), "silver".to_owned()];
    let ty = FlussoType::Enum;
    let out = Output {
        variants: &variants,
        ..out(&ty)
    };
    assert!(from_dynamic(Dynamic::from("gold".to_owned()), &out).is_ok());
    let err = from_dynamic(Dynamic::from("bronze".to_owned()), &out).unwrap_err();
    assert!(err.contains("not a declared variant"), "{err}");
}

#[test]
fn a_timestamp_field_takes_a_timestamp_or_a_time() {
    let ty = FlussoType::Timestamp;
    let wall = chrono::NaiveDate::from_ymd_opt(2024, 5, 1)
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap();
    assert_eq!(
        from_dynamic(Dynamic::from(Timestamp::plain(wall)), &out(&ty)),
        Ok(GenericValue::Timestamp(wall))
    );
    let instant = Utc.from_utc_datetime(&wall);
    assert_eq!(
        from_dynamic(Dynamic::from(Timestamp::utc(instant)), &out(&ty)),
        Ok(GenericValue::TimestampTz(instant))
    );
    assert_eq!(
        from_dynamic(Dynamic::from(Time(wall.time())), &out(&ty)),
        Ok(GenericValue::Time(wall.time()))
    );
    let err =
        from_dynamic(Dynamic::from("2024-05-01T12:00:00Z".to_owned()), &out(&ty)).unwrap_err();
    assert!(err.contains("parse_timestamp"), "{err}");
}

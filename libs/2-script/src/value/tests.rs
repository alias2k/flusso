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
fn dates_and_uuids_cross_as_strings() {
    let date = NaiveDate::from_ymd_opt(2024, 5, 1).unwrap();
    assert_eq!(
        to_dynamic(&GenericValue::Date(date)).into_string().unwrap(),
        "2024-05-01"
    );
    let back = from_dynamic(
        Dynamic::from("2024-05-01".to_owned()),
        &out(&FlussoType::Date),
    );
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
    assert_eq!(back, Ok(GenericValue::Decimal(price)));
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
fn a_timestamp_accepts_an_offset_or_none() {
    let ty = FlussoType::Timestamp;
    assert!(matches!(
        from_dynamic(Dynamic::from("2024-05-01T12:00:00Z".to_owned()), &out(&ty)),
        Ok(GenericValue::TimestampTz(_))
    ));
    assert!(matches!(
        from_dynamic(Dynamic::from("2024-05-01T12:00:00".to_owned()), &out(&ty)),
        Ok(GenericValue::Timestamp(_))
    ));
    assert!(from_dynamic(Dynamic::from("yesterday".to_owned()), &out(&ty)).is_err());
}

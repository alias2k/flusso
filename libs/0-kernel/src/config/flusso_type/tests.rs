use super::*;

#[test]
fn enum_is_text_in_pg_keyword_in_opensearch() {
    let ty = FlussoType::Enum;
    assert_eq!(ty.opensearch(), MappingType::Keyword);
    assert!(ty.accepts_pg("character varying(32)"));
    assert!(ty.accepts_pg("text"));
    assert!(!ty.accepts_pg("integer"));
}

#[test]
fn integer_families_match_by_width() {
    assert!(FlussoType::Long.accepts_pg("bigint"));
    assert!(FlussoType::Integer.accepts_pg("integer"));
    assert!(FlussoType::Short.accepts_pg("smallint"));
    assert!(!FlussoType::Integer.accepts_pg("bigint"));
}

#[test]
fn arrays_and_modifiers_are_stripped() {
    assert!(FlussoType::Integer.accepts_pg("integer[]"));
    assert!(FlussoType::Decimal.accepts_pg("numeric(10,2)"));
    assert_eq!(FlussoType::Timestamp.opensearch(), MappingType::Date);
}

#[test]
fn geo_point_emits_geo_point_and_accepts_only_carryable_columns() {
    let ty = FlussoType::GeoPoint;
    assert_eq!(ty.opensearch(), MappingType::Other("geo_point".to_owned()));
    // Columns whose JSON survives document assembly as valid geo input.
    assert!(ty.accepts_pg("jsonb"));
    assert!(ty.accepts_pg("json"));
    assert!(ty.accepts_pg("text"));
    assert!(ty.accepts_pg("character varying(64)"));
    // PostGIS / PG-native point would serialize as WKB / `(x,y)`.
    assert!(!ty.accepts_pg("point"));
    assert!(!ty.accepts_pg("geometry"));
    assert!(!ty.accepts_pg("integer"));
}

#[test]
fn custom_carries_its_own_mapping_and_pg_set() {
    let ty = FlussoType::Custom {
        postgres: vec!["numeric".to_owned()],
        opensearch: "scaled_float".to_owned(),
    };
    assert_eq!(ty.opensearch(), MappingType::ScaledFloat);
    assert!(ty.accepts_pg("numeric(12,4)"));
    assert!(!ty.accepts_pg("text"));
}

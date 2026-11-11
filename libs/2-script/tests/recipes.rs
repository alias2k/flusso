//! Runs every recipe of the manual's script guide (`tests/recipes/recipes.schema.yml`,
//! included by anchor in `docs/src/author/scripts/recipes.md`) over one product and
//! checks each result, so a recipe the manual shows can't silently stop working.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    unused_crate_dependencies
)]

use std::collections::BTreeMap;
use std::str::FromStr;

use config::yaml::SchemaYaml;
use kernel::{GenericValue, IndexName, IndexSchema, ParseFrom};
use rust_decimal::Decimal;
use script::Scripts;
use source::SourceSpec;

/// The product as the Postgres builder would hand it over: typed values, and
/// the script fields as `null` placeholders.
fn product() -> GenericValue {
    let json: serde_json::Value =
        serde_json::from_str(include_str!("recipes/product.json")).unwrap();
    let mut doc = typed(json);
    if let GenericValue::Map(map) = &mut doc {
        for name in [
            "label",
            "slug",
            "displayName",
            "onSale",
            "grossPrice",
            "stockBand",
            "popularity",
            "createdYear",
            "displayTitle",
            "tagList",
            "tagLine",
            "cheapestColor",
            "variantsInStock",
        ] {
            map.insert(name.to_owned(), GenericValue::Null);
        }
    }
    doc
}

fn typed(json: serde_json::Value) -> GenericValue {
    match json {
        serde_json::Value::Null => GenericValue::Null,
        serde_json::Value::Bool(b) => GenericValue::Bool(b),
        serde_json::Value::Number(n) => n
            .as_i64()
            .map(GenericValue::BigInt)
            .unwrap_or_else(|| GenericValue::Double(n.as_f64().unwrap())),
        serde_json::Value::String(s) => GenericValue::String(s),
        serde_json::Value::Array(items) => {
            GenericValue::Array(items.into_iter().map(typed).collect())
        }
        serde_json::Value::Object(map) => GenericValue::Map(
            map.into_iter()
                .map(|(key, value)| {
                    let value = match (key.as_str(), value) {
                        ("price", serde_json::Value::String(s)) => {
                            GenericValue::Decimal(Decimal::from_str(&s).unwrap())
                        }
                        ("createdAt", serde_json::Value::String(s)) => GenericValue::TimestampTz(
                            chrono::DateTime::parse_from_rfc3339(&s).unwrap().to_utc(),
                        ),
                        ("variants", serde_json::Value::Array(items)) => GenericValue::Array(
                            items
                                .into_iter()
                                .map(|item| {
                                    let mut variant = typed(item);
                                    if let GenericValue::Map(m) = &mut variant {
                                        m.insert("sku".to_owned(), GenericValue::Null);
                                    }
                                    variant
                                })
                                .collect(),
                        ),
                        (_, value) => typed(value),
                    };
                    (key, value)
                })
                .collect(),
        ),
    }
}

fn get<'a>(doc: &'a GenericValue, key: &str) -> &'a GenericValue {
    match doc {
        GenericValue::Map(map) => map.get(key).unwrap_or(&GenericValue::Null),
        _ => &GenericValue::Null,
    }
}

fn text(s: &str) -> GenericValue {
    GenericValue::String(s.to_owned())
}

#[test]
fn every_recipe_in_the_manual_produces_what_it_says() {
    let yaml = include_str!("recipes/recipes.schema.yml");
    let schema = IndexSchema::try_from(SchemaYaml::try_parse(yaml).unwrap()).unwrap();
    let index = IndexName::try_new("products").unwrap();
    let scripts = Scripts::compile(&SourceSpec::new(BTreeMap::from([(index.clone(), schema)])))
        .unwrap_or_else(|errors| panic!("{errors}"));

    let mut doc = product();
    scripts
        .apply(&index, &mut doc)
        .unwrap_or_else(|failure| panic!("{failure}"));

    assert_eq!(get(&doc, "label"), &text("AB-12 · Red Mug"));
    assert_eq!(get(&doc, "slug"), &text("red-mug"));
    assert_eq!(get(&doc, "displayName"), &text("Red Mug"));
    assert_eq!(get(&doc, "onSale"), &GenericValue::Bool(true));
    assert_eq!(
        get(&doc, "grossPrice"),
        &GenericValue::Decimal(Decimal::from_str("15.25").unwrap())
    );
    assert_eq!(get(&doc, "stockBand"), &text("low"));
    assert_eq!(get(&doc, "popularity"), &GenericValue::Double(8.5));
    assert_eq!(get(&doc, "createdYear"), &GenericValue::Int(2024));
    assert_eq!(get(&doc, "displayTitle"), &text("Tazza"));
    assert_eq!(
        get(&doc, "tagList"),
        &GenericValue::Array(vec![text("kitchen"), text("sale")])
    );
    assert_eq!(get(&doc, "tagLine"), &text("kitchen, sale, kitchen"));
    assert_eq!(get(&doc, "cheapestColor"), &text("blue"));
    assert_eq!(get(&doc, "variantsInStock"), &GenericValue::Int(2));

    let GenericValue::Array(variants) = get(&doc, "variants") else {
        panic!("variants should stay an array");
    };
    assert_eq!(get(&variants[1], "sku"), &text("ab-12-blue"));

    for input_only in ["sku", "price", "title", "tags"] {
        assert_eq!(
            get(&doc, input_only),
            &GenericValue::Null,
            "`{input_only}` is stripped"
        );
    }
}

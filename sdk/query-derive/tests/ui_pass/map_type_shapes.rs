// The shapes a `FlussoMap` type takes: each reads one JSON object (issue #289).
use std::collections::{BTreeMap, HashMap};

use flusso_query::{FlussoMap, FlussoRoot};

#[derive(serde::Deserialize, FlussoMap)]
#[flusso(keyword)]
struct Codes(HashMap<String, String>);

#[derive(serde::Deserialize, FlussoMap)]
#[flusso(keyword)]
#[serde(transparent)]
struct Sorted {
    codes: BTreeMap<String, String>,
}

#[derive(serde::Deserialize, FlussoMap)]
#[flusso(keyword)]
struct Wrapped(Codes);

#[derive(serde::Deserialize, FlussoMap)]
#[flusso(keyword)]
struct Raw(serde_json::Value);

#[derive(serde::Deserialize, FlussoMap)]
#[flusso(text)]
struct Translations {
    en: Option<String>,
    #[serde(flatten)]
    rest: HashMap<String, String>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "products")]
struct Product {
    sku: String,
    codes: Option<Codes>,
    title: Translations,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "products")]
struct ProductSorted {
    sku: String,
    codes: Option<Sorted>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "products")]
struct ProductWrapped {
    sku: String,
    codes: Option<Wrapped>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "products")]
struct ProductRaw {
    sku: String,
    codes: Option<Raw>,
}

fn main() {
    let doc = r#"{"sku": "s", "codes": {"ean": "1"}, "title": {"en": "a", "it": "b"}}"#;
    let product: Product = serde_json::from_str(doc).unwrap();
    assert_eq!(product.codes.unwrap().0["ean"], "1");
    assert_eq!(product.title.rest["it"], "b");
    assert!(serde_json::from_str::<ProductSorted>(doc).unwrap().codes.is_some());
    assert!(serde_json::from_str::<ProductWrapped>(doc).unwrap().codes.is_some());
    assert!(serde_json::from_str::<ProductRaw>(doc).unwrap().codes.is_some());
}

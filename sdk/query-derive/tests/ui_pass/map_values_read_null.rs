//! A `map` over `jsonb` keeps a key whose value is `null` (issue #294), so a
//! map with `Option` values reads it, written at the field, wrapped in a
//! `FlussoMap` newtype or named fields, or in a fragment newtype.
use std::collections::{BTreeMap, HashMap};

use flusso_query::{FlussoFragment, FlussoMap, FlussoRoot};

#[derive(Debug, serde::Deserialize, FlussoMap)]
#[flusso(keyword)]
struct Codes(HashMap<String, Option<String>>);

#[derive(Debug, serde::Deserialize, FlussoMap)]
#[flusso(text)]
struct Title {
    it: Option<String>,
    #[serde(flatten)]
    rest: BTreeMap<String, Option<String>>,
}

#[derive(Debug, serde::Deserialize, FlussoFragment)]
struct Prices(HashMap<String, Option<f64>>);

#[derive(Debug, serde::Deserialize, FlussoRoot)]
#[flusso(index = "products")]
struct Product {
    sku: String,
    title: Title,
    codes: Option<Codes>,
    prices: Option<Prices>,
    #[serde(rename = "releaseDates")]
    release_dates: Option<HashMap<String, Option<String>>>,
}

#[derive(Debug, serde::Deserialize, FlussoFragment)]
struct Listing {
    title: Title,
    codes: Option<HashMap<String, Option<String>>>,
}

#[derive(Debug, serde::Deserialize, FlussoRoot)]
#[flusso(index = "products")]
struct ProductListing {
    sku: String,
    #[serde(flatten)]
    listing: Listing,
}

fn main() {
    let doc = r#"{"sku":"a","title":{"it":null,"en":null},"codes":{"x":null},
        "prices":{"eur":null},"releaseDates":{"it":null}}"#;
    let product: Product = serde_json::from_str(doc).unwrap();
    assert_eq!(product.title.it, None);
    assert_eq!(product.title.rest.get("en"), Some(&None));
    assert_eq!(product.codes.unwrap().0.get("x"), Some(&None));
    let listing: ProductListing = serde_json::from_str(doc).unwrap();
    assert_eq!(listing.listing.codes.unwrap().get("x"), Some(&None));
}

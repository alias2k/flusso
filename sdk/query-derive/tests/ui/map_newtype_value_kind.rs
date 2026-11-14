// A `FlussoMap` newtype's value type must be a value of its tag: `i64` is no
// `keyword`, so the first document with a string value would fail to read
// (issue #294). A plain `HashMap<String, i64>` at the field is refused too.
use std::collections::HashMap;

use flusso_query::{FlussoMap, FlussoRoot};

#[derive(serde::Deserialize, FlussoMap)]
#[flusso(keyword)]
struct Codes(HashMap<String, i64>);

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "products")]
struct Product {
    sku: String,
    codes: Option<Codes>,
}

fn main() {}

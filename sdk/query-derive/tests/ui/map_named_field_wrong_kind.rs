// A named field of a `FlussoMap` struct holds one of the map's values, so it is
// a value of the tag: `i64` is no `text`, and `{"it": "x"}` fails to read
// (issue #294).
use flusso_query::{FlussoMap, FlussoRoot};

#[derive(serde::Deserialize, FlussoMap)]
#[flusso(text)]
struct Title {
    it: Option<i64>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "products")]
struct Product {
    sku: String,
    title: Title,
}

fn main() {}

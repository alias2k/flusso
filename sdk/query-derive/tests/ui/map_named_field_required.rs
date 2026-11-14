// A map may lack any key, so a named field of a `FlussoMap` struct reads
// `Option<…>`: `it: String` fails on the first map without `it` (issue #294).
use flusso_query::{FlussoMap, FlussoRoot};

#[derive(serde::Deserialize, FlussoMap)]
#[flusso(text)]
struct Title {
    it: String,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "products")]
struct Product {
    sku: String,
    title: Title,
}

fn main() {}

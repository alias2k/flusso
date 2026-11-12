// A `map` field gets a map handle, not a namespace of its own, so `scope` on
// it would rename nothing.
use std::collections::HashMap;

use flusso_query::FlussoRoot;

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "products")]
struct Product {
    sku: String,
    #[flusso(scope = "Titles")]
    title: HashMap<String, String>,
}

fn main() {}

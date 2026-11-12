// A `HashMap` is checked as a `map` by its value type, with no sub-shape for
// `opaque` to opt out of.
use std::collections::HashMap;

use flusso_query::FlussoRoot;

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "products")]
struct Product {
    sku: String,
    #[flusso(opaque)]
    title: HashMap<String, String>,
}

fn main() {}

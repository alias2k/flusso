use flusso_query::FlussoRoot;
use std::collections::HashMap;

// `codes` is a `keyword` map whose keys carry subfields. Across keys, `codes.*`
// also matches every key's `.text` and `.keyword_lowercase` subfields, so an
// exact `eq` has no correct rendering and doesn't exist: use
// `codes().keyword_lowercase().eq(..)` or `codes().key(k).eq(..)`.
#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "products")]
struct Product {
    sku: String,
    codes: Option<HashMap<String, String>>,
}

fn main() {
    let _ = Product::codes().eq("0049");
}

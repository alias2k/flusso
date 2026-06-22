use flusso_query::FlussoDocument;
use std::collections::HashMap;

// `title` is a `text` map in the schema — its values must be `String`, not an
// integer. The map value type is hard-checked just like a scalar leaf.
#[derive(serde::Deserialize, FlussoDocument)]
#[flusso(index = "products")]
struct Product {
    sku: String,
    title: HashMap<String, i32>,
}

fn main() {}

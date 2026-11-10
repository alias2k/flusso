use flusso_query::FlussoRoot;
use std::collections::HashMap;

// Resolved against `no_subfields.toml`. Across keys, a `text` map mirrors a plain
// `text` field: without subfields there is no `.keyword` to run `any_of` on and
// no `.keyword_lowercase` view, exactly as on a plain field.
#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "products")]
struct Product {
    sku: String,
    title: HashMap<String, String>,
}

fn main() {
    let _ = Product::title().any_of(["a"]);
    let _ = Product::title().keyword_lowercase();
}

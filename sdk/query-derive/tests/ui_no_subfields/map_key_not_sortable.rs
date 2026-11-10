use flusso_query::{FlussoRoot, SortBuilder, SortOrder};
use std::collections::HashMap;

// Resolved against `no_subfields.toml`. `title` is a `text` map, and a map key
// is mapped like a plain field of its value kind: with subfields off it has no
// `.keyword_lowercase`, so — like a plain `text` field — it is not `Sortable`.
// With subfields on, `title().key("it").asc()` compiles and sorts on it.
#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "products")]
struct Product {
    sku: String,
    title: HashMap<String, String>,
}

fn main() {
    let _ = SortBuilder::new()
        .by(Product::title().key("it"), SortOrder::Asc)
        .build();
}

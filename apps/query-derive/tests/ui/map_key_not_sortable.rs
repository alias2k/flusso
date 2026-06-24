use flusso_query::{FlussoDocument, SortBuilder, SortOrder};
use std::collections::HashMap;

// `title` is a `text` map. `title().key("it")` is a `MapKey`-marked leaf, which
// is intentionally NOT `Sortable`: a plain `.asc()` would sort on a nonexistent
// `.keyword_lowercase` subfield and fail at query time. Sort a map by key with
// `TextMap::sort_by` / `SortBuilder::by_map_key` instead.
#[derive(serde::Deserialize, FlussoDocument)]
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

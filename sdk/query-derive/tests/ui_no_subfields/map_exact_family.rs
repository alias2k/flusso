use flusso_query::{AsQuery, FlussoRoot};
use std::collections::HashMap;

// Resolved against `no_subfields.toml`: `codes` keys have no subfields, so the
// exact family runs over the keys themselves (`codes.*`).
#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "products")]
struct Product {
    sku: String,
    codes: Option<HashMap<String, String>>,
}

fn main() {
    let codes = Product::codes();
    let queries = [
        codes.eq("0049").prefer("ean", 2.0).boost(1.5).name("eq").to_value(),
        codes.any_of(["0049", "0050"]).only_preferred().prefer("ean", 1.0).to_value(),
        codes.prefix("00").rewrite("constant_score").to_value(),
        codes.wildcard("00?9").to_value(),
        codes.regexp("00[0-9]+").max_determinized_states(100).to_value(),
        codes.fuzzy("0048").to_value(),
    ];
    for query in &queries[2..] {
        assert_eq!(query["query_string"]["fields"][0], "codes.*", "{query}");
    }
}

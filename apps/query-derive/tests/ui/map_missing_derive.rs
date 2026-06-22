use flusso_query::FlussoDocument;
use std::collections::HashMap;

// `title` is a `text` map. A whole-map newtype wrapper is allowed *only* if it
// implements `FlussoMap<kind::Text>` (via `#[derive(FlussoMap)]`). This one
// doesn't, so the deferred bound fails to resolve.
#[derive(serde::Deserialize)]
struct Translations(HashMap<String, String>);

#[derive(serde::Deserialize, FlussoDocument)]
#[flusso(index = "products")]
struct Product {
    sku: String,
    title: Translations,
}

fn main() {}

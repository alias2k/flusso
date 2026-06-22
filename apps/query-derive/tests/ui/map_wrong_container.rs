use flusso_query::FlussoDocument;

// `title` is a `text` map in the schema — a bare `String` is not a map. The
// deferred `FlussoMap<Text>` bound fails (a non-map type can't satisfy it).
#[derive(serde::Deserialize, FlussoDocument)]
#[flusso(index = "products")]
struct Product {
    sku: String,
    title: String,
}

fn main() {}

use flusso_query::FlussoMultiDocument;

// A variant payload must implement `FlussoDocument` (usually via its derive) —
// the union's generated code reads the payload's `INDEX`/`SCHEMA_HASH`.
#[derive(FlussoMultiDocument)]
enum SearchItem {
    Text(String),
}

fn main() {}

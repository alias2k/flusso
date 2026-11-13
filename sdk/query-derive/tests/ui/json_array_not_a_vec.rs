use flusso_query::FlussoRoot;

// A `json` array column reads an array, not a single value.

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "lists")]
struct Order {
    id: String,
    notes: Option<String>,
}

fn main() {}

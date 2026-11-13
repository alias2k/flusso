use flusso_query::{FlussoRoot, FlussoValue};

// A newtype over a `Vec` whose elements are not of the array field's kind.

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
struct Numbers(Vec<i64>);

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "lists")]
struct Order {
    id: String,
    labels: Numbers,
}

fn main() {}

use flusso_query::{FlussoRoot, FlussoValue};

// A newtype over a `Vec` is no single value: it fits only an array field.

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
struct Ids(Vec<String>);

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "lists")]
struct Order {
    id: Ids,
}

fn main() {}

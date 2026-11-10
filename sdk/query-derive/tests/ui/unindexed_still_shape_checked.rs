use flusso_query::{FlussoFragment, FlussoRoot};

// `variants` is `indexed: false`: it has no handle, but its stored shape is still
// checked, so a fragment that can't hold it is a compile error.
#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "stored")]
struct Listing {
    id: String,
    variants: Vec<Variant>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Variant {
    color: i64,
}

fn main() {}

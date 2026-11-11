// `variants` is a `has_many` with `indexed: false`: it has no handle, so no
// query namespace for `scope` to rename.
use flusso_query::{FlussoFragment, FlussoRoot};

#[derive(serde::Deserialize, FlussoFragment)]
struct Variant {
    color: String,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "stored")]
struct Listing {
    id: String,
    #[flusso(scope = "Variants")]
    variants: Vec<Variant>,
}

fn main() {}

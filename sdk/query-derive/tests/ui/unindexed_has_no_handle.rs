use flusso_query::FlussoRoot;

// `variants` is a `has_many` with `indexed: false` and `snapshot` a `json` with
// `indexed: false`: nothing under them is in the index, so the derive emits no
// handle for either — querying them is a compile error, not an empty result.
#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "stored")]
struct Listing {
    id: String,
}

fn main() {
    let _ = Listing::variants();
    let _ = Listing::snapshot();
}

// `serde_json::Value` is never checked, so `opaque` on it opts out of nothing.
use flusso_query::FlussoRoot;

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "stored")]
struct Listing {
    id: String,
    #[flusso(opaque)]
    snapshot: Option<serde_json::Value>,
}

fn main() {}

// Two schema keys that generate one Rust name are a derive error naming both
// keys, at every level (issue #262).
use flusso_query::FlussoRoot;

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "colliding")]
struct Colliding {
    id: String,
}

fn main() {}

// A schema key no Rust name can spell is an error naming the key, not a panic.
use flusso_query::FlussoRoot;

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "unnamable")]
struct Unnamable {
    id: String,
}

fn main() {}

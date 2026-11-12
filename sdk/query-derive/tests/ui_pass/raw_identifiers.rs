// A field spelled as a raw identifier reads the key serde reads (`r#type` →
// `type`), and a schema key that is a Rust keyword gets a raw-identifier handle.
use flusso_query::{FlussoFragment, FlussoRoot};

#[derive(serde::Deserialize, FlussoFragment)]
struct Match {
    r#loop: String,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "reserved")]
struct Thing {
    id: String,
    r#type: String,
    r#match: Match,
}

fn main() {
    let _ = Thing::r#type().eq("a");
    let _ = Thing::r#match().r#loop().eq("b");
}

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
    r#gen: String,
    r#try: String,
    #[serde(rename = "Self")]
    self_: String,
}

fn main() {
    let _ = Thing::r#type().eq("a");
    let _ = Thing::r#match().r#loop().eq("b");
    let _ = Thing::r#gen().eq("c");
    let _ = Thing::r#try().eq("d");
    let _ = Thing::self_().eq("e");
}

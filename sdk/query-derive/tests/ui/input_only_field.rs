// An input-only (`emit: false`) field is fetched for scripts and stripped: it is
// not in the index, so projecting it is the unknown-field error.
use flusso_query::FlussoRoot;

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "computed")]
struct Person {
    id: String,
    first: String,
}

fn main() {}

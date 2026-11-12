// Every `#[flusso(…)]` on a fragment is read, not only the first: a location
// in the second one is still rejected.
use flusso_query::FlussoFragment;

#[derive(serde::Deserialize, FlussoFragment)]
#[flusso()]
#[flusso(index = "users")]
struct Address {
    city: String,
}

fn main() {}

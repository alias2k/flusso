// serde reads a `#[serde(transparent)]` struct as its one field; with two, the
// derive cannot tell which one serde reads.
use flusso_query::FlussoFragment;

#[derive(serde::Deserialize, FlussoFragment)]
#[serde(transparent)]
struct Billing {
    city: String,
    #[serde(skip)]
    cached: Option<String>,
}

fn main() {}

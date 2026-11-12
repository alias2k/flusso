// serde passes over a skipped field of a `#[serde(transparent)]` struct, so the
// derive never checks it and a `#[flusso(…)]` there is never read.
use flusso_query::FlussoFragment;

#[derive(serde::Deserialize, FlussoFragment)]
#[serde(transparent)]
struct Billing {
    city: String,
    #[serde(skip)]
    #[flusso(skip)]
    cached: Option<String>,
}

fn main() {}

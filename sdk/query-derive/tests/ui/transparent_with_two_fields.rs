// serde reads a `#[serde(transparent)]` struct as its one field it does not
// skip; with two, serde rejects it, and so does the derive.
use flusso_query::FlussoFragment;

#[derive(serde::Deserialize, FlussoFragment)]
#[serde(transparent)]
struct Billing {
    city: String,
    #[serde(skip_serializing)]
    zip: Option<String>,
}

fn main() {}

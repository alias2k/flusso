// A fragment is location-free by definition: naming an index or path would make
// it a second type that references the schema.
use flusso_query::FlussoFragment;

#[derive(serde::Deserialize, FlussoFragment)]
#[flusso(index = "users", path = "account")]
struct Account {
    tier: String,
}

fn main() {}

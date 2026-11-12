// A fragment cannot see the schema, so it takes `Date` for the built-in leaf by
// its name; the message says to rename the type.
use flusso_query::{FlussoFragment, FlussoRoot};

#[derive(serde::Deserialize, FlussoFragment)]
struct Date {
    city: String,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Holder {
    #[serde(rename = "billingAddress")]
    billing: Date,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    #[serde(flatten)]
    holder: Holder,
}

fn main() {}

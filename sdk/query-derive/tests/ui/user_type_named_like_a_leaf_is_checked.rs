// A built-in leaf is recognized by its name, except where one could never sit:
// a root's `object`/`nested` field, and a flattened field. There a type of your
// own named `Date` is checked like any other, so a plain one needs a derive.
use flusso_query::{FlussoFragment, FlussoRoot};

#[derive(serde::Deserialize)]
struct Date {
    city: String,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    #[serde(rename = "billingAddress")]
    billing: Date,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Order {
    #[serde(flatten)]
    rest: Date,
}

fn main() {}

// A fragment newtype over a `HashMap` reads one object of same-kind values, so
// it holds a `map` (or `json`) field, never an `object` with fields of its own:
// `billingAddress` has fields of other kinds, and a document fails to read
// (issue #294). The root refuses it, as it refuses the `HashMap` itself.
use std::collections::HashMap;

use flusso_query::{FlussoFragment, FlussoRoot};

#[derive(serde::Deserialize, FlussoFragment)]
struct Address(HashMap<String, bool>);

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    email: String,
    #[serde(rename = "billingAddress")]
    billing_address: Address,
}

fn main() {}

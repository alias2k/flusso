// A fragment newtype over `Option<HashMap<…>>` is checked as the map it reads:
// `billingAddress` is an `object` with fields of its own (issue #294).
use std::collections::HashMap;

use flusso_query::{FlussoFragment, FlussoRoot};

#[derive(serde::Deserialize, FlussoFragment)]
struct Address(Option<HashMap<String, bool>>);

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    email: String,
    #[serde(rename = "billingAddress")]
    billing_address: Address,
}

fn main() {}

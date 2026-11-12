// A fragment of the user's own named like a built-in leaf is checked where no
// built-in leaf could sit: at a root's `object`/`nested` field, and flattened.
use flusso_query::{FlussoFragment, FlussoRoot};

#[derive(serde::Deserialize, FlussoFragment)]
#[serde(rename_all = "camelCase")]
struct Date {
    city: String,
    postal_code: Option<String>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Uuid {
    email: String,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    #[serde(rename = "billingAddress")]
    billing: Date,
    #[serde(rename = "shippingAddress")]
    shipping: Date,
    #[serde(flatten)]
    contact: Uuid,
}

fn main() {}

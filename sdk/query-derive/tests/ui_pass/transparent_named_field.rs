// A `#[serde(transparent)]` struct with one named field is checked as that
// field's type, like a tuple newtype: the field's name is no document key.
use flusso_query::{FlussoFragment, FlussoRoot};

#[derive(serde::Deserialize, FlussoFragment)]
#[serde(rename_all = "camelCase")]
struct Address {
    city: String,
    postal_code: Option<String>,
}

#[derive(serde::Deserialize, FlussoFragment)]
#[serde(transparent)]
struct Billing {
    address: Address,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    #[serde(rename = "billingAddress")]
    billing: Billing,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct UserFields {
    id: i32,
    email: String,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
#[serde(transparent)]
struct UserView {
    fields: UserFields,
}

fn main() {
    let _ = UserView::email().eq("ada@x.com");
}

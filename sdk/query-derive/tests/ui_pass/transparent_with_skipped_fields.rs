// serde reads a `#[serde(transparent)]` struct as its one field it does not
// skip, so the derives do too (issue #262).
use std::marker::PhantomData;

use flusso_query::{FlussoFragment, FlussoRoot, FlussoValue};

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
    #[serde(skip)]
    cached: Option<String>,
    #[serde(skip_deserializing)]
    seen: bool,
}

#[derive(serde::Deserialize, FlussoFragment)]
#[serde(transparent)]
struct Shipping(#[serde(skip)] u8, Address, PhantomData<u8>);

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[serde(transparent)]
struct Email {
    value: String,
    #[serde(skip)]
    verified: bool,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct UserFields {
    id: i32,
    email: Email,
    #[serde(rename = "billingAddress")]
    billing: Billing,
    #[serde(rename = "shippingAddress")]
    shipping: Shipping,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
#[serde(transparent)]
struct UserView {
    #[serde(skip)]
    loaded: bool,
    fields: UserFields,
}

fn main() {
    let _ = UserView::email().eq(Email {
        value: "ada@x.com".into(),
        verified: false,
    });
}

//! The collision that motivated the module: a fragment named after the level it
//! sits at. Under the old flat naming this was `E0428` plus a `Copy` cascade
//! landing on the user's own struct.
#![allow(dead_code, unused_crate_dependencies)]
use flusso_query::{FlussoFragment, FlussoRoot};

#[derive(serde::Deserialize, FlussoFragment)]
struct UserOrders {
    status: String,
}

// A second one, colliding with what the *object* level would have generated.
#[derive(serde::Deserialize, FlussoFragment)]
#[serde(rename_all = "camelCase")]
struct UserBillingAddress {
    city: String,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users", config = "tests/fixtures/flusso.toml")]
struct User {
    id: i32,
    #[serde(rename = "billingAddress")]
    billing_address: UserBillingAddress,
    orders: Vec<UserOrders>,
}

#[test]
fn a_user_type_named_after_a_level_no_longer_collides() {
    use flusso_query::FlussoRoot as _;
    // Both the user's `UserOrders` and the generated `user_scope::Orders` exist.
    let body = User::query()
        .filter(User::billing_address().city().eq("Rome"))
        .filter(User::orders().any(user_scope::Orders::status().eq("paid")))
        .body();
    assert_eq!(
        body["query"]["bool"]["filter"][0]["term"]["billingAddress.city"],
        "Rome"
    );
}

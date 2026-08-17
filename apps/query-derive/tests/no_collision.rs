//! Nothing the derive generates may land in the caller's namespace.
//!
//! Before the scope module, `#[derive(FlussoRoot)]` emitted its scope types
//! beside the root as `UserOrders`, `UserAccount`, … — so a user type with one
//! of those names was a redefinition. Worse, the derive's own
//! `#[derive(Copy)]` then attached to *their* struct, burying the real `E0428`
//! under a cascade about `String` not being `Copy`.
//!
//! Every name the old scheme would have produced is claimed here by a user type
//! first. If a future change reintroduces a flat name, this file stops
//! compiling — which is the point.
#![allow(dead_code, unused_crate_dependencies)]

use flusso_query::{FlussoFragment, FlussoRoot};

// ── the names the old flat scheme generated, all taken by the user ──────────

/// What `users.orders` used to generate.
#[derive(serde::Deserialize, FlussoFragment)]
struct UserOrders {
    status: String,
}

/// What `users.orders.items`'s deeper level used to generate — the name is
/// claimed even though this project never embeds it there.
#[derive(Debug)]
struct UserOrdersItems {
    whatever: String,
}

/// What the `users.billingAddress` object level used to generate.
#[derive(serde::Deserialize, FlussoFragment)]
#[serde(rename_all = "camelCase")]
struct UserBillingAddress {
    city: String,
}

/// And the plain level names, in case the scheme ever drops the root prefix.
#[derive(Debug)]
struct Orders;
#[derive(Debug)]
struct BillingAddress;

#[derive(serde::Deserialize, FlussoRoot)]
#[serde(rename_all = "camelCase")]
#[flusso(index = "users", config = "tests/fixtures/flusso.toml")]
struct User {
    id: i32,
    billing_address: UserBillingAddress,
    orders: Vec<UserOrders>,
}

#[test]
fn user_types_named_after_every_level_coexist_with_the_generated_scopes() {
    // Compiling at all is the assertion: the user's `UserOrders` / `Orders` /
    // `UserBillingAddress` and the generated `flusso_user_query::*` are different
    // types living in different namespaces.
    let body = User::query()
        .filter(User::billing_address().city().eq("Rome"))
        .filter(User::orders().any(flusso_user_query::Orders::status().eq("paid")))
        .body();
    assert_eq!(
        body["query"]["bool"]["filter"][0]["term"]["billingAddress.city"],
        "Rome"
    );
    assert_eq!(
        body["query"]["bool"]["filter"][1]["nested"]["path"],
        "orders"
    );
}

#[test]
fn the_users_own_type_and_the_generated_scope_are_distinct() {
    // Same spelling in the source, different types — the user's is a document
    // shape, the generated one is a query scope.
    let mine = UserOrders {
        status: "paid".into(),
    };
    assert_eq!(mine.status, "paid");
    let generated = flusso_user_query::Orders;
    assert_eq!(format!("{generated:?}"), "Orders");
}

// ── two roots over the same index, in one module ────────────────────────────
//
// Each gets its own scope module, so their levels can't tread on each other.

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users", config = "tests/fixtures/flusso.toml")]
struct Customer {
    id: i32,
    orders: Vec<UserOrders>,
}

#[test]
fn two_roots_over_one_index_get_separate_scope_modules() {
    let a = User::query()
        .filter(User::orders().any(flusso_user_query::Orders::status().eq("paid")))
        .body();
    let b = Customer::query()
        .filter(Customer::orders().any(flusso_customer_query::Orders::status().eq("paid")))
        .body();
    assert_eq!(a["query"], b["query"]);
}

// ── the module name itself can be renamed ───────────────────────────────────
//
// The one thing that *can* still clash is the module, if the caller already has
// one by that name. `scope_mod` is the escape.

mod flusso_invoice_query {
    /// Claims the name the derive would otherwise generate for `Invoice`.
    #[derive(Debug)]
    pub(crate) struct AlreadyMine;
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(
    index = "users",
    scope_mod = "invoice_queries",
    config = "tests/fixtures/flusso.toml"
)]
struct Invoice {
    id: i32,
}

#[test]
fn scope_mod_escapes_a_module_name_the_caller_already_uses() {
    let _ = flusso_invoice_query::AlreadyMine;
    let body = Invoice::query()
        .filter(Invoice::orders().any(invoice_queries::Orders::status().eq("paid")))
        .body();
    assert_eq!(
        body["query"]["bool"]["filter"][0]["nested"]["path"],
        "orders"
    );
}

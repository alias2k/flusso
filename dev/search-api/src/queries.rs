//! Tests over the dev example's query surface.
//!
//! There is no database here and no OpenSearch: every check builds a request
//! body and asserts its shape. That is enough to catch the things that actually
//! break — a handle that moved, a scope that stopped lifting, a shared fragment
//! that drifted from one of its two schemas — because the derive resolves
//! `dev/flusso.toml` at **compile time**. A schema change that breaks these
//! structs fails `cargo test` before an assertion ever runs.

use flusso_query::{FlussoRoot, Sortable};

use crate::orders::Order;
use crate::products::Product;
use crate::users::User;
use crate::users::flusso_user_query::{Addresses, Orders};

/// The `users` root reaches every level of its index, whether or not the
/// projection deserializes it.
#[test]
fn the_root_owns_the_whole_surface() {
    let body = User::query()
        .filter(User::email().eq("ada@example.com"))
        // An object flattens, so its scope chains from the root.
        .filter(User::account().tier().eq("pro"))
        // A nested array has its own scope, lifted by `any`.
        .filter(User::orders().any(Orders::status().eq("paid")))
        // Never projected by `User` — still queryable.
        .filter(User::addresses().any(Addresses::city().eq("Boston")))
        .body();

    let filters = &body["query"]["bool"]["filter"];
    assert_eq!(filters[0]["term"]["email"], "ada@example.com");
    assert_eq!(filters[1]["term"]["account.tier"], "pro");
    assert_eq!(filters[2]["nested"]["path"], "orders");
    assert_eq!(filters[3]["nested"]["path"], "addresses");
}

/// A nested field sorts through its scope's `PATH`, so the `nested` wrapper is
/// derived rather than hand-written.
#[test]
fn a_nested_sort_carries_its_boundary() {
    let body = User::query().sorts([Orders::placed_at().desc()]).body();
    let sort = &body["sort"][0]["orders.placedAt"];
    assert_eq!(sort["order"], "desc");
    assert_eq!(sort["nested"]["path"], "orders");
}

/// The generated scope module never enters this module's namespace, so a type
/// of ours may share a generated name. `UserOrders` here is *ours*.
#[derive(Debug)]
struct UserOrders;

#[test]
fn a_local_type_named_after_a_level_is_fine() {
    let _ = UserOrders;
    // …and the generated scope is a different type entirely.
    let body = User::query()
        .filter(User::orders().any(Orders::total().gte(10)))
        .body();
    assert_eq!(
        body["query"]["bool"]["filter"][0]["nested"]["path"],
        "orders"
    );
}

/// `LineItem` is one fragment embedded in **two** indexes. Both roots resolve
/// their own schema at compile time, so this test existing at all proves the
/// shape fits `users.orders.items` *and* `orders.items`.
#[test]
fn one_fragment_serves_both_indexes() {
    use crate::orders::flusso_order_query::Items;

    let from_orders = Order::query()
        .filter(Order::items().any(Items::quantity().gte(2)))
        .body();
    assert_eq!(
        from_orders["query"]["bool"]["filter"][0]["nested"]["path"],
        "items"
    );

    // The same shape, three levels down in a different index.
    let from_users = User::query()
        .filter(User::orders().any(
            Orders::items().any(crate::users::flusso_user_query::OrdersItems::quantity().gte(2)),
        ))
        .body();
    assert_eq!(
        from_users["query"]["bool"]["filter"][0]["nested"]["path"],
        "orders"
    );
}

/// The shared `OrderStatus` enum is checked against each schema's declared
/// `variants:`; querying with it proves it is a keyword value at both sites.
#[test]
fn the_shared_status_enum_is_a_query_value_in_both_indexes() {
    use crate::shared::OrderStatus;

    let orders = Order::query()
        .filter(Order::status().eq(OrderStatus::Delivered))
        .body();
    assert_eq!(
        orders["query"]["bool"]["filter"][0]["term"]["status"],
        "delivered"
    );

    let users = User::query()
        .filter(User::orders().any(Orders::status().eq(OrderStatus::Delivered)))
        .body();
    assert_eq!(
        users["query"]["bool"]["filter"][0]["nested"]["path"],
        "orders"
    );
}

/// Each root addresses its own index, and the physical name carries the schema
/// hash the sink writes — so a binding and an index are provably the same
/// schema version.
#[test]
fn every_root_binds_to_its_own_index() {
    assert_eq!(User::INDEX, "users");
    assert_eq!(Order::INDEX, "orders");
    assert_eq!(Product::INDEX, "products");
    assert!(User::physical_index().starts_with("users_"));
    assert_ne!(User::SCHEMA_HASH, "");
}

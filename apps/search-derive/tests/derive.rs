//! End-to-end test of `#[derive(FlussoDocument)]`: a hand-written struct +
//! `flusso.toml` fixture → a generated query surface that builds real requests.
#![allow(dead_code, unused_crate_dependencies)]

use flusso_search::{FlussoDocument, FlussoValue, GeoPoint};

type Result = std::result::Result<(), Box<dyn std::error::Error>>;

#[derive(serde::Deserialize, FlussoDocument)]
#[flusso(index = "users", config = "tests/fixtures/flusso.toml")]
struct User {
    id: i32,
    email: String,
    #[flusso(rename = "fullName")]
    full_name: Option<String>,
    orders: Vec<Order>,
    #[flusso(rename = "orderCount")]
    order_count: i64,
    // `location` (geo) and the orders' inner fields aren't projected here —
    // partial projections are allowed, and their handles still generate.
}

#[derive(serde::Deserialize, FlussoDocument)]
#[flusso(
    index = "users",
    path = "orders",
    config = "tests/fixtures/flusso.toml"
)]
struct Order {
    status: String,
    total: f64,
}

#[test]
fn generated_surface_builds_queries() -> Result {
    let body = User::query()
        .filter(User::email().eq("ada@example.com")) // keyword handle
        .filter(User::order_count().gte(5)) // count → Number<i64>
        .query(User::full_name().matches("ada")) // text (renamed fullName)
        .filter(User::orders().any(Order::status().eq("paid"))) // nested + child handle
        .filter(User::location().within("10km", GeoPoint::new(52.37, 4.90))) // geo, not projected
        .body();

    assert!(body.is_object());
    assert!(!User::SCHEMA_HASH.is_empty());
    // The index const is the physical name: logical + the hash, used by search/get.
    assert_eq!(User::INDEX, "users");

    // Spot-check the emitted DSL (compact JSON, no indexing into Value).
    let json = body.to_string();
    assert!(json.contains(r#""fullName""#), "{json}");
    assert!(json.contains(r#""orders.status""#), "{json}");
    assert!(json.contains(r#""geo_distance""#), "{json}");
    assert!(json.contains(r#""orderCount""#), "{json}");

    Ok(())
}

// `#[derive(FlussoValue)]` lets a field be a Rust enum or newtype wrapper
// instead of a bare leaf type: the derive impls `FlussoValue<K>` for the chosen
// kind, which `FlussoDocument` defers to. Works across kinds — `keyword` here,
// plus a `number` newtype on the orders' decimal `total`.

/// A newtype wrapper over the `email` keyword (kind defaults to `keyword`).
#[derive(serde::Deserialize, FlussoValue)]
struct Email(String);

/// A newtype over the analyzed `fullName` text field — the `text` kind.
#[derive(serde::Deserialize, FlussoValue)]
#[flusso(text)]
struct Headline(String);

/// A unit enum over the orders' `status` (an `enum` mapping → keyword).
/// `Serialize` lets it be passed *as a query value* (`.eq(OrderStatus::Paid)`).
#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[serde(rename_all = "camelCase")]
#[flusso(keyword)]
enum OrderStatus {
    Paid,
    Pending,
    Cancelled,
}

/// A numeric newtype over the orders' decimal `total` — the `number` kind.
#[derive(serde::Deserialize, FlussoValue)]
#[flusso(number)]
struct Money(f64);

#[derive(serde::Deserialize, FlussoDocument)]
#[flusso(index = "users", config = "tests/fixtures/flusso.toml")]
struct TypedUser {
    email: Email,
    #[flusso(rename = "fullName")]
    full_name: Option<Headline>,
    orders: Vec<TypedOrder>,
}

#[derive(serde::Deserialize, FlussoDocument)]
#[flusso(
    index = "users",
    path = "orders",
    config = "tests/fixtures/flusso.toml"
)]
struct TypedOrder {
    status: OrderStatus,
    total: Money,
}

#[test]
fn value_derive_accepts_enums_and_newtypes() -> Result {
    // The struct compiled at all → the deferred `FlussoValue<K>` bounds held
    // (keyword `email`/`status`, number `total`). Keyword operators also accept
    // the typed value directly, matched against its serde string form.
    let body = TypedUser::query()
        .filter(TypedUser::email().eq("ada@example.com")) // &str still works
        .filter(TypedUser::orders().any(TypedOrder::status().eq(OrderStatus::Paid)))
        .body();

    let json = body.to_string();
    assert!(json.contains(r#""orders.status""#), "{json}");
    // The enum serialized to its `rename_all = "camelCase"` form, not "Paid".
    assert!(json.contains(r#""paid""#), "{json}");
    Ok(())
}

// `#[derive(FlussoMultiDocument)]` — the combined-search union over two
// document types from two indexes. Purely syntactic: the generated impl
// references each payload's derive-baked `INDEX`/`SCHEMA_HASH`.

#[derive(serde::Deserialize, FlussoDocument)]
#[flusso(index = "products", config = "tests/fixtures/flusso.toml")]
struct Product {
    sku: String,
    name: Option<String>,
}

#[derive(flusso_search::FlussoMultiDocument)]
enum SearchItem {
    User(User),
    Product(Product),
}

#[test]
fn multi_document_derive_lists_targets_and_dispatches_hits() -> Result {
    use flusso_search::FlussoMultiDocument as _;

    // TARGETS: one (logical index, schema hash) per variant, in order.
    assert_eq!(
        SearchItem::TARGETS,
        [
            ("users", User::SCHEMA_HASH),
            ("products", Product::SCHEMA_HASH),
        ]
    );

    // A hit decodes into the variant matching its physical index.
    let hit = SearchItem::decode(
        &User::physical_index(),
        serde_json::json!({
            "id": 1, "email": "ada@example.com",
            "full_name": null, "orders": [], "order_count": 0
        }),
    )?;
    assert!(matches!(hit, SearchItem::User(_)));

    let hit = SearchItem::decode(
        &Product::physical_index(),
        serde_json::json!({ "sku": "C-01234", "name": "keyboard" }),
    )?;
    match hit {
        SearchItem::Product(product) => assert_eq!(product.sku, "C-01234"),
        SearchItem::User(_) => return Err("expected a product hit".into()),
    }

    // A hit from an index no variant claims is an error, not a skip.
    match SearchItem::decode("ghosts_zzzzzz", serde_json::json!({})) {
        Err(flusso_search::Error::UnexpectedIndex { index }) => {
            assert_eq!(index, "ghosts_zzzzzz");
        }
        Err(other) => return Err(format!("wrong error: {other}").into()),
        Ok(_) => return Err("expected an unexpected-index error".into()),
    }
    Ok(())
}

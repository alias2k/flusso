//! End-to-end test of `#[derive(FlussoDocument)]`: a hand-written struct +
//! `flusso.toml` fixture → a generated query surface that builds real requests.
#![allow(dead_code, unused_crate_dependencies)]

use flusso_search::{Client, FlussoDocument, GeoPoint};

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
    let client = Client::connect("http://localhost:9200")?;

    let body = User::search(&client)
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

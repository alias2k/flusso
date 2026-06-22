//! End-to-end test of `#[derive(FlussoDocument)]`: a hand-written struct +
//! `flusso.toml` fixture → a generated query surface that builds real requests.
#![allow(dead_code, unused_crate_dependencies)]

use std::collections::HashMap;

use flusso_query::{AsQuery, FlussoDocument, FlussoMap, FlussoValue, Fuzziness, GeoPoint};

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

// Issue #19 acceptance test: a realistic projection — weighted + fuzzy +
// case-insensitive-wildcard free-text with `minimum_should_match: 1`, exact
// filters on a `Uuid` and an enum keyword, exact/wildcard/full-text on the
// right subfield, and `created_at desc` with `missing: _first` — written with
// ZERO `Search::raw` / `Json::raw` and ZERO `#[flusso(skip)]` on the `Uuid`.

/// A keyword enum field (`tier`), passed as a query value via its serde form.
#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[serde(rename_all = "camelCase")]
#[flusso(keyword)]
enum CustomerTier {
    Pro,
    Free,
}

#[derive(serde::Deserialize, FlussoDocument)]
#[flusso(index = "users", config = "tests/fixtures/flusso.toml")]
struct Customer {
    email: String,
    #[flusso(rename = "fullName")]
    full_name: Option<String>,
    // A `Uuid` keyword field — no `#[flusso(skip)]`, no `Keyword::at("ownerId")`.
    #[flusso(rename = "ownerId")]
    owner_id: flusso_query::uuid::Uuid,
    tier: CustomerTier,
    #[flusso(rename = "createdAt")]
    created_at: Option<String>,
}

#[test]
fn acceptance_realistic_projection_needs_no_escape_hatch() -> Result {
    let owner = flusso_query::uuid::Uuid::nil();
    let body = Customer::query()
        // Weighted + fuzzy + case-insensitive-wildcard free-text, a real
        // constraint via `minimum_should_match: 1`.
        .should(Customer::full_name().matches("acme").boost(2.0))
        .should(
            Customer::full_name()
                .keyword()
                .wildcard("*acme*")
                .case_insensitive(),
        )
        .should(
            Customer::full_name()
                .matches("acme")
                .fuzziness(Fuzziness::Auto),
        )
        .min_should_match(1)
        // Exact filters on a Uuid and an enum keyword — typed, no string paths.
        .filter(Customer::owner_id().eq(owner))
        .filter(Customer::tier().eq(CustomerTier::Pro))
        // Full-text against a keyword field's `.text` subfield.
        .filter(Customer::email().text().matches("acme"))
        // Null-aware sort, no string path.
        .sort(Customer::created_at().desc().missing_first())
        .body();

    let json = body.to_string();
    assert!(json.contains(r#""minimum_should_match":1"#), "{json}");
    assert!(json.contains(r#""fullName.keyword""#), "{json}");
    assert!(json.contains(r#""case_insensitive":true"#), "{json}");
    assert!(json.contains(r#""ownerId""#), "{json}");
    assert!(
        json.contains("00000000-0000-0000-0000-000000000000"),
        "{json}"
    );
    assert!(
        json.contains(r#""tier""#) && json.contains(r#""pro""#),
        "{json}"
    );
    assert!(json.contains(r#""email.text""#), "{json}");
    assert!(json.contains(r#""missing":"_first""#), "{json}");
    Ok(())
}

// Issue #28: first-class `map` type. The `products` schema declares `title`
// (a `text` map) and `codes` (a `keyword` map). The query surface generates
// from the schema, so `Product` (which projects neither) still gets typed
// `title()`/`codes()` handles.

#[test]
fn map_field_generates_typed_query_surface() -> Result {
    // Specific key — a fully-typed `Text` leaf (zero `.raw()`, zero string path).
    let q = Product::title().key("it").matches("ciao").to_value();
    assert_eq!(q["match"]["title.it"], serde_json::json!("ciao"));

    // Cross-key search with per-key preference + presence checks.
    let body = Product::query()
        .query(
            Product::title()
                .search("ciao")
                .prefer("it", 3.0)
                .prefer("en", 2.0),
        )
        .filter(Product::title().exists())
        .filter(Product::title().has_key("it"))
        // A keyword map: exact per-key lookup, no `search`.
        .filter(Product::codes().key("ean").eq("0049"))
        .body();

    let json = body.to_string();
    assert!(json.contains(r#""title.it^3""#), "{json}");
    assert!(json.contains(r#""title.en^2""#), "{json}");
    assert!(json.contains(r#""title.*""#), "{json}");
    assert!(json.contains(r#""best_fields""#), "{json}");
    assert!(json.contains(r#""codes.ean""#), "{json}");
    Ok(())
}

/// A custom keyword/text value type usable as a map's values (`FlussoValue<Text>`).
#[derive(serde::Deserialize, FlussoValue)]
#[flusso(text)]
struct Locale(String);

/// A whole-map newtype wrapper over the `text` map (`FlussoMap<Text>`).
#[derive(serde::Deserialize, FlussoMap)]
#[flusso(text)]
struct Translations(HashMap<String, String>);

// Each of these compiling proves a `check_type` map arm: a bare `HashMap`
// (hard-checked value kind), a `HashMap` of a custom `FlussoValue`, and a
// whole-map `FlussoMap` wrapper. `codes` is nullable → `Option`.
#[derive(serde::Deserialize, FlussoDocument)]
#[flusso(index = "products", config = "tests/fixtures/flusso.toml")]
struct MappedProduct {
    sku: String,
    title: HashMap<String, String>,
    codes: Option<HashMap<String, String>>,
    prices: Option<HashMap<String, f64>>,
    #[flusso(rename = "releaseDates")]
    release_dates: Option<HashMap<String, String>>,
}

#[derive(serde::Deserialize, FlussoDocument)]
#[flusso(index = "products", config = "tests/fixtures/flusso.toml")]
struct CustomValueProduct {
    sku: String,
    title: HashMap<String, Locale>,
}

#[derive(serde::Deserialize, FlussoDocument)]
#[flusso(index = "products", config = "tests/fixtures/flusso.toml")]
struct WrappedProduct {
    sku: String,
    title: Translations,
}

#[test]
fn number_and_date_maps_generate_typed_leaves() -> Result {
    // `prices` is a `double` map → `NumberMap<f64>`; `.key()` is a `Number<f64>`
    // leaf with range ops (`.matches(..)` would not compile here).
    let body = Product::query()
        .filter(Product::prices().key("usd").gt(9.99))
        .filter(Product::prices().has_key("eur"))
        // `releaseDates` is a `date` map → `DateMap`; `.key()` is a `Date` leaf.
        .filter(Product::release_dates().key("eu").gte("2020-01-01"))
        .body();
    let json = body.to_string();
    assert!(json.contains(r#""prices.usd""#), "{json}");
    assert!(json.contains(r#""prices.eur""#), "{json}");
    assert!(json.contains(r#""releaseDates.eu""#), "{json}");
    Ok(())
}

#[test]
fn map_doc_types_accept_hashmap_custom_value_and_wrapper() -> Result {
    // The three structs above compiled → every deferred map bound held. The
    // generated handles still follow the schema regardless of the doc type.
    let body = MappedProduct::query()
        .query(MappedProduct::title().key("it").matches("ciao"))
        .body();
    assert!(body.to_string().contains(r#""title.it""#));
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

#[derive(flusso_query::FlussoMultiDocument)]
enum SearchItem {
    User(User),
    Product(Product),
}

#[test]
fn multi_document_derive_lists_targets_and_dispatches_hits() -> Result {
    use flusso_query::FlussoMultiDocument as _;

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
        Err(flusso_query::Error::UnexpectedIndex { index }) => {
            assert_eq!(index, "ghosts_zzzzzz");
        }
        Err(other) => return Err(format!("wrong error: {other}").into()),
        Ok(_) => return Err("expected an unexpected-index error".into()),
    }
    Ok(())
}

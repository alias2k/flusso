//! End-to-end test of `#[derive(FlussoRoot)]`: a hand-written struct +
//! `flusso.toml` fixture → a generated query surface that builds real requests.
#![allow(dead_code, unused_crate_dependencies)]

use std::collections::{BTreeMap, HashMap};

use flusso_query::{
    AsQuery, Distance, FlussoFragment, FlussoMap, FlussoRoot, FlussoValue, Fuzziness, GeoPoint,
    Sortable,
};

type Result = std::result::Result<(), Box<dyn std::error::Error>>;

#[derive(serde::Deserialize, FlussoRoot)]
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

#[derive(serde::Deserialize, FlussoFragment)]
struct Order {
    status: String,
    total: f64,
}

#[test]
fn generated_surface_builds_queries() -> Result {
    let body = User::query()
        .filter(User::email().eq("ada@example.com")) // keyword handle
        .filter(User::order_count().gte(5)) // count → Number
        .query(User::full_name().matches("ada")) // text (renamed fullName)
        .filter(User::orders().any(flusso_user_query::Orders::status().eq("paid"))) // nested + child handle
        .filter(User::location().within(Distance::km(10.0), GeoPoint::new(52.37, 4.90))) // geo, not projected
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
// kind, which `FlussoRoot` defers to. Works across kinds — `keyword` here,
// plus a `number` newtype on the orders' decimal `total`.

/// A newtype wrapper over the `email` keyword (untagged, so it inherits
/// `String`'s keyword + text kinds).
/// `FlussoValue` requires `Serialize` (so the type can be a query value).
#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
struct Email(String);

/// A newtype over the analyzed `fullName` text field — the `text` kind.
#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
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

/// A numeric newtype over the orders' decimal `total`. No kind tag — it inherits
/// `Decimal`'s kinds, so it's a `decimal` value both as a document field and as a
/// query value (`total().eq(Money(..))`).
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize, FlussoValue)]
struct Money(flusso_query::Decimal);

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users", config = "tests/fixtures/flusso.toml")]
struct TypedUser {
    email: Email,
    #[flusso(rename = "fullName")]
    full_name: Option<Headline>,
    orders: Vec<TypedOrder>,
}

#[derive(serde::Deserialize, FlussoFragment)]
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
        .filter(
            TypedUser::orders()
                .any(flusso_typed_user_query::Orders::status().eq(OrderStatus::Paid)),
        )
        .body();

    let json = body.to_string();
    assert!(json.contains(r#""orders.status""#), "{json}");
    // The enum serialized to its `rename_all = "camelCase"` form, not "Paid".
    assert!(json.contains(r#""paid""#), "{json}");
    Ok(())
}

// A declared-order `enum` (`status` has `variants: [pending, paid, shipped,
// delivered]`) gets the order-aware `Enum` handle: value ops still target the
// bare keyword, but `.asc()`/`.desc()` sort on the prebaked `.sort` subfield —
// nesting-aware, since `status` lives under the nested `orders` array.
#[test]
fn ordered_enum_sorts_by_declared_order_on_the_sort_subfield() -> Result {
    let body = TypedUser::query()
        .sort(flusso_typed_user_query::Orders::status().asc())
        .body();

    let json = body.to_string();
    assert!(json.contains(r#""orders.status.sort""#), "{json}");
    assert!(
        json.contains(r#""nested""#) && json.contains(r#""path":"orders""#),
        "{json}"
    );
    Ok(())
}

// A `decimal` field's handle (`Number<kind::Decimal>`) accepts any value of that
// kind — a `Decimal`, a losslessly-widening integer, or a `Decimal`-wrapping
// newtype — with no cast. A float would be a compile error (lossy), which is the
// whole point of the per-type split.
#[test]
fn number_handle_accepts_any_decimal_value_no_conversion() -> Result {
    use flusso_query::Decimal;

    let body = TypedUser::query()
        // `rust_decimal::Decimal` — the headline case, no `as f64`.
        .filter(
            TypedUser::orders()
                .any(flusso_typed_user_query::Orders::total().eq(Decimal::new(105_050, 2))),
        )
        // a bare integer literal widens losslessly into `decimal`.
        .filter(TypedUser::orders().any(flusso_typed_user_query::Orders::total().gte(100)))
        // and a custom newtype over `Decimal`, as a query value.
        .filter(
            TypedUser::orders()
                .any(flusso_typed_user_query::Orders::total().lt(Money(Decimal::new(500_000, 2)))),
        )
        .body();

    let json = body.to_string();
    assert!(json.contains(r#""orders.total""#), "{json}");
    assert!(json.contains("1050.5"), "{json}");
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

#[derive(serde::Deserialize, FlussoRoot)]
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
#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(text)]
struct Locale(String);

/// A whole-map newtype wrapper over the `text` map (`FlussoMap<Text>`).
#[derive(serde::Deserialize, FlussoMap)]
#[flusso(text)]
struct Translations(HashMap<String, String>);

// Each of these compiling proves a `check_type` map arm: a bare `HashMap`
// (hard-checked value kind), a `HashMap` of a custom `FlussoValue`, and a
// whole-map `FlussoMap` wrapper. `codes` is nullable → `Option`.
#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "products", config = "tests/fixtures/flusso.toml")]
struct MappedProduct {
    sku: String,
    title: HashMap<String, String>,
    codes: Option<HashMap<String, String>>,
    prices: Option<HashMap<String, f64>>,
    #[flusso(rename = "releaseDates")]
    release_dates: Option<HashMap<String, String>>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "products", config = "tests/fixtures/flusso.toml")]
struct CustomValueProduct {
    sku: String,
    title: HashMap<String, Locale>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "products", config = "tests/fixtures/flusso.toml")]
struct WrappedProduct {
    sku: String,
    title: Translations,
}

#[test]
fn number_and_date_maps_generate_typed_leaves() -> Result {
    // `prices` is a `double` map → `NumberMap`; `.key()` is a `Number`
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
fn map_fields_sort_by_key_with_language_fallback() -> Result {
    use flusso_query::{SortBuilder, SortOrder};

    // Sort by Italian title, falling back to English — a `_script` sort over the
    // dynamic `.keyword` subfields (not the broken single-key field sort), plus
    // a numeric map sort on bare keys, driven from request directions.
    let body = Product::query()
        .sorts(
            SortBuilder::new()
                .by(Product::title().sort_key("it").or("en"), SortOrder::Desc)
                .by(Product::prices().sort_key("usd"), SortOrder::Asc),
        )
        .body();

    let json = body.to_string();
    assert!(json.contains(r#""type":"string""#), "{json}");
    assert!(
        json.contains(r#"["title.it.keyword","title.en.keyword"]"#),
        "{json}"
    );
    assert!(json.contains("toLowerCase"), "{json}");
    assert!(json.contains(r#""type":"number""#), "{json}");
    assert!(json.contains(r#"["prices.usd"]"#), "{json}");
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

/// The real-world shape that pushed `FlussoMap` beyond newtypes: multilingual
/// text stored as a flat object of language keys plus a dedicated `fallback` —
/// a named-field struct, `BTreeMap`-backed. The derive (not a hand-written
/// `impl FlussoMap<K>`) is what makes it embeddable in a *fragment*: only the
/// derive emits the const metadata a fragment's check reads.
#[derive(serde::Deserialize, FlussoMap)]
#[flusso(text)]
struct Translation {
    fallback: Option<String>,
    #[serde(flatten)]
    langs: BTreeMap<String, String>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "products", config = "tests/fixtures/flusso.toml")]
struct NamedWrapperProduct {
    sku: String,
    title: Translation,
    codes: Option<BTreeMap<String, String>>,
}

#[test]
fn a_named_field_map_wrapper_and_a_btreemap_work_at_a_root() -> Result {
    // Compiling proves the deferred `FlussoMap<Text>` bound held for the
    // named-field wrapper, and that a `BTreeMap` peeled like a `HashMap`.
    let body = NamedWrapperProduct::query()
        .query(NamedWrapperProduct::title().key("it").matches("ciao"))
        .filter(NamedWrapperProduct::codes().key("ean").eq("0049"))
        .body();
    let json = body.to_string();
    assert!(json.contains(r#""title.it""#), "{json}");
    assert!(json.contains(r#""codes.ean""#), "{json}");
    Ok(())
}

// The same wrapper inside a *fragment*, embedded by a real root — the case
// that used to fail with "FlussoValueMeta is not implemented". The fragment
// cannot see the mapping, so its check rides the derive-emitted metadata:
// kind (object), map value kind (text), and the no-op recursion.

#[derive(serde::Deserialize, FlussoFragment)]
struct LocalizedProduct {
    title: Translation,
    codes: Option<BTreeMap<String, String>>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "products", config = "tests/fixtures/flusso.toml")]
struct FlattenedProduct {
    sku: String,
    #[serde(flatten)]
    localized: LocalizedProduct,
}

#[test]
fn a_map_wrapper_inside_a_fragment_is_validated_by_the_embedding_root() -> Result {
    // Compiling is the check: `title` was validated as a `text` map against
    // the real products mapping, value kind included. Handles follow the
    // schema as usual.
    let body = FlattenedProduct::query()
        .query(FlattenedProduct::title().key("it").matches("ciao"))
        .body();
    assert!(body.to_string().contains(r#""title.it""#));
    Ok(())
}

// `#[derive(FlussoMultiDocument)]` — the combined-search union over two
// document types from two indexes. Purely syntactic: the generated impl
// references each payload's derive-baked `INDEX`/`SCHEMA_HASH`.

#[derive(serde::Deserialize, FlussoRoot)]
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

// `FlussoValueMeta` is the const-readable twin of `FlussoValue<K>`: a fragment's
// check runs in const evaluation, where the schema kind is only a value, so it
// cannot name `FlussoValue<K>` — it reads these constants instead.

#[test]
fn value_derive_exposes_its_kinds_and_variants_as_constants() {
    use flusso_query::{FlussoValueMeta, KindTag};

    // An explicit kind tag → exactly that kind, and no variants.
    assert_eq!(<Headline as FlussoValueMeta>::KINDS, &[KindTag::Text]);
    assert!(<Headline as FlussoValueMeta>::VARIANTS.is_empty());

    // No kind tag on a newtype → it inherits every kind the inner type has,
    // exactly as the blanket `FlussoValue<K>` impl does. `String` is a valid
    // keyword, text, *and* date value, so `Email` is too.
    const EMAIL: &[KindTag] = <Email as FlussoValueMeta>::KINDS;
    assert_eq!(EMAIL, <String as FlussoValueMeta>::KINDS);
    assert!(EMAIL.contains(&KindTag::Keyword));

    // An enum reports its variants as the *document* spells them, so they can be
    // compared against the schema's declared `variants:` — note `rename_all`.
    assert_eq!(
        <OrderStatus as FlussoValueMeta>::VARIANTS,
        &["paid", "pending", "cancelled"]
    );

    // A newtype with no kind tag forwards its inner type's kinds as data,
    // mirroring how it forwards them as a bound.
    assert_eq!(
        <Money as FlussoValueMeta>::KINDS,
        <flusso_query::Decimal as FlussoValueMeta>::KINDS
    );
}

#[test]
fn value_metadata_is_readable_during_const_evaluation() {
    use flusso_query::{FieldSpec, FlussoValueMeta, KindTag, kind_is, variants_covered};

    const LEVEL: &[FieldSpec] = &[FieldSpec {
        name: "status",
        kind: KindTag::Keyword,
        nullable: false,
        array: false,
        variants: &["paid", "pending", "cancelled"],
        map_values: None,
        children: &[],
    }];

    // Exactly the two assertions a generated fragment check emits for a
    // custom-typed field — both resolved at compile time.
    const _: () = assert!(kind_is(
        LEVEL,
        "status",
        <OrderStatus as FlussoValueMeta>::KINDS
    ));
    const _: () = assert!(variants_covered(
        LEVEL,
        "status",
        <OrderStatus as FlussoValueMeta>::VARIANTS
    ));

    // And the no-op leaf check that lets a fragment treat every custom type alike.
    const _: () = OrderStatus::__flusso_check(&[]);
}

// Issue #100: `#[flusso(…, exhaustive)]` demands the *whole* declared variant
// set at every embedding. These enums cover their fields' sets fully, so the
// types compile — at the root (`tier`) and through a fragment (`orders.status`)
// alike. A marked enum missing a variant is a compile error (see
// tests/ui/root_exhaustive_partial.rs and fragment_exhaustive_partial.rs);
// `OrderStatus` above stays a legal partial projection because it is unmarked.

/// Covers every variant `tier` declares — allowed to demand exhaustiveness.
#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[serde(rename_all = "camelCase")]
#[flusso(keyword, exhaustive)]
enum FullTier {
    Free,
    Pro,
    Enterprise,
}

/// Covers every variant `orders.status` declares, checked via the fragment.
#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[serde(rename_all = "camelCase")]
#[flusso(keyword, exhaustive)]
enum FullOrderStatus {
    Pending,
    Paid,
    Shipped,
    Delivered,
    Cancelled,
}

/// An untagged newtype forwards its inner type's exhaustiveness with its kinds.
#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
struct WrappedTier(FullTier);

#[derive(serde::Deserialize, FlussoFragment)]
struct ExhaustiveOrder {
    status: FullOrderStatus,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users", config = "tests/fixtures/flusso.toml")]
struct ExhaustiveUser {
    tier: FullTier,
    orders: Vec<ExhaustiveOrder>,
}

#[test]
fn exhaustive_enums_covering_the_whole_declared_set_compile() -> Result {
    use flusso_query::FlussoValueMeta;

    let body = ExhaustiveUser::query()
        .filter(ExhaustiveUser::tier().eq(FullTier::Pro))
        .body();
    assert!(body.to_string().contains(r#""pro""#));

    // The marker rides `FlussoValueMeta` as const data; off by default, and an
    // untagged newtype forwards it like `KINDS`/`VARIANTS`.
    assert!(<FullTier as FlussoValueMeta>::EXHAUSTIVE);
    assert!(!<OrderStatus as FlussoValueMeta>::EXHAUSTIVE);
    assert!(<WrappedTier as FlussoValueMeta>::EXHAUSTIVE);
    Ok(())
}

// The root is the only type that resolves the schema, so it bakes the resolved
// level and drives the check into every shape it embeds. These fragments carry
// no location at all — they are validated purely by where `FragUser` puts them.

#[derive(serde::Deserialize, FlussoFragment)]
struct FragOrder {
    status: String,
    // `decimal` in the schema — OpenSearch stores it as `double`, and the
    // baked tag keeps the two apart.
    total: f64,
    // An object *inside* a nested array: the one shape the old `path =` model
    // could not express at all, because such a struct could not name its scope.
    // A `has_one` join is nullable, so the schema requires the `Option`.
    shipping: Option<FragShipping>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct FragShipping {
    carrier: String,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users", config = "tests/fixtures/flusso.toml")]
struct FragUser {
    id: i32,
    // Checked against `users.orders`, and `FragShipping` against
    // `users.orders.shipping` — reached by recursion, without either struct
    // naming that path.
    orders: Vec<FragOrder>,
}

#[test]
fn a_root_validates_the_fragments_it_embeds_against_the_real_mapping() -> Result {
    // Compiling at all means the baked level matched: `orders` resolved to a
    // nested array, its `status`/`total` matched, and recursion reached
    // `shipping.carrier` two levels down. The query surface works alongside it.
    let body = FragUser::query().filter(FragUser::id().eq(1)).body();
    assert_eq!(body["query"]["bool"]["filter"][0]["term"]["id"], 1);
    Ok(())
}

// A shared field group, flattened in. Its keys live at the *enclosing* level, so
// the root checks it against that level rather than looking up a container
// field named `common` — which doesn't exist in the mapping at all.

#[derive(serde::Deserialize, FlussoFragment)]
struct Common {
    id: i32,
    email: String,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users", config = "tests/fixtures/flusso.toml")]
struct FlatUser {
    #[serde(flatten)]
    common: Common,
    #[flusso(rename = "fullName")]
    full_name: Option<String>,
}

#[test]
fn a_flattened_group_is_checked_against_the_enclosing_level() -> Result {
    // `id` and `email` are root fields of `users`; nothing named `common` is.
    // Compiling proves the group was checked here, not one level down.
    let body = FlatUser::query()
        .filter(FlatUser::email().eq("ada@x.com"))
        .body();
    assert_eq!(
        body["query"]["bool"]["filter"][0]["term"]["email"],
        "ada@x.com"
    );
    Ok(())
}

// A `#[serde(transparent)]` newtype: same shape, new name. The wrapper gets the
// full root surface, and the inner shape is checked against the root level.

#[derive(serde::Deserialize, FlussoFragment)]
struct UserFields {
    id: i32,
    email: String,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[serde(transparent)]
#[flusso(index = "users", config = "tests/fixtures/flusso.toml")]
struct UserView(UserFields);

#[test]
fn a_transparent_newtype_inherits_the_whole_surface() -> Result {
    // `UserView` has every handle the index has, and can start a search — while
    // `UserFields` was validated against the root level through the wrapper.
    let body = UserView::query()
        .filter(UserView::email().eq("ada@x.com"))
        .filter(UserView::orders().any(flusso_user_view_query::Orders::status().eq("paid")))
        .body();
    assert_eq!(
        body["query"]["bool"]["filter"][0]["term"]["email"],
        "ada@x.com"
    );
    Ok(())
}

// The case the whole feature exists for: ONE shape, TWO paths in the SAME index.
// `Address` names neither, and is validated against each.

#[derive(serde::Deserialize, FlussoFragment)]
#[serde(rename_all = "camelCase")]
struct Address {
    city: String,
    postal_code: Option<String>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[serde(rename_all = "camelCase")]
#[flusso(index = "users", config = "tests/fixtures/flusso.toml")]
struct AddressUser {
    id: i32,
    billing_address: Address,
    shipping_address: Address,
}

#[test]
fn one_fragment_serves_two_paths_in_the_same_index() -> Result {
    // Compiling means `Address` was checked twice — once per embedding.
    // Handles come from the root, so each path has its own, correctly prefixed.
    let body = AddressUser::query()
        .filter(AddressUser::billing_address().city().eq("Rome"))
        .filter(AddressUser::shipping_address().city().eq("Milan"))
        .body();
    let filters = &body["query"]["bool"]["filter"];
    assert_eq!(filters[0]["term"]["billingAddress.city"], "Rome");
    assert_eq!(filters[1]["term"]["shippingAddress.city"], "Milan");
    Ok(())
}

#[test]
fn a_generated_nested_namespace_carries_its_path_for_sorting() -> Result {
    // Sorting inside a `nested` array needs the boundary chain, which the
    // generated namespace supplies through `FlussoScope::PATH`.
    let body = User::query()
        .sorts([flusso_user_query::Orders::total().desc()])
        .body();
    let sort = &body["sort"][0]["orders.total"];
    assert_eq!(sort["order"], "desc");
    assert_eq!(sort["nested"]["path"], "orders");
    Ok(())
}

// `#[flusso(scope = "…")]` renames a generated namespace — to escape a clash
// with a type the caller already has, or to shorten a deep chain. The rename
// becomes the base for everything under that level.

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users", config = "tests/fixtures/flusso.toml")]
struct ScopedUser {
    id: i32,
    #[flusso(scope = "Purchases")]
    orders: Vec<Order>,
}

#[test]
fn a_field_can_rename_its_generated_namespace() -> Result {
    // `Purchases`, not the default `Orders` — and the sort still renders the
    // nested boundary, so the renamed type carries the same `PATH`.
    let body = ScopedUser::query()
        .filter(ScopedUser::orders().any(flusso_scoped_user_query::Purchases::status().eq("paid")))
        .sorts([flusso_scoped_user_query::Purchases::total().desc()])
        .body();
    assert_eq!(
        body["query"]["bool"]["filter"][0]["nested"]["path"],
        "orders"
    );
    assert_eq!(body["sort"][0]["orders.total"]["nested"]["path"], "orders");
    Ok(())
}

// Where you put the `Option` on a nested clause changes the meaning — the
// compiler accepts both, so this pins the difference.
#[test]
fn an_optional_nested_filter_means_different_things_inside_and_outside() -> Result {
    let absent: Option<String> = None;

    // INNER: an absent element predicate is *not* "skip" — `any` falls back to
    // `match_all`, so the clause becomes "has at least one order".
    let inner = User::query()
        .filter(
            User::orders().any(
                absent
                    .clone()
                    .map(|v| flusso_user_query::Orders::status().eq(v)),
            ),
        )
        .body();
    assert_eq!(
        inner["query"]["bool"]["filter"][0]["nested"]["path"],
        "orders"
    );
    assert!(inner["query"]["bool"]["filter"][0]["nested"]["query"]["match_all"].is_object());

    // OUTER: an absent nested clause drops out entirely — this is the one that
    // means "skip this filter when the request didn't ask for it".
    let outer = User::query()
        .filter(absent.map(|v| User::orders().any(flusso_user_query::Orders::status().eq(v))))
        .body();
    assert!(outer["query"]["match_all"].is_object());

    Ok(())
}

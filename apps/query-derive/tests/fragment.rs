//! `#[derive(FlussoFragment)]` — a location-free shape, validated by whoever
//! embeds it.
//!
//! The root codegen is what emits `const _: () = Frag::__flusso_check(LEVEL);`
//! in real use. These tests hand-bake the level so the fragment half is pinned
//! on its own: the same call, the same constants, no config resolution.
#![allow(dead_code, unused_crate_dependencies)]

use std::collections::{BTreeMap, HashMap};

use flusso_query::{FieldSpec, FlussoFragment, FlussoValue, KindTag};

/// A leaf spec, so the levels below stay readable.
const fn leaf(name: &'static str, kind: KindTag, nullable: bool) -> FieldSpec {
    FieldSpec {
        name,
        kind,
        nullable,
        array: false,
        variants: &[],
        map_values: None,
        children: &[],
    }
}

// ── the shapes ──────────────────────────────────────────────────────────────

#[derive(serde::Deserialize, FlussoFragment)]
#[serde(rename_all = "camelCase")]
struct Address {
    city: String,
    postal_code: Option<String>,
    geo: Geo,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Geo {
    lat: f64,
    lon: f64,
}

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[serde(rename_all = "lowercase")]
#[flusso(keyword)]
enum OrderStatus {
    Pending,
    Shipped,
    Delivered,
}

#[derive(serde::Deserialize, FlussoFragment)]
#[serde(rename_all = "camelCase")]
struct LineItem {
    product_id: i32,
    quantity: i32,
    status: OrderStatus,
    labels: HashMap<String, String>,
    notes: BTreeMap<String, String>,
    #[flusso(skip)]
    computed: String,
    #[flusso(opaque)]
    raw: PlainStruct,
}

/// Derives nothing — reachable only because the field is `#[flusso(opaque)]`.
#[derive(serde::Deserialize)]
struct PlainStruct {
    whatever: String,
}

// ── the levels a root would bake ────────────────────────────────────────────

const GEO: &[FieldSpec] = &[
    leaf("lat", KindTag::Double, false),
    leaf("lon", KindTag::Double, false),
];

const ADDRESS: &[FieldSpec] = &[
    leaf("city", KindTag::Keyword, false),
    leaf("postalCode", KindTag::Keyword, true),
    FieldSpec {
        name: "geo",
        kind: KindTag::Object,
        nullable: false,
        array: false,
        variants: &[],
        map_values: None,
        children: GEO,
    },
];

const LINE_ITEM: &[FieldSpec] = &[
    leaf("productId", KindTag::Integer, false),
    leaf("quantity", KindTag::Integer, false),
    FieldSpec {
        name: "status",
        kind: KindTag::Keyword,
        nullable: false,
        array: false,
        variants: &["pending", "shipped", "delivered", "cancelled"],
        map_values: None,
        children: &[],
    },
    FieldSpec {
        name: "labels",
        kind: KindTag::Object,
        nullable: false,
        array: false,
        variants: &[],
        map_values: Some(KindTag::Text),
        children: &[],
    },
    // A `BTreeMap` peels exactly like a `HashMap` — the deterministic-order
    // sibling, identical on disk.
    FieldSpec {
        name: "notes",
        kind: KindTag::Object,
        nullable: false,
        array: false,
        variants: &[],
        map_values: Some(KindTag::Keyword),
        children: &[],
    },
];

// ── what the root emits, once per embedding ─────────────────────────────────

// The whole point: ONE `Address`, checked at two different paths.
const _: () = Address::__flusso_check(ADDRESS);
const _: () = Address::__flusso_check(ADDRESS);

// Recursion reached `Geo` through `Address`; it also stands alone.
const _: () = Geo::__flusso_check(GEO);

// Custom value type, dynamic-key map, skipped and opaque fields.
const _: () = LineItem::__flusso_check(LINE_ITEM);

#[test]
fn a_fragment_checks_itself_against_a_level_it_was_handed() {
    // Reaching this line means every `const _` above evaluated — a fragment
    // embedded twice was checked twice, and recursion walked into `Geo`.
    assert_eq!(ADDRESS.len(), 3);
}

#[test]
fn a_fragment_reports_object_and_nested_kinds_so_a_parent_can_place_it() {
    use flusso_query::FlussoValueMeta;

    assert_eq!(
        <Address as FlussoValueMeta>::KINDS,
        &[KindTag::Object, KindTag::Nested]
    );
    assert!(<Address as FlussoValueMeta>::VARIANTS.is_empty());
}

#[test]
fn a_subset_of_the_schema_variants_is_a_legal_projection() {
    use flusso_query::{FlussoValueMeta, variants_covered};

    // The schema declares four; the Rust enum covers three. Legal — flusso
    // allows partial projections, so this must not fail the build.
    const _: () = assert!(variants_covered(
        LINE_ITEM,
        "status",
        <OrderStatus as FlussoValueMeta>::VARIANTS
    ));
    assert_eq!(<OrderStatus as FlussoValueMeta>::VARIANTS.len(), 3);
}

// A type of your own standing in for a `map` — not a newtype, a named-field
// struct whose *on-disk* shape is a flat object of same-kind values (language
// keys plus a `fallback`). `#[derive(FlussoMap)]` accepts it, and carries the
// declared value kind so a fragment checks the map's values, not merely that
// the schema field is object-ish.

#[derive(serde::Deserialize, flusso_query::FlussoMap)]
#[flusso(text)]
struct Translation {
    fallback: Option<String>,
    #[serde(flatten)]
    langs: HashMap<String, String>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Localized {
    title: Translation,
}

const TEXT_MAP: &[FieldSpec] = &[FieldSpec {
    name: "title",
    kind: KindTag::Object,
    nullable: false,
    array: false,
    variants: &[],
    map_values: Some(KindTag::Text),
    children: &[],
}];

const _: () = Localized::__flusso_check(TEXT_MAP);

#[test]
fn a_named_field_map_type_is_checked_down_to_its_value_kind() {
    use flusso_query::{FlussoValueMeta, map_kind_ok};

    // The declared kind travels with the type…
    assert_eq!(
        <Translation as FlussoValueMeta>::MAP_VALUES,
        &[KindTag::Text]
    );
    // …so a text map passes,
    const _: () = assert!(map_kind_ok(
        TEXT_MAP,
        "title",
        <Translation as FlussoValueMeta>::MAP_VALUES
    ));

    // …and a keyword map does not. (The generated check turns this into a
    // compile error at the embedding — see tests/ui/fragment_map_wrong_values.rs.)
    const KEYWORD_MAP: &[FieldSpec] = &[FieldSpec {
        name: "title",
        kind: KindTag::Object,
        nullable: false,
        array: false,
        variants: &[],
        map_values: Some(KindTag::Keyword),
        children: &[],
    }];
    assert!(!map_kind_ok(
        KEYWORD_MAP,
        "title",
        <Translation as FlussoValueMeta>::MAP_VALUES
    ));

    // A non-map type has no opinion and passes either way.
    const _: () = assert!(map_kind_ok(TEXT_MAP, "title", &[]));
}

// Parity with a root: an opaque `json` field — an object with *no declared
// children* — accepts anything, a bare map or a derived wrapper included. The
// baked spec carries that shape (object, no children, no value kind), so the
// fragment check recognises "opaque" without ever seeing the mapping.

#[derive(serde::Deserialize, FlussoFragment)]
struct WithOpaqueJson {
    meta: HashMap<String, String>,
    extra: Translation,
}

const OPAQUE_JSON: &[FieldSpec] = &[
    FieldSpec {
        name: "meta",
        kind: KindTag::Object,
        nullable: false,
        array: false,
        variants: &[],
        map_values: None,
        children: &[],
    },
    FieldSpec {
        name: "extra",
        kind: KindTag::Object,
        nullable: false,
        array: false,
        variants: &[],
        map_values: None,
        children: &[],
    },
];

const _: () = WithOpaqueJson::__flusso_check(OPAQUE_JSON);

#[test]
fn an_opaque_json_field_accepts_map_shaped_types() {
    // Reaching this line means the `const _` above evaluated: neither the
    // `HashMap` nor the map wrapper was rejected for landing on opaque JSON.
    assert_eq!(OPAQUE_JSON.len(), 2);
}

// The reported payment-method topology: a nullable object level containing a
// `text`-map `name`, projected by a fragment whose field is a map wrapper. The
// checks run in order (kind → map value kind → variants), so which assert fires
// tells apart the ways this can go wrong; the cases below pin each one.

#[derive(serde::Deserialize, FlussoFragment)]
struct PaymentMethod {
    name: Translation,
}

/// Derived with **no** kind tag — `FlussoMap` defaults to `keyword`, exactly
/// like `FlussoValue`. The likely way to hit the "matching value kind" error
/// on a `text` map is forgetting the tag when switching from a hand impl.
#[derive(serde::Deserialize, flusso_query::FlussoMap)]
struct UntaggedTranslation {
    fallback: Option<String>,
    #[serde(flatten)]
    langs: HashMap<String, String>,
}

const PAYMENT_METHOD: &[FieldSpec] = &[FieldSpec {
    name: "name",
    kind: KindTag::Object,
    nullable: false,
    array: false,
    variants: &[],
    map_values: Some(KindTag::Text),
    children: &[],
}];

const CUSTOMER: &[FieldSpec] = &[FieldSpec {
    name: "paymentMethod",
    kind: KindTag::Object,
    nullable: true,
    array: false,
    variants: &[],
    map_values: None,
    children: PAYMENT_METHOD,
}];

// What the root emits for `payment_method: Option<PaymentMethod>` — the
// fragment is checked against the object's sub-level. Text tag on the
// wrapper + text map in the schema: passes.
const _: () = PaymentMethod::__flusso_check(flusso_query::children(CUSTOMER, "paymentMethod"));

#[test]
fn the_map_value_kind_error_separates_its_three_causes() {
    use flusso_query::{FlussoValueMeta, map_kind_ok};

    let level = flusso_query::children(CUSTOMER, "paymentMethod");

    // Cause 1 — the wrapper forgot `#[flusso(text)]` and defaulted to keyword:
    // rejected against the text map (the generated check makes this the
    // compile error pinned in tests/ui/fragment_map_untagged_kind.rs).
    assert_eq!(
        <UntaggedTranslation as FlussoValueMeta>::MAP_VALUES,
        &[KindTag::Keyword]
    );
    assert!(!map_kind_ok(
        level,
        "name",
        <UntaggedTranslation as FlussoValueMeta>::MAP_VALUES
    ));

    // Cause 2 — the schema declares a map of another value kind (a keyword map
    // under a text-tagged wrapper): rejected. Same assert, other direction.
    const KEYWORD_NAME: &[FieldSpec] = &[FieldSpec {
        name: "name",
        kind: KindTag::Object,
        nullable: false,
        array: false,
        variants: &[],
        map_values: Some(KindTag::Keyword),
        children: &[],
    }];
    assert!(!map_kind_ok(
        KEYWORD_NAME,
        "name",
        <Translation as FlussoValueMeta>::MAP_VALUES
    ));

    // Cause 3 — the schema declares `name` as a *structured* object (children,
    // no value kind): rejected, a wrapper is not a sub-document.
    const OBJECT_NAME: &[FieldSpec] = &[FieldSpec {
        name: "name",
        kind: KindTag::Object,
        nullable: false,
        array: false,
        variants: &[],
        map_values: None,
        children: PAYMENT_METHOD,
    }];
    assert!(!map_kind_ok(
        OBJECT_NAME,
        "name",
        <Translation as FlussoValueMeta>::MAP_VALUES
    ));

    // And the correct declaration — `map:` with `values: text` — passes, as
    // does opaque `json` (covered by `an_opaque_json_field_accepts_map_shaped_types`).
    assert!(map_kind_ok(
        level,
        "name",
        <Translation as FlussoValueMeta>::MAP_VALUES
    ));
}

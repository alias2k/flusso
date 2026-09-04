#![allow(
    unused_crate_dependencies,
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing
)]

//! Drift guard between the `*.schema.yml` parser and its editor-assist JSON
//! Schema.
//!
//! `index.schema.yml` is hand-curated for editor UX, but the **sets** it
//! enumerates — field type tags, field siblings, enum tokens — must stay in
//! lockstep with what the parser accepts. Each test reflects a set from the Rust
//! types and asserts the schema lists exactly the same one. (The `flusso.toml`
//! schema is *generated* from the adapters' descriptions by the CLI and guarded
//! by a drift test there.)
//!
//! The reflection is compile-coupled to the types, not hand-copied:
//! - enum tokens come from `Serialize` (the real wire name), and the full
//!   variant list is fronted by a **wildcard-free `match`** that fails to
//!   *compile* when a variant is added;
//! - struct field sets come from **field-complete destructures** (no `..`) that
//!   likewise fail to compile when a field is added.
//!
//! What this deliberately does NOT check (see the schema file headers and
//! CLAUDE.md): prose descriptions, defaults, the intentionally permissive
//! `field` union grammar, and the identifier `pattern`s — which can't model the
//! newtypes' `trim`/`lowercase` sanitization.

use std::collections::BTreeSet;

use serde::Serialize;
use serde_json::Value;

// ── loading the embedded schemas ──────────────────────────────────────────────
//
// Read the schema as the crate embeds it (`include_str!`), so the test
// validates the bytes that ship and emit — not a loose file that could diverge.

/// The index schema (authored in YAML, but a JSON Schema document), as JSON.
fn index_schema() -> Value {
    serde_yaml::from_str(config::INDEX_SCHEMA).expect("index schema is YAML")
}

// ── helpers ──────────────────────────────────────────────────────────────────

/// The serde token of a value — its wire name, read from `Serialize` rather than
/// hard-coded, so it tracks any `#[serde(rename)]`.
fn token<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .expect("serializable")
        .as_str()
        .expect("a string-valued enum token")
        .to_owned()
}

fn tokens<T: Serialize>(values: &[T]) -> BTreeSet<String> {
    values.iter().map(token).collect()
}

fn strs(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

/// The `enum: [...]` strings at `pointer`.
fn schema_enum(schema: &Value, pointer: &str) -> BTreeSet<String> {
    schema
        .pointer(pointer)
        .unwrap_or_else(|| panic!("no node at {pointer}"))
        .as_array()
        .unwrap_or_else(|| panic!("{pointer} is not an array"))
        .iter()
        .map(|v| v.as_str().expect("enum entry is a string").to_owned())
        .collect()
}

/// The property names of the object at `pointer`.
fn schema_keys(schema: &Value, pointer: &str) -> BTreeSet<String> {
    schema
        .pointer(pointer)
        .unwrap_or_else(|| panic!("no node at {pointer}"))
        .as_object()
        .unwrap_or_else(|| panic!("{pointer} is not an object"))
        .keys()
        .cloned()
        .collect()
}

fn object_keys(value: &Value) -> BTreeSet<String> {
    value.as_object().expect("object").keys().cloned().collect()
}

// ── the parser's enum vocabularies (compile-coupled) ─────────────────────────

fn all_filter_ops() -> Vec<config::yaml::FilterOp> {
    use config::yaml::FilterOp::*;
    fn _exhaustive(op: config::yaml::FilterOp) {
        // Wildcard-free: a new FilterOp variant fails to compile until listed.
        use config::yaml::FilterOp::*;
        match op {
            Eq | Neq | Lt | Lte | Gt | Gte | In | NotIn | Like | Ilike | Between => {}
        }
    }
    vec![Eq, Neq, Lt, Lte, Gt, Gte, In, NotIn, Like, Ilike, Between]
}

fn all_null_ops() -> Vec<config::yaml::NullOp> {
    use config::yaml::NullOp::*;
    fn _exhaustive(op: config::yaml::NullOp) {
        use config::yaml::NullOp::*;
        match op {
            IsNull | IsNotNull => {}
        }
    }
    vec![IsNull, IsNotNull]
}

fn all_transforms() -> Vec<config::yaml::Transform> {
    use config::yaml::Transform::*;
    fn _exhaustive(t: config::yaml::Transform) {
        use config::yaml::Transform::*;
        match t {
            Lowercase | Trim => {}
        }
    }
    vec![Lowercase, Trim]
}

fn all_directions() -> Vec<config::yaml::Direction> {
    use config::yaml::Direction::*;
    fn _exhaustive(d: config::yaml::Direction) {
        use config::yaml::Direction::*;
        match d {
            Asc | Desc => {}
        }
    }
    vec![Asc, Desc]
}

fn all_join_verbs() -> Vec<config::yaml::JoinVerb> {
    use config::yaml::JoinVerb::*;
    fn _exhaustive(j: config::yaml::JoinVerb) {
        use config::yaml::JoinVerb::*;
        match j {
            BelongsTo | HasOne | HasMany | ManyToMany => {}
        }
    }
    vec![BelongsTo, HasOne, HasMany, ManyToMany]
}

fn all_aggregate_ops() -> Vec<config::yaml::AggregateOp> {
    use config::yaml::AggregateOp::*;
    fn _exhaustive(a: config::yaml::AggregateOp) {
        use config::yaml::AggregateOp::*;
        match a {
            Count | Sum | Avg | Min | Max | Ids => {}
        }
    }
    vec![Count, Sum, Avg, Min, Max, Ids]
}

/// The 16 scalar `FlussoType`s — those usable as a field type tag and as an
/// aggregate `value_type`. `GeoPoint`/`Custom` are excluded (their tags are the
/// separate `geo`/`custom` keywords, and they're not valid aggregate results).
fn scalar_type_tokens() -> BTreeSet<String> {
    use config::FlussoType::*;
    fn _exhaustive(ty: &config::FlussoType) {
        use config::FlussoType::*;
        match ty {
            Text | Identifier | Keyword | Enum | Uuid | Boolean | Short | Integer | Long
            | Float | Double | Decimal | Date | Timestamp | Binary | Json => {}
            GeoPoint => {}
            Map { .. } => {}
            Custom { .. } => {}
        }
    }
    tokens(&[
        Text, Identifier, Keyword, Enum, Uuid, Boolean, Short, Integer, Long, Float, Double,
        Decimal, Date, Timestamp, Binary, Json,
    ])
}

/// The union of every field body's keys, minus the internal `field` (which is
/// the type tag's value on the wire, not a sibling). Fronted by field-complete
/// destructures so adding a body field fails to compile until handled here.
fn body_sibling_keys() -> BTreeSet<String> {
    fn _guards(
        s: config::yaml::ScalarBody,
        e: config::yaml::EnumBody,
        c: config::yaml::CustomBody,
        g: config::yaml::GeoBody,
        o: config::yaml::ObjectBody,
        m: config::yaml::MapBody,
        j: config::yaml::JoinBody,
        a: config::yaml::AggregateBody,
        k: config::yaml::ConstantBody,
    ) {
        // No `..`: a new field on any body breaks compilation until added below.
        let config::yaml::ScalarBody {
            field: _,
            column: _,
            required: _,
            options: _,
            transforms: _,
            default: _,
        } = s;
        let config::yaml::EnumBody {
            field: _,
            variants: _,
            column: _,
            required: _,
            options: _,
            transforms: _,
            default: _,
        } = e;
        let config::yaml::CustomBody {
            field: _,
            postgres: _,
            opensearch: _,
            column: _,
            required: _,
            options: _,
            default: _,
        } = c;
        let config::yaml::GeoBody {
            field: _,
            lat: _,
            lon: _,
            column: _,
            required: _,
            options: _,
        } = g;
        let config::yaml::ObjectBody {
            field: _,
            options: _,
            fields: _,
        } = o;
        let config::yaml::MapBody {
            field: _,
            values: _,
            column: _,
            required: _,
            options: _,
        } = m;
        let config::yaml::JoinBody {
            field: _,
            table: _,
            primary_key: _,
            required: _,
            column: _,
            foreign_key: _,
            through: _,
            filters: _,
            order_by: _,
            limit: _,
            fields: _,
            options: _,
        } = j;
        let config::yaml::AggregateBody {
            field: _,
            table: _,
            column: _,
            value_type: _,
            element_type: _,
            foreign_key: _,
            through: _,
            filters: _,
            options: _,
        } = a;
        let config::yaml::ConstantBody { field: _, value: _ } = k;
    }
    strs(&[
        "column",
        "required",
        "options",
        "transforms",
        "default",  // scalar
        "variants", // enum (declared order)
        "postgres",
        "opensearch", // custom
        "lat",
        "lon",    // geo
        "fields", // object / join
        "table",
        "primary_key",
        "foreign_key",
        "through",
        "filters",
        "order_by",
        "limit",        // join
        "value_type",   // aggregate (sum/min/max)
        "element_type", // aggregate (ids)
        "values",       // map (value leaf type)
        "value",        // constant
    ])
}

// ── index schema ─────────────────────────────────────────────────────────────

/// A field type tag in the schema is any `field` property whose value is exactly
/// a `field_name` ref (the document key); everything else is a sibling.
fn schema_field_props() -> (BTreeSet<String>, BTreeSet<String>) {
    let schema = index_schema();
    let props = schema
        .pointer("/definitions/field/properties")
        .expect("field.properties")
        .as_object()
        .expect("object")
        .clone();
    let is_tag =
        |v: &Value| v.get("$ref").and_then(Value::as_str) == Some("#/definitions/field_name");
    let tags = props
        .iter()
        .filter(|(_, v)| is_tag(v))
        .map(|(k, _)| k.clone())
        .collect();
    let siblings = props
        .iter()
        .filter(|(_, v)| !is_tag(v))
        .map(|(k, _)| k.clone())
        .collect();
    (tags, siblings)
}

#[test]
fn index_field_type_tags_match_parser() {
    let (schema_tags, _) = schema_field_props();

    let mut rust_tags = scalar_type_tokens();
    rust_tags.extend(tokens(&all_join_verbs()));
    rust_tags.extend(tokens(&all_aggregate_ops()));
    // Parser keywords with no 1:1 serializable enum: `geo`→GeoPoint, `object`→
    // Group, `map`→Map, `custom`→Custom, `constant`→Constant (see
    // `field::classify`).
    rust_tags.extend(strs(&["geo", "object", "map", "custom", "constant"]));

    assert_eq!(
        schema_tags, rust_tags,
        "field type tags drifted (index.schema.yml vs field::classify)"
    );
}

#[test]
fn index_field_siblings_match_parser() {
    let (_, schema_siblings) = schema_field_props();
    assert_eq!(
        schema_siblings,
        body_sibling_keys(),
        "field siblings drifted (index.schema.yml vs the *Body structs)"
    );
}

#[test]
fn index_value_type_enum_matches_scalar_types() {
    assert_eq!(
        schema_enum(&index_schema(), "/definitions/flusso_type/enum"),
        scalar_type_tokens(),
        "flusso_type/value_type enum drifted (vs FlussoType scalars)"
    );
}

#[test]
fn index_filter_ops_match_parser() {
    let schema = index_schema();
    let branches = schema
        .pointer("/definitions/filter/oneOf")
        .expect("filter.oneOf")
        .as_array()
        .expect("array");

    let mut value_ops = BTreeSet::new();
    let mut null_ops = BTreeSet::new();
    for branch in branches {
        if let Some(arr) = branch
            .pointer("/properties/op/enum")
            .and_then(Value::as_array)
        {
            let set: BTreeSet<String> =
                arr.iter().map(|v| v.as_str().unwrap().to_owned()).collect();
            if set.contains("is_null") {
                null_ops = set;
            } else {
                value_ops = set;
            }
        }
    }

    assert_eq!(
        value_ops,
        tokens(&all_filter_ops()),
        "value filter ops drifted"
    );
    assert_eq!(null_ops, tokens(&all_null_ops()), "null filter ops drifted");
}

#[test]
fn index_transforms_match_parser() {
    assert_eq!(
        schema_enum(&index_schema(), "/definitions/transforms/items/enum"),
        tokens(&all_transforms()),
        "transforms drifted",
    );
}

#[test]
fn index_order_by_directions_match_parser() {
    assert_eq!(
        schema_enum(
            &index_schema(),
            "/definitions/order_by/items/properties/direction/enum"
        ),
        tokens(&all_directions()),
        "order_by directions drifted",
    );
}

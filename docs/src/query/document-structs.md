# Document structs

Write the root and fragment structs for an index, know what the derive checks about each field, and map every flusso type to its Rust type.

## The structs

A root binds to an index by name and projects the fields it wants. Everything nested is a fragment.

```rust
use flusso_query::{FlussoFragment, FlussoRoot};

#[derive(Debug, Clone, serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
pub struct User {
    pub id: i32,                                // primary key → never null
    pub email: String,                          // keyword, required
    #[serde(rename = "fullName")]
    pub full_name: Option<String>,              // text, not required → nullable
    pub account: Account,                       // object → always present
    pub orders: Vec<Order>,                     // has_many → nested, never null
    #[serde(rename = "orderCount")]
    pub order_count: i64,                       // count → long, never null
    #[serde(rename = "lifetimeValue")]
    pub lifetime_value: Option<f64>,            // sum → nullable
}

#[derive(Debug, Clone, serde::Deserialize, FlussoFragment)]
pub struct Account {
    pub tier: String,
    pub country: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: time::OffsetDateTime,
}

#[derive(Debug, Clone, serde::Deserialize, FlussoFragment)]
pub struct Order {
    pub status: String,
    pub total: Decimal,
    pub items: Vec<Item>,                       // a deeper has_many
}

#[derive(Debug, Clone, serde::Deserialize, FlussoFragment)]
pub struct Item {
    #[serde(rename = "productId")]
    pub product_id: i32,
    pub quantity: i32,
}
```

A struct is a **projection**: it may omit schema fields (here `addresses`, `profile`, `avgOrderValue`). Only the fields it declares are checked.

## What the derive checks

For each declared field, the derive finds the schema field by **document key**, honoring `#[serde(rename)]` and a container `#[serde(rename_all)]`, then checks three things.

| Check | Compile error when |
| --- | --- |
| field exists | `` no field `totl` in index `users` `` |
| type matches | `` email is `keyword` → expected `String`, found `i32` `` |
| nullability matches | `` email is required → expected `String`, found `Option<String>` `` |

Type matching is by **leaf identifier plus `Option` shape**: the macro compares the final path segment (`String`, `i32`, `OffsetDateTime`) against the table below, since it can't resolve aliases. An `object` field expects a struct, a `nested` field a `Vec<_>`, and the inner check is handed to that fragment.

Fragments are checked **where embedded**. The root bakes the resolved level into const data and drives one assertion per fragment field; the error's primary span is the embedding, with a note chain down to the offending field. Two limits follow from const evaluation: messages can't interpolate the schema's type name, and there are no warnings, only pass or fail.

## Escape hatches

- `serde_json::Value` as a field type opts that field out of type checking.
- `#[flusso(skip)]` drops a field from validation entirely, for a computed or app-only field (pair with `#[serde(skip)]` or `#[serde(default)]`).
- `#[flusso(opaque)]` keeps the field checked against the mapping but skips the shape check, for a plain un-derived struct.
- `#[serde(flatten)]` on a fragment field checks its fields against the enclosing level. `#[serde(transparent)]` newtypes are checked against the enclosing level too.

## flusso types to Rust types

| flusso type | Rust type | Handle |
| --- | --- | --- |
| `text`, `identifier` | `String` | `Text` |
| `keyword` | `String`, or a `FlussoValue` newtype/enum | `Keyword` |
| `enum` | `String`, or a `#[derive(FlussoValue)]` enum | `Enum` with declared `variants`, else `Keyword` |
| `uuid` | `String`, or `uuid::Uuid` (`uuid` feature) | `Keyword` |
| `boolean` | `bool` | `Bool` |
| `short` / `integer` / `long` | `i16` / `i32` / `i64` | `Number<kind::Short \| Integer \| Long>` |
| `float` / `double` | `f32` / `f64` | `Number<kind::Float \| Double>` |
| `decimal` | `Decimal` (`decimal` feature) or `f64` | `Number<kind::Decimal>` |
| `date` | `time::Date` or chrono's | `Date` |
| `timestamp` | `time::OffsetDateTime` or chrono's | `Date` |
| `binary` | `String` (base64) | `Binary` |
| `json` | `serde_json::Value` | `Json` |
| `map` | `HashMap<String, V>`, `BTreeMap<String, V>`, or a `#[derive(FlussoMap)]` struct | `TextMap` / `KeywordMap` / `NumberMap` / `DateMap` |
| `geo` | `GeoPoint { lat, lon }` | `Geo` |
| `custom` | the matching scalar, else `serde_json::Value` | by OpenSearch type |
| `object` | a struct | object namespace |
| `belongs_to`, `has_one` | `Option<Struct>` | object namespace |
| `has_many`, `many_to_many` | `Vec<Struct>` | `Nested<S, T>` |
| `ids` aggregate | `Vec<i64>` or `Vec<String>` | `Number` / `Keyword` |

Dates sit behind a feature so a caller picks `time` or `chrono` (or `String` for raw ISO-8601).

## Nullability

A field is `T` or `Option<T>`, and the derive checks which. Nullability comes from the schema, not from data.

| Field source | Nullable |
| --- | --- |
| root `primary_key`, a join's `primary_key` field | no |
| leaf with `required: true` | no |
| leaf without `required` | yes |
| `object` | no |
| `belongs_to`, `has_one` | yes |
| `has_many`, `many_to_many` | no; empty `Vec`, never null |
| `count`, `ids` | no |
| `sum`, `avg`, `min`, `max` | yes |

## Related

- [Binding to the schema](binding.md) for how the root finds the schema and what it generates.
- [Enums and custom values](enums-and-values.md) for `FlussoValue` types in struct fields.

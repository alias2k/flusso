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

For each declared field, the derive finds the schema field by **document key**, honoring `#[serde(rename)]` (plain, or the `deserialize` name of `rename(serialize = …, deserialize = …)`) and a container `#[serde(rename_all)]` (the `deserialize` rule of `rename_all(serialize = …, deserialize = …)`), then checks three things. It reads those from among any other serde attributes (`bound`, `default`, `with`, …). A field spelled as a raw identifier reads the key serde reads: `r#type` is the key `type`.

| Check | Compile error when |
| --- | --- |
| field exists | `` no field `totl` in index `users` `` |
| type matches | `` email is `keyword` → expected `String`, found `i32` `` |
| nullability matches | `` email is required → expected `String`, found `Option<String>` `` |

Type matching is by **leaf identifier plus `Option` shape**: the macro compares the final path segment (`String`, `i32`, `OffsetDateTime`) against the table below, since it can't resolve aliases. An `object` field expects a struct, a `nested` field a `Vec<_>`, and the inner check is handed to that fragment. A `HashMap`/`BTreeMap` is a `map`'s shape or a `json` field's, and a `Vec` of them a `json` field's alone; neither is ever an `object` or `nested` array with fields of their own: declare a struct for that (at a `nested` array, one per element, as `Vec<Struct>`), on a root and in a fragment. A `Vec<_>` goes exactly where the schema field is an array (a `nested` array or a flat one), on a root and in a fragment.

A `#[serde(transparent)]` (or tuple) `FlussoFragment` newtype over a `Vec` of a struct reads the same JSON array, so it goes where that `Vec` would: `orders: Orders` with `struct Orders(Vec<Order>)` at a `nested` field, each element checked as `Order`. So does a newtype over such a newtype. One over `Option<Vec<_>>` reads `null` too, so it is no array. A newtype over a `Vec` is rejected at an `object`, and so is a `Vec` of one anywhere a level is checked: that is an array of arrays. The element has to be a fragment: a newtype over a `Vec` of a value type (an enum, a `FlussoValue` newtype) is no array of sub-documents. The fragment derive marks a newtype this way, and a hand-written `FlussoValueMeta` leaves `ARRAY` unset: a plain struct under `opaque` at a `nested` field still has to be a `Vec<_>`.

### Types named like a built-in

Because a macro can't resolve paths, a built-in leaf type is recognized by that last segment alone: `Date`, `NaiveDate`, `NaiveDateTime`, `DateTime`, `OffsetDateTime`, `PrimitiveDateTime`, `Uuid`, `Decimal`, `GeoPoint`, and `Value` for `serde_json::Value`. Matching the whole path instead would reject re-exports such as `sqlx::types::Uuid` or a `pub use chrono::NaiveDate` in your own crate, so a type of your own with one of these names counts as the built-in, with two exceptions where no built-in leaf could sit:

- on a root, at an `object` or `nested` field (a level with fields of its own), it is checked as your own type;
- on a `#[serde(flatten)]` field, root or fragment, it is checked as your own type too, except `GeoPoint`: flattened, it is taken for flusso's own, which serde can flatten and which is not checked.

Anywhere else it is taken for the built-in: in a fragment the field then fails to match the schema, and the error says to rename the type. A type named `Value` is always taken for `serde_json::Value`, which is never checked. Rename it to have it checked.

Fragments are checked **where embedded**. The root bakes the resolved level into const data and drives one assertion per fragment field; the error's primary span is the embedding, with a note chain down to the offending field. Two limits follow from const evaluation: messages can't interpolate the schema's type name, and there are no warnings, only pass or fail.

## Stored-only fields

A field with [`indexed: false`](../reference/joins.md#indexed) (a join or a `json` blob) is in `_source` but not in the index, so the derive generates **no handle** for it: `User::price_history()` doesn't exist, and a query on it is a compile error rather than an empty result. The struct field still deserializes and is checked like any other: a stored `has_many` is a `Vec<Fragment>`, a stored `json` a `serde_json::Value`.

```rust
#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "products")]
pub struct Product {
    pub name: String,
    #[serde(rename = "priceHistory")]
    pub price_history: Vec<PriceChange>,   // indexed: false → read, never queried
}
```

## Escape hatches

- `serde_json::Value` as a field type opts that field out of type checking.
- `#[flusso(skip)]` drops a field from validation entirely, for a computed or app-only field (pair with `#[serde(skip)]` or `#[serde(default)]`).
- `#[flusso(opaque)]` turns off the checks the derive hands to a field's custom type, for a plain un-derived struct. The field is still checked against the mapping: it exists, its nullability matches, and it is a `Vec<…>` exactly where the schema field is an array. It goes on a field whose type is a custom struct or enum, bare or in `Option<…>` or `Vec<…>`, at an `object`, `nested` or `json` field, where it turns off the walk into the type, or at a `keyword`/`text` field, where it turns off the variant check of a `FlussoValue` enum (or of a [hand-written `FlussoValue`](enums-and-values.md#hand-written-values)) and the type's kind is still checked. Anywhere it would turn off nothing it is a compile error:
  - a built-in leaf type (see [Types named like a built-in](#types-named-like-a-built-in)), `serde_json::Value` (or a `Vec` of it), a `HashMap`/`BTreeMap` (flattened or not), a `#[flusso(skip)]` field;
  - a `map` or a leaf other than `keyword`/`text` in the schema, where a custom type is checked by its kind alone;
  - at a `keyword`/`text` field, a type with no variants to check: a newtype over a `String`, or an enum that isn't `exhaustive` where the schema field declares no `variants:`;
  - the field of a `#[serde(transparent)]` newtype: put `opaque` on the field that embeds the newtype.

  A fragment applies these rules where it is embedded. It reads a type's kind from the metadata a flusso derive emits, where a root checks the type's `FlussoValue` bound, so at a leaf a fragment rejects a type without that metadata, `opaque` or not; at an `object`, `nested` or `json` field `opaque` still takes a plain struct. A hand-written `FlussoValue` [carries the metadata by hand](enums-and-values.md#hand-written-values).
- `#[flusso(rename = "…")]` picks one of a field's `#[serde(alias)]`es as the document key the derive checks; serde reads either. That is its only use: the derive already reads the key serde reads, from `#[serde(rename = "…")]` (or the `deserialize` name of `rename(serialize = …, deserialize = …)`) and `rename_all`, so set the key there. A `#[flusso(rename)]` that names no `alias` is a compile error: naming the key serde reads repeats it, and naming any other key would check the schema under a key serde never reads, so every real document would fail to deserialize. A `#[serde(flatten)]` field has no key of its own, so `rename` on one is a compile error too.
- `#[serde(flatten)]` on a fragment field checks its fields against the enclosing level. A flattened `HashMap<String, serde_json::Value>`, `BTreeMap<…>` or `serde_json::Value`, serde's catch-all for the keys no other field reads, is never checked, on a root or a fragment. `#[serde(transparent)]` newtypes are checked against the enclosing level too, as the one field serde reads: the field that is not `#[serde(skip)]`/`skip_deserializing`, `#[serde(default)]` or a `PhantomData`, in a tuple struct or a named one. No field of a transparent struct takes a `#[flusso(…)]` attribute, and one with more than one such field is a compile error, as it is in serde.
- Each `#[flusso(…)]` key is given once per field or type: a repeat is a compile error, not "last one wins".

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
| `json` | `serde_json::Value`, any `HashMap`/`BTreeMap<String, _>`, or a `Vec` of them; a `Vec<_>` with `array: true` | `Json`; none with `indexed: false` |
| `map` | `HashMap<String, V>`, `BTreeMap<String, V>`, or a `#[derive(FlussoMap)]` struct | `TextMap` / `KeywordMap` / `NumberMap` / `DateMap` |
| `geo` | `GeoPoint { lat, lon }` | `Geo` |
| `custom` | the matching scalar, else `serde_json::Value` | by OpenSearch type |
| `object` | a struct | object namespace |
| `belongs_to`, `has_one` | `Option<Struct>` | object namespace |
| `has_many`, `many_to_many` | `Vec<Struct>`, or a fragment newtype over one | `Nested<S, T>` |
| any join or `json` with `indexed: false` | as above | none; see [Stored-only fields](#stored-only-fields) |
| any scalar, `enum` or `custom` with `array: true` | `Vec<T>` of the type above, or a `FlussoValue` newtype over one; see [Arrays](#arrays) | the element's handle |
| `ids` aggregate | `Vec<i64>` or `Vec<String>` | `Number` / `Keyword` |
| `exists` aggregate | `bool` | `Bool` |

Dates sit behind a feature so a caller picks `time` or `chrono` (or `String` for raw ISO-8601).

## Arrays

A field declared [`array: true`](../reference/field-types.md#array) is a `Vec` of the element type, in `Option` unless it's `required`. The handle is the element's, so operators take one element and match a document when any element matches.

```rust
#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "orders")]
pub struct Order {
    pub tags: Option<Vec<Tag>>,    // enum: tags, array: true
}

Order::tags().any_of([Tag::Urgent]);
```

A [`FlussoValue` newtype over a `Vec`](enums-and-values.md#arrays) goes where that `Vec` would: `tags: Option<Tags>` with `struct Tags(Vec<Tag>)`, the shape an ORM decoding a `jsonb` column usually wants. A plain type (`Tag`, `String`) at an array field is a compile error, and so is a `Vec` or an array newtype at a field that isn't one. A `json` array takes a `Vec` of anything, `serde_json::Value`, or an array newtype; no bare struct. Sorting an array uses OpenSearch's default: the smallest element ascending, the largest descending.

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
| `count`, `ids`, `exists` | no |
| `sum`, `avg`, `min`, `max` | yes |

## Related

- [Binding to the schema](binding.md) for how the root finds the schema and what it generates.
- [Enums and custom values](enums-and-values.md) for `FlussoValue` types in struct fields.

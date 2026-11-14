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

A `#[serde(transparent)]` (or tuple) `FlussoFragment` newtype over a `Vec` of a struct reads the same JSON array, so it goes where that `Vec` would: `orders: Orders` with `struct Orders(Vec<Order>)` at a `nested` field, each element checked as `Order`. Where each [kind of array newtype](#two-kinds-of-array-newtype) fits is one table. A `Vec` of an array newtype is an array of arrays, rejected anywhere a level is checked. The fragment derive marks a newtype this way, and a hand-written `FlussoValueMeta` leaves `ARRAY` unset: a plain struct under `opaque` at a `nested` field still has to be a `Vec<_>`.

### Types named like a built-in

Because a macro can't resolve paths, a built-in leaf type is recognized by that last segment alone: `Date`, `NaiveDate`, `NaiveDateTime`, `NaiveTime`, `DateTime`, `OffsetDateTime`, `PrimitiveDateTime`, `Uuid`, `Decimal`, `GeoPoint`, and `Value` for `serde_json::Value`. Matching the whole path instead would reject re-exports such as `sqlx::types::Uuid` or a `pub use chrono::NaiveDate` in your own crate, so a type of your own with one of these names counts as the built-in, with two exceptions where no built-in leaf could sit:

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
- `#[serde(flatten)]` on a fragment field checks its fields against the enclosing level. serde flattens only a struct or a map, so a flattened `Vec`, array newtype or single value (a `String`, a `FlussoValue` newtype) is a compile error, on a root and in a fragment, `opaque` or not; an `Option` of a struct is fine. A flattened `HashMap`, `BTreeMap` or `serde_json::Value`, serde's catch-all for the keys no other field reads, is [trusted](#what-the-derive-trusts), on a root or a fragment. `#[serde(transparent)]` newtypes are checked against the enclosing level too, as the one field serde reads: the field that is not `#[serde(skip)]`/`skip_deserializing`, `#[serde(default)]` or a `PhantomData`, in a tuple struct or a named one. No field of a transparent struct takes a `#[flusso(…)]` attribute, and one with more than one such field is a compile error, as it is in serde.
- Each `#[flusso(…)]` key is given once per field or type: a repeat is a compile error, not "last one wins".
- `#[serde(from)]`/`try_from` on a `FlussoFragment` or `FlussoMap` type is a compile error: serde then reads another type, so derive on the type serde reads, and convert after reading. The other serde attributes that change what is read are [trusted](#what-the-derive-trusts).

## What the derive trusts

The derive checks a struct against the mapping at compile time, but a struct that compiles can still fail to read a document. This section says what it checks, what it knowingly takes on trust, and the gaps known so far.

**What it checks.** A type wherever it can see it: a built-in leaf, a type with a flusso derive, the two [array newtypes](#two-kinds-of-array-newtype), and a `HashMap`/`BTreeMap<String, V>` at a field, where `V` may be `Option<V>` (one level).

- A map's keys are the document's keys, any string: a key type the derive knows reads no arbitrary string (a number, `bool`, `char`, `Uuid`, a tuple) is a compile error.
- A map type is checked as the map it reads: a [`FlussoMap` type](maps.md#document-types)'s values against its tag, and a `FlussoFragment` or `FlussoValue` newtype over a map (or over an `Option` of one) only goes where that map goes, its values checked against the field's.
- A `FlussoFragment` newtype over a leaf value is a compile error: it is a value, so derive `FlussoValue`.
- An `exhaustive` enum where the schema declares no `variants:` is a compile error everywhere, a `json` field and a map's values included.

**What it trusts.** Each of these compiles, and a document that disagrees fails to read:

- **Any type at a `json` field, or at a `constant` stored unindexed.** Neither declares anything to check against: a [`constant`](../reference/field-types.md#constant) holding an object or a list that mixes kinds is mapped like `json`.
- **A map key type of your own.** `String`, `Box<str>`, a `type` alias or a newtype of yours is trusted to read every key.
- **A `#[serde(flatten)]` map.** A flattened `HashMap`, `BTreeMap`, `FlussoMap` type or newtype over a map reads every key no other field reads, whatever its kind, and its value type isn't checked against them. Only `serde_json::Value` values read them all.
- **A map value that is not an `Option`.** It is trusted never to be `null`, but a `map` over a `jsonb` column holding `{"x": null}` indexes that `null` under its key. Use `Option<V>` (`HashMap<String, Option<String>>`, or `Option` values in a `FlussoMap` type) when the column can hold one.
- **An enum where the schema declares no `variants:`.** At a plain `keyword`, or as a map's values, every value must be one of the enum's.
- **An enum without `exhaustive` where the schema declares `variants:`.** A Rust enum may cover only some of them (a projection), so it is trusted to cover the values stored. Mark it `#[flusso(keyword, exhaustive)]` to have the derive require every declared variant.
- **A date type at a `date` or `timestamp` field.** Both map to OpenSearch `date`, so the derive can't tell them apart, and a `timestamp` field holds whatever its column is. What flusso writes, and what reads it:

  | Column | Written as | Reads into |
  | --- | --- | --- |
  | `date` | `2024-01-02` | `chrono::NaiveDate`, `String` |
  | `time` | `03:04:05.123456` | `chrono::NaiveTime`, `String` |
  | `timetz` | `03:04:05.123456+02` | `String` only |
  | `timestamp` (no zone) | `2024-01-02T03:04:05.123456` | `chrono::NaiveDateTime`, `String` |
  | `timestamptz` | `2024-01-02T03:04:05.123456+00:00` | `chrono::DateTime<Utc>`, `String` |

  The `time` crate's types aren't covered here: nothing in flusso's tests reads these forms with them.
- **Any other type behind a tagged `FlussoValue` newtype.** `#[flusso(keyword)] struct Sku(SmolStr)` is how a string type from another crate goes at a `keyword` field, so it is trusted to read one string, whatever it is named (`Date`, `Decimal`, `Duration`, `chrono::NaiveDate` holding date text included). Looking through `Option`, `Box`, `Rc`, `Arc`, `Cell`, `RefCell`, `Mutex`, `RwLock`, `Cow`, `Wrapping`, `Saturating` and `Reverse`, the derive refuses exactly: a `HashMap`/`BTreeMap`, a `Vec`/`HashSet`/`BTreeSet`/`VecDeque`/`LinkedList`/`BinaryHeap`, a slice, tuple or array, a Rust primitive other than `String` and `char` (a `char` reads a one-character string), `NonZero*`, and a flusso type that reads an object (a `FlussoMap` type, a sub-document).
- **What [`#[flusso(opaque)]`](#escape-hatches) turns off**, and a type the derive can't see into under it (`Box<…>`, a tuple, a plain struct).
- **serde attributes that change what is read**: `with`/`deserialize_with` on a field, `from`/`try_from` on a `FlussoValue` type (validation in `try_from` is a common use), and `deny_unknown_fields` on a fragment, which then fails on every key it leaves out.
- **A `String` at a `geo` field.** flusso writes a point as `{lat, lon}` for a two-column `geo` (`lat:`/`lon:`), which a `String` doesn't read; a one-column `geo` is written as its column holds it ([#307](https://github.com/alias2k/flusso/issues/307)), and a `custom` field mapped to `geo_point` can hold `"lat,lon"` strings.

**Known gaps.** These compile and can fail on a document flusso writes, and are neither checked nor a deliberate trust yet ([#306](https://github.com/alias2k/flusso/issues/306), unless noted):

- an untagged integer newtype at a wider numeric kind;
- a `custom` field whose OpenSearch type the derive does not model;
- a type of your own named like a built-in (`Value`, `Date`, `Uuid`, `Decimal`): see [Types named like a built-in](#types-named-like-a-built-in);
- `uuid::Uuid` at a plain `keyword`;
- a stored value outside an `enum` field's `variants:`;
- `Decimal` at a `double`;
- a `jsonb` map or array column holding values of another type than its `values:`/element type;
- `GeoPoint` at a one-column `geo` ([#307](https://github.com/alias2k/flusso/issues/307)).

`sdk/query-derive/tests/boundary.rs` pins what it covers: it places a list of type shapes at one field of each kind in its fixture, on a root and in a fragment, and fails on one that compiles and fails to read a sample document under no rule above, or that a root and a fragment treat differently. It covers the kinds its fixture holds, not every kind, which is why the gaps above went unseen. One kind of pair differs on purpose: a `type` alias (of a map, or of `String`) reads on a root, which sees through it with a trait bound, and is a compile error in a fragment, which can't.

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
| `json` | `serde_json::Value`, any `HashMap`/`BTreeMap<String, _>`, or a `Vec` of them; with `array: true`, see [Arrays](#arrays) | `Json`; none with `indexed: false` |
| `map` | `HashMap<String, V>`, `BTreeMap<String, V>`, or a `#[derive(FlussoMap)]` struct | `TextMap` / `KeywordMap` / `NumberMap` / `DateMap` |
| `geo` | `GeoPoint { lat, lon }`, read from every form a `geo` field is stored in ([geo](../reference/field-types.md#geo)) | `Geo` |
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

A [`FlussoValue` newtype over a `Vec`](enums-and-values.md#arrays) goes where that `Vec` would: `tags: Option<Tags>` with `struct Tags(Vec<Tag>)`, the shape an ORM decoding a `jsonb` column usually wants. A plain type (`Tag`, `String`) at an array field is a compile error, and so is a `Vec` or an array newtype at a field that isn't one. Sorting an array uses OpenSearch's default: the smallest element ascending, the largest descending.

A `json` array takes `serde_json::Value`, a `Vec` of any value (`String`, a number, an enum, `serde_json::Value`, a map, an array newtype), or an array newtype of values. A `Vec` of a fragment and a bare struct are compile errors: a `json` field declares no fields to check a struct against. A root and a fragment agree on all of it, with `#[flusso(opaque)]` too, which there turns off the walk into a struct element.

### Two kinds of array newtype

Both read a whole JSON array and go where that `Vec` would. What the array holds decides where each fits:

| Newtype | Derive | Goes at | A compile error at |
| --- | --- | --- | --- |
| over `Vec<Struct>`, an array of sub-documents | `#[derive(FlussoFragment)]`, `#[serde(transparent)]` or a tuple struct | a `nested` field (`has_many`, `many_to_many`) | an array leaf, a `json` array, an `object`, a `map`, a single-valued leaf |
| over `Vec<E>`, `E` a leaf value (`String`, a number, an enum, a `FlussoValue` newtype) | `#[derive(FlussoValue)]`, no kind tag | a field with `array: true` of `E`'s kind, or a `json` array | a `nested` field, an `object`, a `map`, a single-valued leaf |

A newtype over either one, with the same derive, is the same kind. A plain `json` field takes either, as it takes any JSON.

These two are the only types that read an array in place of a `Vec`. Each derive refuses, where the newtype is declared, any other newtype that would:

| Declared | Why | Instead |
| --- | --- | --- |
| `FlussoFragment` over `Option<Vec<Struct>>`, or `Option<…>` of a fragment array newtype | it reads `null` too, and a `nested` field is never null: no rows read as an empty array | drop the `Option`: `struct Orders(Vec<Order>)`, field `orders: Orders` |
| `FlussoValue` over `Option<Vec<_>>` | it reads `null` too, which no array field holds | wrap the `Vec`, and put the `Option` on the field: `Option<Tags>` |
| over a `Vec` of an array newtype | an array of arrays | wrap the element type |
| `FlussoFragment` over a `Vec` of values (`Vec<String>`, `Vec<i64>`, a `Vec` of a `FlussoValue` type), or over a `FlussoValue` array newtype | an array of values, the other kind | `#[derive(FlussoValue)]` on the newtype |
| `FlussoFragment` over a `Vec` of maps or of `serde_json::Value` | opaque JSON | the `Vec` itself, at a `json` field |
| `FlussoValue` over a `Vec` of a fragment or of a map type | not a leaf value | `Vec<Struct>` or a `FlussoFragment` newtype; a `Vec` of maps at a `json` field |

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

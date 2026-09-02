# Enums and custom values

Use your own Rust types as query values and struct fields: newtypes inherit their inner type's kinds, enums declare a string kind, and an ordered schema enum sorts by declared order.

## FlussoValue

`FlussoValue<K>` is what a handle accepts for kind `K`; it requires `serde::Serialize`. `#[derive(FlussoValue)]` implements it for your types.

**A newtype inherits its inner type's kinds.** No tag needed.

```rust
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize, FlussoValue)]
struct Money(Decimal);            // a decimal value

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
struct Sku(String);               // a keyword and text value

Customer::balance().gte(Money(Decimal::ONE));
```

**An enum declares a string kind**, `#[flusso(keyword)]` or `#[flusso(text)]`; there is no default, and numeric or date tags don't exist (use a newtype). It's matched against its serde string form.

```rust
#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[serde(rename_all = "camelCase")]
#[flusso(keyword)]
enum Tier { Pro, Enterprise, Free }

Customer::tier().eq(Tier::Pro);   // term "pro"
```

Keep enum keyword fields typed in the struct; never `#[flusso(skip)]` them.

## Variant coverage

When a `FlussoValue` enum sits in a struct field whose schema declares `variants:`, the derive checks them. A Rust variant the schema doesn't list is a compile error, since it could never match a document. Covering only some schema variants is fine: a partial projection.

`#[flusso(keyword, exhaustive)]` flips that: every embedding must cover the schema's **whole** declared set, and a missing variant is a compile error naming the list. It's enum-only (an untagged newtype inherits it from its inner type), and it hard-errors on a field that declares no `variants:` at all, so a schema edit can't silently disarm it.

```rust
// Schema: variants: [free, pro, enterprise]
#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[serde(rename_all = "camelCase")]
#[flusso(keyword, exhaustive)]
enum Tier { Free, Pro, Enterprise }   // drop one and the build fails
```

## The Enum handle

A keyword field whose schema declares `variants` gets the `Enum` handle instead of `Keyword`: the same value operators (`.keyword()` exposes the full keyword surface), but `.asc()`/`.desc()` sort on the prebaked `{field}.sort` subfield. A plain field sort, no script, by declared order. A bare enum with no `variants` is a `Keyword`.

## Features

| Feature | Adds |
| --- | --- |
| `decimal` | `rust_decimal::Decimal` re-exported and implemented as `FlussoValue<kind::Decimal>`. A `decimal` field takes `Decimal` or a lossless integer, never `f64`. Query precision is `f64`-bound (JSON has no arbitrary precision). |
| `uuid` | `uuid::Uuid` as a keyword value: `Customer::owner_id().eq(some_uuid)` with no `.to_string()`, and `Uuid` struct fields with no skip. |
| `time`, `chrono` | the date types the derive expects for `date`/`timestamp` fields and accepts in `Date` operators |

## Related

- [Field types](../reference/field-types.md#enum) for `variants` on the schema side.
- [Document structs](document-structs.md#flusso-types-to-rust-types).

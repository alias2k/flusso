# Query overview

`flusso-query` is the read side: a typed OpenSearch client whose derives validate your document structs against the flusso schema at compile time and generate a query surface where a wrong field, type, or operator is a compile error.

## The model

flusso's schema is a contract. It fixes every document's shape: which fields exist, their types, which are nested arrays. flusso enforces it on the write side. `flusso-query` enforces it on the read side, at `cargo build`, with no database.

The document struct is written by hand and stays under the caller's control. `#[derive(FlussoRoot)]` then does two things against the resolved mapping:

1. **Validates** every struct field: it exists, its Rust type matches, its nullability matches. A drifted struct stops compiling, pointing at the field.
2. **Generates the typed query surface** from the whole schema: a handle per field at every level, `get`/`query`, the schema hash. Operators exist only on the handles whose type supports them.

The surface comes from the schema, not the struct, so a query can filter or sort on a field the struct never deserializes.

## One root, many fragments

Exactly one type binds to an index: the **root**, `#[derive(FlussoRoot)]`. It is the only thing that reads `flusso.toml`, and it owns the whole handle tree for its index.

Every other shape is a **fragment**, `#[derive(FlussoFragment)]`: no index, no path. It describes a shape and nothing else, so one declaration serves every place that shape appears, and each root that embeds it validates it at that path.

```rust
#[derive(serde::Deserialize, FlussoFragment)]
struct Address { city: String, zip: Option<String> }

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    billing_address:  Address,     // checked against users.billingAddress
    shipping_address: Address,     // checked against users.shippingAddress
}
```

Retype one of `Address`'s fields and you get two errors, one per embedding.

## A query, end to end

```rust
use flusso_query::{Client, FlussoRoot};
use flusso_user_query::Orders;              // the generated scope for users.orders

let client = Client::connect("https://localhost:9200")?
    .basic_auth("admin", std::env::var("OS_PASSWORD")?);

let page = User::query()
    .query(User::full_name().matches("ada lovelace"))       // text → analyzed
    .filter(User::email().eq("ada@example.com"))            // keyword → exact
    .filter(User::order_count().gte(5))                     // long → range
    .filter(User::account().tier().eq("gold"))              // object, chained
    .filter(User::orders().any(Orders::status().eq("delivered")))  // nested, lifted
    .sort(User::order_count().desc())
    .size(20)
    .send(&client)
    .await?;

for hit in page.hits {
    let user: &User = &hit.source;
}
```

What won't compile: `User::email().matches(..)` (a text operator on a keyword), `User::full_name().eq(..)` (exact match on analyzed text), `User::nmae()`, and a struct declaring `email: Option<String>` when the schema says required.

## Setup

```toml
[dependencies]
flusso-query = { version = "…", features = ["derive"] }
```

Optional features: `decimal` (`rust_decimal::Decimal` as a decimal value), `uuid` (`uuid::Uuid` as a keyword), and a date crate, `time` or `chrono`. The crate targets OpenSearch and Elasticsearch 7.x, which share the query DSL.

## Where to next

| Job | Page |
| --- | --- |
| write the structs | [Document structs](document-structs.md) |
| know which operators a field has | [Field handles](field-handles.md) |
| combine clauses, set options | [Composing queries and options](composing.md) |
| filter by or shape a nested array | [Nested collections](nested.md) |
| sort, including across nesting | [Sorting](sorting.md) |
| dynamic-key fields | [Maps](maps.md) |
| Rust enums and newtypes as values | [Enums and custom values](enums-and-values.md) |
| several indexes in one call | [Several indexes](several-indexes.md) |
| the response, and the DSL hatch | [Results and the escape hatch](results-and-escape-hatch.md) |
| how the derive finds and checks the schema | [Binding to the schema](binding.md) |
| coming from `path = "…"` | [Migrating from path](migrating-from-path.md) |

# flusso-query — the typed query client

A typed OpenSearch client for indexes flusso maintains. `#[derive(FlussoRoot)]` validates your document struct against the flusso schema at compile time, with no database, and generates a query surface where a wrong field, type, or operator is a compile error.

flusso keeps OpenSearch in sync with Postgres from a declarative schema (the write side). That schema is a contract fixing every document's shape. flusso enforces it when writing; this crate enforces it when reading.

## One root, many fragments

Exactly one type binds to an index: the **root**. It reads `flusso.toml`, resolves the index's mapping, and owns the whole handle tree. Everything below is a **fragment**: a shape with no location, validated wherever a root embeds it.

```rust
use flusso_query::{Client, FlussoFragment, FlussoRoot};
use flusso_user_query::Orders;                      // the generated scope for users.orders

#[derive(Debug, serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]                          // the only input
struct User {
    id: i32,
    email: String,                                  // keyword, required → String
    #[serde(rename = "fullName")]
    full_name: Option<String>,                      // text, nullable → Option
    orders: Vec<Order>,                             // has_many → Vec
    #[serde(rename = "orderCount")]
    order_count: i64,                               // count → long
}

#[derive(Debug, serde::Deserialize, FlussoFragment)]
struct Order { status: String, total: f64 }

async fn busy_ada(client: &Client) -> anyhow::Result<()> {
    let page = User::query()
        .query(User::full_name().matches("ada"))                    // text → analyzed
        .filter(User::email().eq("ada@example.com"))                // keyword → exact
        .filter(User::order_count().gte(5))                         // long → range
        .filter(User::orders().any(Orders::status().eq("delivered")))  // nested, lifted
        .sort(User::order_count().desc())
        .size(20)
        .send(client)
        .await?;
    for hit in page.hits {
        println!("{} ({} orders)", hit.source.email, hit.source.order_count);
    }
    Ok(())
}
```

`User::email().matches(..)` doesn't compile (a text operator on a keyword). Neither does `email: Option<String>` when the schema says required, nor a field the schema doesn't have.

## What the field type buys you

| Handle | Operators |
| --- | --- |
| `Keyword` | `eq`, `any_of`, `prefix`, `wildcard`, `regexp`, `fuzzy`, `exists` |
| `Enum` | keyword ops, plus `asc`/`desc` by the schema's declared order |
| `Text` | `matches`, `match_phrase`, `match_phrase_prefix`, `matches_fuzzy`, `exists`; no `eq` |
| `Bool`, `Number<K>`, `Date` | `eq`, `any_of`, ranges, `exists` |
| object namespace | chains: `User::account().tier()` |
| `Nested<S, T>` | `any`/`all` to filter parents, `matching` to shape the array |
| `Geo` | `within`, `within_box`, `within_polygon`, distance sorts |
| `TextMap` / `KeywordMap` / `NumberMap` / `DateMap` | `key(k)` → a typed leaf; `search` across keys on text maps |

Values are typed by **kind**: numerics widen losslessly (`eq(5)` on a `long`, never a float on an integer), and a `#[derive(FlussoValue)]` newtype or enum queries with no cast. Subfield accessors (`.keyword()`, `.keyword_lowercase()`, `.text()`) exist only where the sink provisioned the subfield.

## Features

| Feature | Adds |
| --- | --- |
| `derive` | `FlussoRoot`, `FlussoFragment`, `FlussoValue`, `FlussoMap`, `FlussoMultiDocument` |
| `decimal` | `rust_decimal::Decimal` as a decimal value |
| `uuid` | `uuid::Uuid` as a keyword value |
| `time`, `chrono` | the date types for `date`/`timestamp` fields |

Targets OpenSearch and Elasticsearch 7.x, which share the query DSL.

## Learn more

The manual's **Query** part is the full guide: [overview](https://alias2k.github.io/flusso/query/overview.html), [document structs](https://alias2k.github.io/flusso/query/document-structs.html), [field handles](https://alias2k.github.io/flusso/query/field-handles.html), [composing and options](https://alias2k.github.io/flusso/query/composing.html), [nested collections](https://alias2k.github.io/flusso/query/nested.html), [sorting](https://alias2k.github.io/flusso/query/sorting.html), [maps](https://alias2k.github.io/flusso/query/maps.html), [enums and custom values](https://alias2k.github.io/flusso/query/enums-and-values.html), [several indexes](https://alias2k.github.io/flusso/query/several-indexes.html), [results and the escape hatch](https://alias2k.github.io/flusso/query/results-and-escape-hatch.html), [binding to the schema](https://alias2k.github.io/flusso/query/binding.html), and [migrating from `path`](https://alias2k.github.io/flusso/query/migrating-from-path.html). The API surface is on [docs.rs](https://docs.rs/flusso-query).

`flusso-query-derive` is the proc-macro crate behind the `derive` feature; don't depend on it directly. Both crates sit above the schema vocabulary and depend on nothing from the engine, sources, or sinks.

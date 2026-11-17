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
| `TextMap` / `KeywordMap` / `NumberMap` / `DateMap` | `key(k)` → the plain-field handle of the value kind; the same operators across every key (`matches`, `eq`, `between`, …) with `prefer(key, weight)` / `only_preferred()`; `contains` (part of a value, any key) on string maps |

Values are typed by **kind**: numerics widen losslessly (`eq(5)` on a `long`, never a float on an integer), and a `#[derive(FlussoValue)]` newtype or enum queries with no cast. Subfield accessors (`.keyword()`, `.keyword_lowercase()`, `.text()`) exist only where the sink provisioned the subfield.

## Fields that depend on the clock or the caller

What a row decides on its own, flusso stores. What needs a request-time value is declared on the root and decided when a document is read:

```rust
#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "contracts", context = Viewing)]
struct Contract {
    id: String,
    #[flusso(rules = visible_state)]
    state: ContractState,
}

fn visible_state(cx: &Viewing) -> Rules<ContractState> {
    let doc = Contract::doc();
    Rules::when(doc.signed_at().missing()).then(ContractState::Draft)
        .when(doc.ends_at().lt(cx.now)).then(ContractState::Closed)
        .otherwise(doc.state())
}

Contract::query(&cx)
    .filter(Contract::state(&cx).eq(ContractState::Closed))
    .sort(Contract::state(&cx).asc())
```

One rule list gives the value, the filter, the sort and the counts, with no script; a `#[flusso(computed = …)]` field is any Rust function, value only. See [Read-time fields](https://alias2k.github.io/flusso/query/read-time-fields.html).

## Features

| Feature | Adds |
| --- | --- |
| `derive` | `FlussoRoot`, `FlussoFragment`, `FlussoValue`, `FlussoMap`, `FlussoMultiDocument` |
| `decimal` | `rust_decimal::Decimal` as a `decimal` or `double` value |
| `uuid` | `uuid::Uuid` as a keyword value |
| `rc` | `Arc<str>` and `Rc<str>` as string values, like `String` (turns on serde's `rc`); `Box<str>` and `Cow<str>` need no feature |
| `time`, `chrono` | the date types for `date`/`timestamp` fields |
| `diesel`, `diesel-async` | `position::diesel::current` / `position::diesel_async::current`: read the position a fenced read waits on |

Always on, not a feature of this crate: `serde_json`'s `float_roundtrip`. A double in `_source`, and a sort value a page cursor carries back, is read as the value flusso wrote; serde_json's default parser reads about one in three random doubles one unit in the last place off. Cargo features unify, so this turns `float_roundtrip` on for your whole build: every float `serde_json` parses there is correctly rounded, at a cost (measured: a float-only array about 30% slower, about 12 ns a float; a typed `_search` decode of 1,000 hits with 14 floats each 7–12% slower; integers and strings unchanged; about 46 KB more code). It is not optional because a cursor resumed from a misread sort value can skip or repeat hits.

## Paging

Offset paging (`.from(n).size(k)`) re-runs the query each page, so CDC inserts and deletes shift it. For infinite scroll or an export, page by **cursor**: each page resumes after the last hit, so inserts and deletes never shift the walk, with no depth limit and nothing held on the server. `.point_in_time()` pins a walk to one fixed view instead.

```rust,ignore
let page = User::query().size(50).cursor(params.cursor).send(client).await?;
// page.next: Option<PageCursor> — an opaque string; send it back for the next page
```

`.hits(&client)` (or `.pages`, `.id_stream`) streams the whole walk as one `futures::Stream`, fetching each page as the previous one is consumed; see [streaming a walk](https://alias2k.github.io/flusso/query/streaming.html).

See [paging](https://alias2k.github.io/flusso/query/paging.html) in the manual.

Targets OpenSearch and Elasticsearch 7.x, which share the query DSL ([pinned cursor walks](https://alias2k.github.io/flusso/query/paging.html#pinned-walks) are OpenSearch-only).

## Reading your own writes

The index trails the database by a moment, so a read right after a commit can miss it. Point the client at flusso as well, read the database's position after the commit, and read through `client.after(..)`: a fetch by id waits until the write is applied, a search until it is searchable.

```rust,ignore
let client = Client::connect("https://opensearch:9200")?.flusso("http://flusso:9464")?;

let position = flusso_query::position::diesel_async::current(&mut conn).await?;
let user = User::get(&client.after(&position), id).await?;
```

Reads still go straight to OpenSearch; flusso is only asked to wait. A fence rides out a restart of flusso (a deploy), retrying until its `fence_timeout` (20 s by default), and never falls back to a stale read: it fails with `Error::FenceTimeout` when flusso was waiting on the position, `Error::FenceUnavailable { cause, .. }` when flusso couldn't be asked, `Error::FenceMisconfigured` at once when it can't be asked as configured. See [reading your own writes](https://alias2k.github.io/flusso/query/read-your-writes.html).

## Learn more

The manual's **Query** part is the full guide: [overview](https://alias2k.github.io/flusso/query/overview.html), [document structs](https://alias2k.github.io/flusso/query/document-structs.html), [field handles](https://alias2k.github.io/flusso/query/field-handles.html), [composing and options](https://alias2k.github.io/flusso/query/composing.html), [nested collections](https://alias2k.github.io/flusso/query/nested.html), [sorting](https://alias2k.github.io/flusso/query/sorting.html), [maps](https://alias2k.github.io/flusso/query/maps.html), [enums and custom values](https://alias2k.github.io/flusso/query/enums-and-values.html), [several indexes](https://alias2k.github.io/flusso/query/several-indexes.html), [results and the escape hatch](https://alias2k.github.io/flusso/query/results-and-escape-hatch.html), [paging](https://alias2k.github.io/flusso/query/paging.html), [streaming a walk](https://alias2k.github.io/flusso/query/streaming.html), [reading your own writes](https://alias2k.github.io/flusso/query/read-your-writes.html), [binding to the schema](https://alias2k.github.io/flusso/query/binding.html), and [migrating from `path`](https://alias2k.github.io/flusso/query/migrating-from-path.html). The API surface is on [docs.rs](https://docs.rs/flusso-query).

`flusso-query-derive` is the proc-macro crate behind the `derive` feature; don't depend on it directly. Both crates sit above the schema vocabulary and depend on nothing from the engine, sources, or sinks.

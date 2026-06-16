---
name: flusso-query
description: Query a flusso-maintained OpenSearch index from Rust with the `flusso-query` crate and `#[derive(FlussoDocument)]`. Use when writing or editing read-side code against a flusso index — typed document structs, the compile-time-checked query surface, nested filtering, custom value types, multi-index search. Trigger on flusso-query / FlussoDocument / FlussoValue / FlussoMultiDocument work.
---

# Querying a flusso index (`flusso-query` + the derive)

flusso owns the **write** side: it builds an OpenSearch index to match the schema. `flusso-query` is the **read** side — a typed OpenSearch/Elasticsearch query client. Reads go **straight to OpenSearch**, not through flusso (the engine is write-only).

The contract is the schema. `#[derive(FlussoDocument)]` reads the resolved schema **at compile time, with no database**, and:

1. **Validates** your hand-written struct against the schema — field exists, leaf Rust type matches, nullability matches. A drifted struct **stops compiling**.
2. **Generates the typed query surface** — one field handle per *schema* field (not just the ones you project), `get`/`query` entry points, and the schema hash that names the physical index.

You write and own the struct (a **projection** — deserialize the subset you want). The query surface covers the **whole schema**, so you can filter/sort on fields the struct never deserializes.

## Crates and features

- `flusso-query` — the runtime: `Client`, field handles, `Query`/`Search`, `SearchResponse`. Re-exports the derive behind the **`derive`** feature, so you `use flusso_query::FlussoDocument;`.
- `flusso-query-derive` (`apps/query-derive`) — the proc-macros: `FlussoDocument`, `FlussoMultiDocument`, `FlussoValue`.
- Optional features: **`derive`** (the macros), **`decimal`** (`rust_decimal::Decimal`), **`chrono`** / **`time`** (date leaf types — pick one, or use `String` for raw ISO-8601).

## The shape of a consumer

```rust
use flusso_query::{Client, FlussoDocument};

// You write this. A projection of the `users` index. The derive checks every
// field against the schema and hangs the query surface off `User`.
#[derive(Debug, Clone, serde::Deserialize, FlussoDocument)]
#[flusso(index = "users")]              // the only required input: which index
pub struct User {
    pub id: i32,                        // primary key (integer) → never null
    pub email: String,                  // keyword, required → never null
    #[serde(rename = "fullName")]
    pub full_name: Option<String>,      // text, not required → nullable
    pub orders: Vec<Order>,             // has_many join → nested, never null
    #[serde(rename = "orderCount")]
    pub order_count: i64,               // count aggregate → long, never null
}

// A nested/child struct names its dotted PATH in the same index. It contributes
// field validation + handles, but no entry points of its own.
#[derive(Debug, Clone, serde::Deserialize, FlussoDocument)]
#[flusso(index = "users", path = "orders")]
pub struct Order {
    pub status: String,                 // enum → keyword
    pub total: f64,                     // decimal → double (lossy; see type table)
}
```

```rust
let client = Client::connect("https://localhost:9200")?
    .basic_auth("admin", std::env::var("OS_PASSWORD")?);

let user: Option<User> = User::get(&client, 42).await?;     // by primary key

let page = User::query()                                     // client-free value
    .filter(User::email().eq("ada@example.com"))             // keyword → exact
    .filter(User::order_count().gte(5))                      // long → range
    .query(User::full_name().matches("ada lovelace"))        // text → analyzed
    .filter(User::orders().any(Order::status().eq("delivered")))  // nested, lifted
    .sort(User::order_count().desc())
    .from(0).size(20)
    .send(&client).await?;

for hit in page.hits {                  // hit.id, hit.score from the envelope;
    let u: &User = &hit.source;         // hit.source is a fully-typed User
}
```

See `examples/consumer.rs` for a fuller worked file.

## How the derive binds to the schema (no DB, no codegen file)

`#[flusso(index = "users")]` is the only input. At compile time the macro:

1. Walks **up from `CARGO_MANIFEST_DIR`** to find `flusso.toml` (like cargo finds `Cargo.toml`). Override with `#[flusso(config = "…")]` or the `FLUSSO_CONFIG` env var.
2. Selects the `[[index]]` whose `name` matches — which is why an index name is required.
3. Loads that index's `schema:` file and resolves the `IndexMapping` in-process — the **same** resolution `flusso build` performs. Self-describing schemas make this hermetic.
4. Tracks `flusso.toml` + every schema file as build inputs, so editing config/schema retriggers compilation and a drifted struct fails the next build.

The resolved schema's content hash is `User::SCHEMA_HASH`, and `User::INDEX` is the physical name `users_<hash>` — the exact index the sink writes. So `get`/`query` address the right index directly; **no read alias needed**, and a structural schema change rotates the hash and forces a recompile.

## What each field type lets you write (the type safety that matters)

An operator that doesn't fit a field's type **doesn't exist** on its handle — the mistake is a compile error, not a 400 from OpenSearch.

| Handle | Operators |
| --- | --- |
| `Keyword` | `eq` `in_` `prefix` `wildcard` `regexp` `fuzzy` `exists` |
| `Text` | `matches` `match_phrase` `match_phrase_prefix` `matches_fuzzy` `exists` — **no exact `eq`** (analyzed) |
| `Bool` | `eq` `exists` |
| `Number<T>` | `eq` `in_` `lt` `lte` `gt` `gte` `between` `exists` |
| `Date` | `eq` `lt` `lte` `gt` `gte` `between` `exists` |
| `Object<S>` | `exists` only (same-doc sub-object / to-one join). Query its sub-fields via the **child struct's** flattened handles (`Account::tier()`), not by chaining off this handle. |
| `Nested<S,T>` | `any(q)` / `all(q)` to match parents and **lift** a child query into scope `S`; `matching(q)` (+ `.sort/.size/.from`) to shape the returned array; `exists` |
| `Geo` | `within(distance, center)` `in_bounding_box` `in_polygon` `exists`; `distance_sort(...)` |
| `Binary` | `exists` (base64, not searchable) |
| `Json` | `exists` `raw(serde_json::Value)` |

`sort(…)` only accepts sortable handles (numbers, dates, keywords, bools) — `sort` on a `text` field is a compile error. Cross-field: `multi_match("ada", [User::full_name(), User::bio()])`. Anything outside the typed surface (`knn`, `function_score`, `script`, `geo_shape`) → the [`raw`](#escape-hatch) hatch.

## Composing — scope is in the type

A handle's operator produces `Query<S>`, carrying the **scope** `S` it was built in. The root and any flattened `object`/to-one join share `Root` (`Query<Root>`); a **`nested` array introduces a fresh scope tagged with the element struct** (`Order::status()` → `Query<Order>`).

```rust
// within a scope: and / or / not
let q = User::email().eq("ada@x.io").and(User::order_count().gte(5));

// clause style — filter/must_not don't score; query(=must)/should do
User::query()
    .query(User::full_name().matches("ada"))    // scored
    .filter(User::order_count().gte(5))          // filtered, cached, no score
    .must_not(User::email().prefix("test-"))
    .should(User::orders().any(Order::status().eq("delivered")))
    .send(&client).await?;
```

`User::email().and(Order::status().eq(…))` **does not compile** — you can't `and` a `Query<Root>` with a `Query<Order>`. Lift the child first: `User::orders().any(child)` takes a `Query<Order>` → returns `Query<Root>`. Lifting composes through depth: `Order::items().any(Item::quantity().gt(1))` is `Query<Order>`, which `User::orders().any(…)` lifts to `Query<Root>`.

**Queries are values, the client appears once.** `Type::query()` takes no client — `Search<T>` is a plain `Clone` value. Build it in a helper, store it, reuse it; hand `&Client` to a terminal when running:

```rust
fn busy_users() -> flusso_query::Search<User> {
    User::query().filter(User::order_count().gte(5))
}
let page = busy_users().send(&client).await?;
let next = busy_users().from(20).send(&client).await?;
```

**Terminals:** `.send(&client)` → `SearchResponse<T>`; `.count(&client)` → `u64` (no fetch/score); `.ids(&client)` → `Vec<String>` (matching ids, `_source: false`).

**Optional filters:** `Option<Q>` is itself a `Query` — `None` adds nothing. So `.filter(params.email.map(|e| User::email().eq(e)))` just drops out when absent.

## Nested collections — filter *by* vs filter *of*

Two independent things, deliberately separate:

- **Filter BY** — which *parents* return, based on children: `User::orders().any(...)` / `.all(...)`. A matching parent still carries its **whole** array. It's a `Query`, so it goes in `filter`/`query`/etc.
- **Filter OF** — shape the array each parent returns, without changing which parents match: `.filter_nested(User::orders().matching(q).sort(...).size(...))`.

```rust
let page = User::query()
    .filter(User::orders().any(Order::status().eq("delivered")))   // BY
    .filter_nested(                                                // OF
        User::orders().matching(Order::status().eq("delivered"))
            .sort(Order::placed_at().desc()).size(5),
    )
    .send(&client).await?;

for hit in &page.hits {
    for order in &hit.source.orders { /* delivered, newest first, ≤5 */ }
}
```

By default `filter_nested` **replaces** `hit.source.<path>` with the matched subset (read it straight off the struct). A parent with no matches still returns, with `[]`. (`keep_source()` + the typed `hit.nested(handle)` side-accessor are deferred in v1.)

## Multi-index

- **One blended list** — `#[derive(FlussoMultiDocument)]` on an enum with one single-field tuple variant per document type. `StoreItem::query()…send(&client)` ranks hits together; dispatch by `hit.source` match. Purely syntactic (no schema resolution); validates enum shape + no duplicate payload types. A *sort* on a field not in every index needs `unmapped_type` — sort by relevance or shared fields.
- **Several searches, one round-trip** — `client.msearch((&q1, &q2))` (tuple arity 1–8) → one typed `SearchResponse` per slot, in order. `client.msearch_all(&searches)` for many of one type.

## Custom value types — `#[derive(FlussoValue)]`

Let a scalar field be your own enum/newtype instead of a bare leaf:

```rust
#[derive(serde::Deserialize, serde::Serialize, FlussoValue)]
#[flusso(keyword)]                       // kind: keyword (default) | text | number | date
enum AccountTier { Free, Pro, Enterprise }
```

Then `Account::tier().eq(AccountTier::Pro)` works (`String`/`&str` still do). Kind rules: keyword/text accept a unit enum **or** a newtype; number/date accept a **newtype only**. Query-value wiring is currently keyword-only (`eq`/`in_`); number/date custom types generalize the **doc side** only. A missing `FlussoValue` impl gives a precise "`T` is not a valid value for a `kind::Keyword` field" error.

## flusso type → Rust type (what the derive expects)

| flusso `type` | Rust | Handle |
| --- | --- | --- |
| `text` / `identifier` | `String` | `Text` |
| `keyword` / `enum` | `String` | `Keyword` |
| `uuid` | `String` (or `uuid::Uuid`, feature) | `Keyword` |
| `boolean` | `bool` | `Bool` |
| `short`/`integer`/`long` | `i16`/`i32`/`i64` | `Number<T>` |
| `float`/`double` | `f32`/`f64` | `Number<T>` |
| `decimal` | `f64` *(lossy)* | `Number<f64>` |
| `date` | `time::Date` / `chrono` (feature) | `Date` |
| `timestamp` | `time::OffsetDateTime` / `chrono` | `Date` |
| `binary` | `String` (base64) | `Binary` |
| `json` | `serde_json::Value` | `Json` |
| `geo` | `GeoPoint { lat, lon }` | `Geo` |
| `object` / `belongs_to` / `has_one` | struct / `Option<struct>` | `Object` |
| `has_many` / `many_to_many` | `Vec<struct>` | `Nested<S,T>` |

Matching is by **leaf identifier + `Option` shape** — the macro compares the final type segment, not aliases. Exact money: declare a `custom` `scaled_float` in the schema and the derive accepts `rust_decimal::Decimal` (with the `decimal` feature).

## Nullability is checked, not guessed

`T` vs `Option<T>` must match the schema. Non-null: root/join `primary_key`, `required: true` leaf, `object`/group, `count`, to-many joins (empty `Vec`, never null). Nullable: `required: false` leaf, `belongs_to`/`has_one`, `avg`/`sum`/`min`/`max`. Declaring the wrong shape is a derive compile error.

Escape hatches from validation: a `serde_json::Value` field skips type-checking; `#[flusso(skip)]` drops a field entirely (pair with `#[serde(skip)]`/`#[serde(default)]`).

## <a id="escape-hatch"></a>The raw escape hatch

```rust
User::query().raw(serde_json::json!({
    "function_score": { "query": { "match_all": {} }, "random_score": {} }
})).send(&client).await?;     // still deserializes into SearchResponse<User>
```

## Out of scope (v1)

Search aggregations/facets (use `raw`), writes (flusso owns the index — query-only by construction), cross-index hit correlation, scroll/`search_after` deep pagination.

## Working reference

`dev/search-api` (crate `flusso-dev-search-api`, axum) derives `FlussoDocument` for users/products/orders, plus `FlussoMultiDocument` (`/search`) and `msearch` (`/overview`). Read it for a real consumer — but in an exported project, validate against your own `flusso.toml`, not `dev/`.

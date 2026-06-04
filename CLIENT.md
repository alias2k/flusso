# flusso client (endgame)

> [!NOTE]
> This is a **target document**, not a description of code that exists. It writes
> the client crate's API the way we wish we could call it, so the implementation
> has a fixed point to aim at. Everything below is aspirational; nothing here is
> built yet.

flusso keeps an OpenSearch index in sync with Postgres from a declarative
schema. That schema is a contract: it fixes the shape of every document in the
index — which fields exist, their types, which are nested arrays, which are
scalars. Today that contract is enforced on the **write** side (the engine
builds the index to match) but not on the **read** side: anyone querying the
index hand-writes OpenSearch JSON and hand-deserializes the results, with
nothing checking either against the schema.

`flusso-client` closes that gap. It turns the same schema into Rust types, so a
downstream service queries the index the way it would call a typed function:
field names checked at compile time, operators that only exist for the field
types that support them, and results that deserialize into a struct whose shape
*is* the document. When the schema changes and the index is rebuilt, the
generated code changes with it, and any query that no longer fits stops
compiling — the drift surfaces at `cargo build`, not in production.

---

## The shape of it, from a caller's seat

A service that searches the `users` index from [`SCHEMA.md`](SCHEMA.md) — its
whole interaction with flusso:

```rust
use flusso_client::Client;
use myapp::index::users::{self, User};   // generated — see "Generating bindings"

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Transport. Points at OpenSearch, not at flusso — the engine is write-only;
    // reads go straight to the index it maintains.
    let client = Client::connect("https://localhost:9200")?
        .basic_auth("admin", std::env::var("OS_PASSWORD")?);

    // Fetch one document by its id. `id` is typed as the root table's primary
    // key (i64 here), and the result is Option<User> — None when absent.
    let user: Option<User> = users::get(&client, 42).await?;

    // A typed search. Every `users::field()` is a handle that only exposes the
    // operators its mapping supports (see "What the field type buys you").
    let page = users::search(&client)
        .filter(users::email().eq("ada@example.com"))   // keyword → exact
        .filter(users::order_count().gte(5))            // integer → range
        .query(users::name().matches("ada lovelace"))   // text  → analyzed match
        .filter(users::orders().any(|o| o.status().eq("paid")))  // nested
        .sort(users::order_count().desc())
        .from(0)
        .size(20)
        .send()
        .await?;

    println!("{} total matches", page.total);
    for hit in page.hits {
        // hit.source is a fully-typed User. hit.id and hit.score come from the
        // search envelope, not the document body.
        let u: &User = &hit.source;
        println!("{:.3}  {}  ({} orders)", hit.score, u.email, u.order_count);
        for order in &u.orders {                         // Vec<UserOrders>
            println!("    order {} — {} — {}", order.id, order.total, order.status);
        }
    }

    Ok(())
}
```

The whole point is what you *can't* write. `users::email().matches(..)` does not
compile — `matches` is a text operator and `email` is a `keyword`.
`users::nmae()` does not compile — there is no such field. `hit.source.totl`
does not compile. None of these are runtime errors; the schema has been lifted
into the type system, so the compiler is checking your queries against it.

---

## Generating bindings

The contract the client generates from is the **resolved mapping**:
flusso's [`IndexMapping`](libs/0-schema/0-core/src/config/index_mapping.rs) —
every field with a concrete `MappingType`, whether it is **nullable**, and its
nested `children`, plus the schema `hash`. The engine already computes this to
create the index up front, so the client is reusing the engine's own typed view
of the document rather than re-deriving one.

> `ResolvedField` carries a `nullable: bool` (already implemented): the shared
> resolver in `sources-core` derives it while resolving the mapping, asking the
> source only about plain columns. See
> [Nullability](#nullability-comes-from-the-source) — it is not something the
> client should guess.

A field carries an explicit type only when the schema gives it a `mapping:`.
Where it doesn't, the engine fills the type in from the database column. So the
resolved mapping — the thing that is fully typed — is produced *against the
source*. The client gets at it one of two ways:

1. **Committed artifact (recommended).** `flusso` emits the resolved mapping as
   a deterministic JSON file you check in:

   ```sh
   flusso emit-mapping --config config.toml --index users > users.mapping.json
   ```

   A `build.rs` reads it and generates the module. The build is hermetic — no
   database, no network — and the artifact's content hash is the same one the
   engine folds into the physical index name, so binding and index are provably
   the same schema version. Regenerating after a schema change is a diff in
   version control.

   ```rust
   // build.rs
   fn main() {
       flusso_codegen::generate()
           .mapping("users.mapping.json")
           .out_module("users")
           .emit()                       // writes $OUT_DIR/users.rs
           .unwrap();
   }
   ```

   ```rust
   // src/index.rs
   pub mod users {
       include!(concat!(env!("OUT_DIR"), "/users.rs"));
   }
   ```

2. **Live introspection.** OpenSearch already holds the resolved mapping — flusso
   creates the index with explicit typed mappings — so for codegen-free or
   scripting use the client can read `GET <index>/_mapping` and build the typed
   surface at runtime. This is the escape hatch for tools that can't run a build
   step; the committed-artifact path is what production services should use,
   because only it gives compile-time checking.

Either way there is exactly one source of truth — the schema — and the bindings
are derived from it, never written by hand.

### What gets generated, for the `users` schema

```rust
pub mod users {
    /// One `users` document. Field order and names follow the schema; the doc
    /// keys (which may be camelCase) are preserved via serde rename.
    #[derive(Debug, Clone, serde::Deserialize)]
    pub struct User {
        pub id: i64,                         // primary key — never null
        pub email: String,                   // users.email is NOT NULL in the db
        pub name: Option<String>,            // users.name is nullable in the db
        pub orders: Vec<UserOrders>,         // a nested array — empty, never null
        #[serde(rename = "orderCount")]
        pub order_count: i32,                // count() aggregate — never null
    }

    /// The projection of each row folded in by the `orders` join.
    #[derive(Debug, Clone, serde::Deserialize)]
    pub struct UserOrders {
        pub id: i64,
        pub total: rust_decimal::Decimal,
        pub status: Option<String>,
    }

    // Field handles — one per field, carrying its type. These are what the query
    // builder consumes; see below. (Signatures shown; bodies are generated.)
    pub fn email() -> Keyword { /* … */ }
    pub fn name() -> Text { /* … */ }
    pub fn order_count() -> Number<i32> { /* … */ }
    pub fn orders() -> Nested<UserOrders> { /* … */ }

    // Entry points.
    pub fn get(client: &Client, id: i64) -> impl Future<Output = Result<Option<User>>>;
    pub fn search(client: &Client) -> Search<User>;

    /// The schema hash this module was generated from — asserted against the live
    /// index on first query so a stale binding fails loudly, not silently.
    pub const SCHEMA_HASH: &str = "3f2a1b9c…";
}
```

### Mapping types → Rust types

| `MappingType`                                   | Rust type                       | Field handle    |
| ----------------------------------------------- | ------------------------------- | --------------- |
| `Keyword`                                       | `String`                        | `Keyword`       |
| `Text`                                          | `String`                        | `Text`          |
| `Boolean`                                       | `bool`                          | `Bool`          |
| `Byte` / `Short` / `Integer` / `Long`           | `i8` / `i16` / `i32` / `i64`    | `Number<_>`     |
| `Float` / `HalfFloat`                           | `f32`                           | `Number<f32>`   |
| `Double`                                        | `f64`                           | `Number<f64>`   |
| `ScaledFloat`                                   | `rust_decimal::Decimal`         | `Number<Decimal>` |
| `Date`                                          | `time::OffsetDateTime` (feature)| `Date`          |
| `Object`                                        | generated struct (the children)| `Object<T>`     |
| `Nested`                                        | `Vec<` generated struct `>`     | `Nested<T>`     |
| `Other(s)`                                      | `serde_json::Value`             | `Json`          |

**Decimals** map to `rust_decimal::Decimal`, the same type flusso already carries
in [`GenericValue`](libs/0-schema/0-core/src/common/generic_value.rs) — no lossy
float round-trip. **Dates** are behind a feature so a caller picks `time` or
`chrono` (or `String` for raw ISO-8601) without the crate forcing a dependency.

### Nullability comes from the source

A field is `T` or `Option<T>`, and the client must **not** guess which — it
should know, because the resolved mapping records it. `ResolvedField` carries a
`nullable: bool`, derived during mapping resolution: the shared resolver in
`sources-core` applies the rules below, and the only thing it asks the source is
a plain column's intrinsic type and nullability (the Postgres source reads both
from one `pg_attribute` row — `format_type` and `attnotnull`). The client then
maps `nullable: false → T`, `nullable: true → Option<T>`, with no inference of
its own.

The resolver derives `nullable` per field source. Most of it is
source-independent; only the *plain column* row consults the source:

| Field source                       | `nullable`                                                              |
| ----------------------------------- | ----------------------------------------------------------------------- |
| primary-key column                  | `false` — it backs the document id                                      |
| column with a schema `default:`     | `false` — the engine coalesces null to the default                      |
| plain column                        | mirrors the column's `NOT NULL` — *the one row that consults the source* |
| `group` (`object`)                  | `false` — always assembled                                              |
| join, `one_to_one` (`object`)       | `true` — there may be no related row                                    |
| join, `one_to_many` / `many_to_many`| `false` — a `Vec`, empty when there are none, never null                |
| aggregate `count`                   | `false` — zero rows is `0`, not null                                    |
| aggregate `sum`/`avg`/`min`/`max`   | `true` — null over zero rows                                            |
| constant                            | `false`, unless the constant *is* `Null`                                |

This is why the derivation can't be a column-nullability lookup alone: a `count`
is never null, a to-many join is never null, a one-to-one join is nullable even
when its key column isn't. Only the resolver — which knows each field's *source*
— gets these right, and it lives in `sources-core` so every source shares it.
The read alias remains the one outstanding requirement the endgame places on the
engine.

---

## What the field type buys you

The handle a field generates to determines the operators in scope. This is the
type safety that matters: an operator that doesn't make sense for a field's
mapping *doesn't exist* on its handle, so the mistake is a compile error.

| Handle          | Operators                                                                 |
| --------------- | ------------------------------------------------------------------------- |
| `Keyword`       | `eq`, `in_`, `prefix`, `exists`                                           |
| `Text`          | `matches`, `match_phrase`, `exists` — *no* exact `eq` (it's analyzed)     |
| `Bool`          | `eq`, `exists`                                                            |
| `Number<T>`     | `eq`, `in_`, `lt`, `lte`, `gt`, `gte`, `between`, `exists`                |
| `Date`          | `eq`, `lt`, `lte`, `gt`, `gte`, `between`, `exists`                       |
| `Object<T>`     | field access into `T`'s handles (a same-document sub-object)              |
| `Nested<T>`     | `any(|t| …)` / `all(|t| …)` — a nested query over `T`'s handles, plus `exists` |
| `Json`          | `exists`, `raw(serde_json::Value)` — the untyped fallback                 |

Each operator's argument is typed too: `order_count().gte(_)` takes an `i32`,
`order_count().between(_, _)` takes two; `email().in_(_)` takes an
`IntoIterator<Item = impl Into<String>>`. Sorting is the same — `sort(…)` only
accepts handles whose mapping is sortable (numbers, dates, keywords, booleans),
so `sort(name().desc())` on a `text` field is a compile error, mirroring
OpenSearch's own refusal to sort un-`fielddata` text.

### Composing queries

Handles produce a `Query`; queries compose with `and` / `or` / `not`, and the
`Search` builder exposes the bool-query clauses directly for callers who want
score-vs-filter control:

```rust
// Combinator style.
let q = users::email().eq("ada@example.com")
    .and(users::order_count().gte(5))
    .and(users::orders().any(|o| o.status().eq("paid").and(o.total().gt(dec!(0)))));

users::search(&client).query(q).send().await?;

// Clause style — `filter`/`must_not` don't score, `query`(=must)/`should` do.
users::search(&client)
    .query(users::name().matches("ada"))          // scored
    .filter(users::order_count().gte(5))          // filtered, cached, no score
    .must_not(users::email().prefix("test-"))
    .should(users::orders().any(|o| o.status().eq("vip")))
    .send()
    .await?;
```

`query`/`filter`/`must_not`/`should` accept anything that is a `Query`, so the
two styles mix freely.

---

## Results

```rust
pub struct SearchResponse<T> {
    pub total: u64,            // total matches (not the page size)
    pub max_score: Option<f32>,
    pub hits: Vec<Hit<T>>,
    pub took: std::time::Duration,
}

pub struct Hit<T> {
    pub id: String,            // the document id (root primary key, stringified)
    pub score: f32,
    pub source: T,             // the fully-typed document
}
```

`get` returns `Option<T>`; `search` returns `SearchResponse<T>`. There is no
`serde_json::Value` in the common path — the typed struct is the result.

---

## Resolving the index name

Generated code knows the **logical** name (`users`) and its `SCHEMA_HASH`. The
**physical** index carries the hash suffix (`users_3f2a1b9c`) and rotates on a
structural schema change — so the client must not query the physical name
directly, or every reindex breaks it.

The contract: **flusso maintains a read alias from the logical name to the
current physical index.** The client queries `users`; flusso points `users` at
`users_<hash>` and atomically re-points it after a backfill into a new shape.
This is a requirement the endgame places on the **engine**, not just the client —
without the alias there is no stable name to query, and "where we need to arrive"
includes the engine owning that alias. On first query the client compares its
`SCHEMA_HASH` against the alias's current target and warns (or errors, by config)
on a mismatch, so a binding that has drifted from the deployed index says so
instead of silently returning the wrong shape.

---

## The escape hatch

Anything the typed builder can't express stays reachable, and still
deserializes into the typed struct:

```rust
let page: SearchResponse<User> = users::search(&client)
    .raw(serde_json::json!({
        "function_score": { "query": { "match_all": {} }, "random_score": {} }
    }))
    .send()
    .await?;
```

`raw` takes the OpenSearch query DSL verbatim and is the pressure-release valve
for geo queries, `function_score`, percolators, and anything else not yet in the
typed surface — without dropping to an untyped client or losing typed results.

---

## Explicitly out of scope for the first cut

So the target is unambiguous about where the line is:

- **Search aggregations** (facets, histograms, cardinality). The typed surface
  is filter/query/sort + typed hits first. Aggregations are a known, larger
  follow-on — they need their own typed result tree — and the `raw` hatch covers
  them in the meantime.
- **Writes.** flusso owns the index; the client never upserts or deletes. It is a
  query client by construction.
- **Cross-index / multi-index search.** One binding, one index. Joining across
  indexes is the caller's job, above this crate.
- **Scroll / `search_after` pagination.** `from`/`size` first; deep pagination is
  a follow-on once the typed cursor shape is settled.

---

## Where this lands in the workspace

A new consumer-facing crate plus a codegen helper, both reusing the existing
schema layer rather than re-implementing it:

| Crate            | Role                                                                                  |
| ---------------- | ------------------------------------------------------------------------------------- |
| `flusso-client`  | Runtime: the `Client` transport, the field-handle/`Query`/`Search` builder, `SearchResponse`. Generic over the generated document types. |
| `flusso-codegen` | Turns a resolved [`IndexMapping`](libs/0-schema/0-core/src/config/index_mapping.rs) into a Rust module — structs, field handles, entry points. Driven by `build.rs`. Depends on `schema-core`. |
| `flusso emit-mapping` | A CLI subcommand on the existing binary that writes the resolved mapping artifact codegen consumes. |

The numeric-layer rule still holds: both new crates sit above `schema-core` and
depend only downward. The client crate has no dependency on the engine, the
sources, or the sinks — it shares only the domain model, which is the one thing
the read and write sides must agree on.
```
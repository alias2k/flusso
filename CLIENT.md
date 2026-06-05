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

`flusso-client` closes that gap — but it does **not** generate the document
types for you. The developer writes the document struct by hand and keeps full
control over it: its derives, its field types, which fields it projects, how the
doc keys map to Rust names. A **derive macro** then does two things against the
resolved schema, at compile time:

1. **Validates** that every field the struct declares lines up with the schema —
   the field exists, its Rust type matches the field's mapping, and its
   nullability matches. A struct that has drifted from the schema *stops
   compiling*, pointing at the offending field.
2. **Generates the typed query surface** from the schema — field handles,
   `get`/`search` entry points, the schema hash — so a downstream service queries
   the index the way it would call a typed function: field names checked at
   compile time, operators that only exist for the field types that support them,
   and results that deserialize into the struct the developer wrote.

The query surface comes from the **full schema**, not from the struct — so you
can filter or sort on a field even if your struct doesn't deserialize it. The
struct is a projection you control; the schema is the contract both the struct
and the query surface are checked against. When the schema changes and the index
is rebuilt, the artifact the macro reads changes with it, and any struct or query
that no longer fits stops compiling — the drift surfaces at `cargo build`, not in
production.

---

## The shape of it, from a caller's seat

A service that searches the `users` index from [`SCHEMA.md`](SCHEMA.md). The
developer writes the structs; `#[derive(FlussoDocument)]` validates them and
generates the query surface:

```rust
use flusso_client::{Client, FlussoDocument};

/// One `users` document. *You* write this — pick the derives, the field types,
/// and which fields to project. The derive checks it against the schema and
/// hangs the typed query surface off `User` (see "Binding to the schema").
#[derive(Debug, Clone, serde::Deserialize, FlussoDocument)]
#[flusso(mapping = "users.mapping.json", index = "users")]
pub struct User {
    pub id: i64,                         // primary key — never null
    pub email: String,                   // users.email is NOT NULL in the db
    pub name: Option<String>,            // users.name is nullable in the db
    pub orders: Vec<UserOrders>,         // a nested array — empty, never null
    #[serde(rename = "orderCount")]
    pub order_count: i32,                // count() aggregate — never null
}

/// The projection of each row folded in by the `orders` join. A nested struct
/// validates against its path in the same mapping; it has no entry points of
/// its own (the root `User` owns those).
#[derive(Debug, Clone, serde::Deserialize, FlussoDocument)]
#[flusso(mapping = "users.mapping.json", path = "orders")]
pub struct UserOrders {
    pub id: i64,
    pub total: rust_decimal::Decimal,
    pub status: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Transport. Points at OpenSearch, not at flusso — the engine is write-only;
    // reads go straight to the index it maintains.
    let client = Client::connect("https://localhost:9200")?
        .basic_auth("admin", std::env::var("OS_PASSWORD")?);

    // Fetch one document by its id. `id` is typed as the root table's primary
    // key (i64 here), and the result is Option<User> — None when absent.
    let user: Option<User> = User::get(&client, 42).await?;

    // A typed search. Every `User::field()` is a handle that only exposes the
    // operators its mapping supports (see "What the field type buys you"). The
    // handles cover the *whole schema*, so `User::name()` exists for filtering
    // even though it's fine to have left `name` off a narrower projection.
    let page = User::search(&client)
        .filter(User::email().eq("ada@example.com"))   // keyword → exact
        .filter(User::order_count().gte(5))            // integer → range
        .query(User::name().matches("ada lovelace"))   // text  → analyzed match
        .filter(User::orders().any(|o| o.status().eq("paid")))  // nested
        .sort(User::order_count().desc())
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

The whole point is what you *can't* write. `User::email().matches(..)` does not
compile — `matches` is a text operator and `email` is a `keyword`.
`User::nmae()` does not compile — there is no such handle. `hit.source.totl`
does not compile. And — the new guarantee — the struct itself can't drift:
declaring `email: i32`, or `email: Option<String>` when the column is `NOT NULL`,
or a field `totl` that the schema doesn't have, is a **compile error from the
derive**, not a runtime surprise. None of these are runtime errors; the schema
has been lifted into the type system, so the compiler is checking both your
queries and your struct against it.

---

## Binding to the schema

The contract the macro validates against is the **resolved mapping**:
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
source*. flusso emits it as a deterministic JSON file you check in:

```sh
flusso emit-mapping --config config.toml --index users > users.mapping.json
```

The derive reads that artifact at compile time. The `mapping` attribute is a
path **relative to the crate's `CARGO_MANIFEST_DIR`**, so the build is hermetic —
no database, no network, just a file read. The macro forces a rebuild dependency
on the artifact (it folds the file in via `include_bytes!`, so editing
`users.mapping.json` retriggers compilation), and the artifact's content hash is
the same one the engine folds into the physical index name — so binding and index
are provably the same schema version. Regenerating after a schema change is a
diff in version control, and any struct that no longer matches fails to compile.

There is no `build.rs` and no generated `.rs` file to `include!` — the struct is
the file you maintain, and the derive expands in place.

### What the derive expands to

`#[derive(FlussoDocument)]` on the root `User` emits (roughly):

```rust
impl User {
    // Entry points.
    pub fn get(client: &Client, id: i64) -> impl Future<Output = Result<Option<User>>>;
    pub fn search(client: &Client) -> Search<User>;

    // Field handles — one per *schema* field, carrying its type. These are what
    // the query builder consumes; see "What the field type buys you". They exist
    // for every field in the mapping, whether or not `User` projects it.
    pub fn id() -> Number<i64> { /* … */ }
    pub fn email() -> Keyword { /* … */ }
    pub fn name() -> Text { /* … */ }
    pub fn order_count() -> Number<i32> { /* … */ }
    pub fn orders() -> Nested<UserOrdersFields> { /* … */ }

    /// The schema hash this binding was checked against — asserted against the
    /// live index on first query so a stale binding fails loudly, not silently.
    pub const SCHEMA_HASH: &str = "3f2a1b9c…";
}

// The nested query context handed to `orders().any(|o| …)`. Generated from the
// nested children in the schema — independent of `UserOrders`, so you can query
// nested fields you don't deserialize.
pub struct UserOrdersFields { /* … */ }
impl UserOrdersFields {
    pub fn id() -> Number<i64> { /* … */ }
    pub fn total() -> Number<Decimal> { /* … */ }
    pub fn status() -> Keyword { /* … */ }
}
```

The nested `UserOrders` struct's own derive (with `path = "orders"`) emits **no**
entry points or handles — the root owns the query surface. It only contributes
its field validation against the `orders` children of the mapping.

### What the derive checks

For each field the struct declares, the macro resolves the matching schema field
by its **document key** — honoring `#[serde(rename = "…")]` and a container
`#[serde(rename_all = "…")]` so the struct's serde config and flusso's validation
agree — then checks three things:

| Check                | Pass                                              | Compile error                                                        |
| -------------------- | ------------------------------------------------- | -------------------------------------------------------------------- |
| **field exists**     | the doc key is in the schema                       | `no field `totl` in index `users`` (span on the field)              |
| **type matches**     | leaf Rust type matches the field's `MappingType`   | `email is `keyword` → expected `String`, found `i32``               |
| **nullability matches** | `Option<_>` iff the field is `nullable`         | `email is NOT NULL → expected `String`, found `Option<String>``     |

The rules that make this **full control rather than a straitjacket**:

- **Partial projections are allowed.** Leaving schema fields off your struct is
  fine — you only deserialize the subset you declare. Omission is never an error;
  only the three checks above fail.
- **Type matching is by leaf identifier + `Option` shape.** The macro can't
  resolve arbitrary type aliases, so it compares the final path segment
  (`String`, `i32`, `Decimal`, `OffsetDateTime`, …) and the `Option<_>` wrapper
  against the [mapping table](#mapping-types--rust-types). For an `object` field
  it expects a struct, for a `nested` field a `Vec<_>`, and it defers the inner
  field checks to *that* struct's own `FlussoDocument` derive.
- **Escape hatches.** A field typed `serde_json::Value` opts out of type checking
  (it'll deserialize whatever is there). `#[flusso(skip)]` drops a field from
  validation entirely — for a computed or app-only field not backed by the index
  (pair it with `#[serde(skip)]` or `#[serde(default)]` so it deserializes).

### Mapping types → Rust types

The type the derive **expects** for each field. Declare something else and it
won't compile (modulo the leaf-identifier rule above).

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
| `Object`                                        | a struct (the children)         | `Object<T>`     |
| `Nested`                                        | `Vec<` a struct `>`             | `Nested<T>`     |
| `Other(s)`                                      | `serde_json::Value`             | `Json`          |

**Decimals** map to `rust_decimal::Decimal`, the same type flusso already carries
in [`GenericValue`](libs/0-schema/0-core/src/common/generic_value.rs) — no lossy
float round-trip. **Dates** are behind a feature so a caller picks `time` or
`chrono` (or `String` for raw ISO-8601) without the crate forcing a dependency;
the derive accepts whichever leaf type the chosen feature settles on.

### Nullability comes from the source

A field is `T` or `Option<T>`, and the developer must not guess which — the
derive **checks** it against the resolved mapping and rejects a mismatch.
`ResolvedField` carries a `nullable: bool`, derived during mapping resolution:
the shared resolver in `sources-core` applies the rules below, and the only thing
it asks the source is a plain column's intrinsic type and nullability (the
Postgres source reads both from one `pg_attribute` row — `format_type` and
`attnotnull`). The derive then requires `nullable: false → T`,
`nullable: true → Option<T>`.

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

This is why the check can't be a column-nullability lookup alone: a `count`
is never null, a to-many join is never null, a one-to-one join is nullable even
when its key column isn't. Only the resolver — which knows each field's *source*
— gets these right, and it lives in `sources-core` so every source shares it.
The read alias remains the one outstanding requirement the endgame places on the
engine.

---

## What the field type buys you

The handle a schema field generates determines the operators in scope. This is
the type safety that matters: an operator that doesn't make sense for a field's
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

Each operator's argument is typed too: `User::order_count().gte(_)` takes an
`i32`, `User::order_count().between(_, _)` takes two; `User::email().in_(_)` takes
an `IntoIterator<Item = impl Into<String>>`. Sorting is the same — `sort(…)` only
accepts handles whose mapping is sortable (numbers, dates, keywords, booleans),
so `sort(User::name().desc())` on a `text` field is a compile error, mirroring
OpenSearch's own refusal to sort un-`fielddata` text.

### Composing queries

Handles produce a `Query`; queries compose with `and` / `or` / `not`, and the
`Search` builder exposes the bool-query clauses directly for callers who want
score-vs-filter control:

```rust
// Combinator style.
let q = User::email().eq("ada@example.com")
    .and(User::order_count().gte(5))
    .and(User::orders().any(|o| o.status().eq("paid").and(o.total().gt(dec!(0)))));

User::search(&client).query(q).send().await?;

// Clause style — `filter`/`must_not` don't score, `query`(=must)/`should` do.
User::search(&client)
    .query(User::name().matches("ada"))           // scored
    .filter(User::order_count().gte(5))           // filtered, cached, no score
    .must_not(User::email().prefix("test-"))
    .should(User::orders().any(|o| o.status().eq("vip")))
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

`get` returns `Option<T>`; `search` returns `SearchResponse<T>`, where `T` is the
struct you wrote. There is no `serde_json::Value` in the common path — your typed
struct is the result.

---

## Resolving the index name

The struct's binding knows the **logical** name (`users`, from the `index`
attribute) and its `SCHEMA_HASH`. The **physical** index carries the hash suffix
(`users_3f2a1b9c`) and rotates on a structural schema change — so the client must
not query the physical name directly, or every reindex breaks it.

The contract: **flusso maintains a read alias from the logical name to the
current physical index.** The client queries `users`; flusso points `users` at
`users_<hash>` and atomically re-points it after a backfill into a new shape.
This is a requirement the endgame places on the **engine**, not just the client —
without the alias there is no stable name to query, and "where we need to arrive"
includes the engine owning that alias. On first query the client compares its
`User::SCHEMA_HASH` against the alias's current target and warns (or errors, by
config) on a mismatch, so a binding that has drifted from the deployed index says
so instead of silently returning the wrong shape.

---

## The escape hatch

Anything the typed builder can't express stays reachable, and still
deserializes into the typed struct:

```rust
let page: SearchResponse<User> = User::search(&client)
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
- **Generating the document struct.** By design — the developer owns the struct.
  The macro validates and generates the query surface, nothing more.

---

## Where this lands in the workspace

A new consumer-facing crate plus a derive-macro crate, both reusing the existing
schema layer rather than re-implementing it:

| Crate            | Role                                                                                  |
| ---------------- | ------------------------------------------------------------------------------------- |
| `flusso-client`  | Runtime: the `Client` transport, the field-handle/`Query`/`Search` builder, `SearchResponse`. Generic over the developer's document types. Re-exports the derive behind a `derive` feature (serde-style), so callers `use flusso_client::FlussoDocument`. |
| `flusso-derive`  | The `#[derive(FlussoDocument)]` proc-macro crate. Reads a resolved [`IndexMapping`](libs/0-schema/0-core/src/config/index_mapping.rs) artifact at compile time, validates the annotated struct against it, and emits the field handles, entry points, and schema hash. Depends on `schema-core` for the mapping's deserialize types. |
| `flusso emit-mapping` | A CLI subcommand on the existing binary that writes the resolved mapping artifact the derive consumes. |

The numeric-layer rule still holds: both new crates sit above `schema-core` and
depend only downward. The client crate has no dependency on the engine, the
sources, or the sinks — it shares only the domain model, which is the one thing
the read and write sides must agree on. (`flusso-derive` reuses `schema-core`'s
`IndexMapping` types purely to *read* the committed artifact at compile time — no
runtime coupling.)

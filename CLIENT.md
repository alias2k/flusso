# flusso client (endgame)

> [!IMPORTANT]
> ## 🤖 Generative AI disclosure
>
> **Generative AI was used in this project to produce boilerplate and
> documentation.** Every single line of code has been manually reviewed and
> revised by a human software developer.

> [!NOTE]
> This is a **target document**, not a description of code that exists. It writes
> the client crate's API the way we wish we could call it, so the implementation
> has a fixed point to aim at. Everything below is aspirational; nothing here is
> built yet.

flusso keeps an OpenSearch index in sync with Postgres from a declarative schema
(see [`README.md`](README.md) for the write side). That schema is a contract: it
fixes the shape of every document in the index — which fields exist, their types,
which are nested arrays, which are scalars.

Today that contract is enforced on the **write** side — the engine builds the
index to match — but not on the **read** side. Anyone querying the index
hand-writes OpenSearch JSON and hand-deserializes the results, with nothing
checking either against the schema. It works right up until someone renames a
field and finds out in production.

`flusso-search` closes that gap. Notably, it does **not** generate the document
types for you. You write the document struct by hand and keep full control over
it: its derives, its field types, which fields it projects, how the doc keys map
to Rust names. A **derive macro** then does two things against the resolved
schema, at compile time:

1. **Validates** that every field the struct declares lines up with the schema —
   the field exists, its Rust type matches the field's type, and its nullability
   matches. A struct that has drifted from the schema *stops compiling*, pointing
   at the offending field.
2. **Generates the typed query surface** from the schema — field handles,
   `get`/`search` entry points, the schema hash — so a downstream service queries
   the index the way it would call a typed function: field names checked at
   compile time, operators that only exist for the field types that support them,
   and results that deserialize into the struct you wrote.

The query surface comes from the **full schema**, not from the struct — so you
can filter or sort on a field even if your struct doesn't deserialize it. The
struct is a projection you control; the schema is the contract both the struct
and the query surface are checked against. When the schema changes and the index
is rebuilt, the schema the macro reads changes with it, and any struct or query
that no longer fits stops compiling. The drift surfaces at `cargo build`, not at
2am.

## Contents

- [A query, from the caller's seat](#a-query-from-the-callers-seat) — the whole thing in one example
- [What the field type buys you](#what-the-field-type-buys-you) — operators, composing queries, optional filters
- [Filtering nested collections](#filtering-nested-collections) — filter *by* vs. filter *of*
- [Results](#results) — what `get`/`search` hand back
- [Binding to the schema](#binding-to-the-schema) — how the macro finds and reads the schema
- [The escape hatch](#the-escape-hatch) — raw DSL when the typed surface can't reach
- [Resolving the index name](#resolving-the-index-name) — the hashed physical index
- [Explicitly out of scope for the first cut](#explicitly-out-of-scope-for-the-first-cut)
- [Where this lands in the workspace](#where-this-lands-in-the-workspace)

---

## A query, from the caller's seat

Here is a service that searches the `users` index from [`SCHEMA.md`](SCHEMA.md).
You write the structs; `#[derive(FlussoDocument)]` validates them and generates
the query surface. The only thing the macro needs is the **index name** — it
finds `flusso.toml` itself (see [Binding to the schema](#binding-to-the-schema)).

### The document structs

```rust
use flusso_search::{Client, FlussoDocument};

/// A `users` document — *you* write this. It's a **projection**: it deserializes
/// the fields below and omits the rest of the index (addresses, profile,
/// avgOrderValue, …), which the derive allows. The derive checks every field
/// against the `users` schema and hangs the typed query surface off `User`.
#[derive(Debug, Clone, serde::Deserialize, FlussoDocument)]
#[flusso(index = "users")]                     // ← the only input: which index
pub struct User {
    pub id: i32,                                // primary key (integer) — never null
    pub email: String,                          // keyword, required → never null
    #[serde(rename = "fullName")]
    pub full_name: Option<String>,              // text, not required → nullable
    pub account: Account,                       // a group (object) — always assembled
    pub orders: Vec<Order>,                     // has_many join → nested, never null
    #[serde(rename = "orderCount")]
    pub order_count: i64,                        // count aggregate → long, never null
    #[serde(rename = "lifetimeValue")]
    pub lifetime_value: Option<f64>,            // sum aggregate → nullable
}

/// The `account` group — a same-row sub-object. A nested/group struct validates
/// against its `path` in the same index; it has no entry points of its own.
#[derive(Debug, Clone, serde::Deserialize, FlussoDocument)]
#[flusso(index = "users", path = "account")]
pub struct Account {
    pub tier: String,                           // enum → keyword, required
    pub country: Option<String>,                // keyword, not required
    #[serde(rename = "createdAt")]
    pub created_at: time::OffsetDateTime,       // timestamp, required
}

#[derive(Debug, Clone, serde::Deserialize, FlussoDocument)]
#[flusso(index = "users", path = "orders")]
pub struct Order {
    pub status: String,                         // enum, required
    pub total: f64,                             // decimal → double (lossy; see the type table)
    #[serde(rename = "placedAt")]
    pub placed_at: time::OffsetDateTime,        // timestamp, required
    pub items: Vec<Item>,                       // a deeper has_many → nested
}

#[derive(Debug, Clone, serde::Deserialize, FlussoDocument)]
#[flusso(index = "users", path = "orders.items")]
pub struct Item {
    #[serde(rename = "productId")]
    pub product_id: i32,
    pub quantity: i32,
    #[serde(rename = "unitPrice")]
    pub unit_price: f64,
}
```

### The query

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Transport. Points at OpenSearch, not at flusso — the engine is write-only;
    // reads go straight to the index it maintains.
    let client = Client::connect("https://localhost:9200")?
        .basic_auth("admin", std::env::var("OS_PASSWORD")?);

    // Fetch one document by its id. `id` is typed as the root table's primary
    // key (i32 here), and the result is Option<User> — None when absent.
    let user: Option<User> = User::get(&client, 42).await?;

    // A typed search. Every `User::field()` is a handle that only exposes the
    // operators its mapping supports (see "What the field type buys you"). The
    // handles cover the *whole schema* — so `User::addresses()` filters fine even
    // though this projection never deserializes addresses.
    let page = User::search(&client)
        .filter(User::email().eq("ada@example.com"))      // keyword → exact
        .filter(User::order_count().gte(5))               // long → range
        .filter(User::account().tier().eq("gold"))        // into the group's handles
        .query(User::full_name().matches("ada lovelace")) // text → analyzed match
        .filter(User::orders().any(Order::status().eq("delivered")))   // nested, via your Order struct
        .filter(User::addresses().any(AddressFields::city().eq("Boston"))) // not projected — generated namespace
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
        for order in &u.orders {                          // Vec<Order>
            println!("    order — {} — {}", order.total, order.status);
        }
    }

    Ok(())
}
```

### What you *can't* write

That last part is the whole point. The compiler refuses the mistakes:

- `User::email().matches(..)` does not compile — `matches` is a text operator and
  `email` is a `keyword`.
- `User::full_name().eq(..)` does not compile — `full_name` is analyzed `text`, so
  it has no exact `eq`.
- `User::nmae()` does not compile — there is no such handle.
- `hit.source.totl` does not compile.

And — the new guarantee — the struct itself can't drift. Declaring `email: i32`,
or `email: Option<String>` when the field is `required`, or a field `totl` the
schema doesn't have, is a **compile error from the derive**, not a runtime
surprise. None of these are runtime errors; the schema has been lifted into the
type system, so the compiler is checking both your queries and your struct
against it.

> `OS_PASSWORD` above is just a variable name you picked for the example — there's
> nothing special about it. The full env-var story (secrets, the reserved
> deployment overrides, `FLUSSO_CONFIG`) lives in [`CONFIG.md`](CONFIG.md).

---

## What the field type buys you

The handle a schema field generates determines the operators in scope. This is
the type safety that matters: an operator that doesn't make sense for a field's
type *doesn't exist* on its handle, so the mistake is a compile error rather than
a 400 from OpenSearch.

| Handle          | Operators                                                                 |
| --------------- | ------------------------------------------------------------------------- |
| `Keyword`       | `eq`, `in_`, `prefix`, `wildcard`, `regexp`, `fuzzy`, `exists`            |
| `Text`          | `matches`, `match_phrase`, `match_phrase_prefix`, `matches_fuzzy`, `exists` — *no* exact `eq` (it's analyzed) |
| `Bool`          | `eq`, `exists`                                                            |
| `Number<T>`     | `eq`, `in_`, `lt`, `lte`, `gt`, `gte`, `between`, `exists`                |
| `Date`          | `eq`, `lt`, `lte`, `gt`, `gte`, `between`, `exists`                       |
| `Object<S>`     | `exists` (a same-document sub-object — group or a to-one join (`belongs_to`/`has_one`); `S` is the enclosing scope). Its sub-fields are *flattened*, so query them via the child struct's dotted-path handles (`Account::tier()`), not through this handle |
| `Nested<S, T>`  | `any(q)` / `all(q)` to match parents and **lift** the child query into scope `S` — `q` is a child query built from `T`'s handles ([merging](#building-a-child-filter-and-merging-it-into-the-parent)); `matching(q)` (with `.sort`/`.size`) to shape what's returned — see [Filtering nested collections](#filtering-nested-collections); plus `exists` |
| `Geo`           | `within(distance, center)`, `in_bounding_box`, `in_polygon`, `exists`; sort with `distance_sort(center, order, unit)` |
| `Binary`        | `exists` — base64-encoded, not searchable                                 |
| `Json`          | `exists`, `raw(serde_json::Value)` — the untyped fallback                 |

Each operator's argument is typed too: `User::order_count().gte(_)` takes an
`i64`, `User::order_count().between(_, _)` takes two; `User::email().in_(_)` takes
an `IntoIterator<Item = impl Into<String>>`.

Sorting is the same — `sort(…)` only accepts handles whose type is sortable
(numbers, dates, keywords, booleans), so `sort(User::full_name().desc())` on a
`text` field is a compile error, mirroring OpenSearch's own refusal to sort
un-`fielddata` text.

A few clauses span more than one field, so they're free functions rather than
single-handle operators: `multi_match("ada", [User::full_name(), User::bio()])`
runs one analyzed query across several `Text` fields.

Anything outside the typed surface — `knn`, `function_score`, `script`,
`geo_shape`, span queries — stays reachable through the
[`raw`](#the-escape-hatch) hatch by design. Lifting every OpenSearch clause into a
typed handle would dilute the type guarantee (a `regexp` on a number is
meaningless), so the typed surface is the operators that fit a field's type, and
nothing more.

### Composing queries

A handle's operator produces a `Query<S>` — a query that carries the **scope** it
was built in: the document or nested context `S` it constrains. The root document
and any flattened `object`/to-one-join sub-field share the scope `Root` (so root
handles produce `Query<Root>`); only a `nested` array introduces a fresh scope,
tagged with its element type (`Query<Order>`).

Queries compose with `and` / `or` / `not` *within a scope*, and the `Search`
builder exposes the bool-query clauses directly for callers who want
score-vs-filter control:

```rust
// Combinator style.
let q = User::email().eq("ada@example.com")
    .and(User::order_count().gte(5))
    .and(User::orders().any(Order::status().eq("delivered").and(Order::total().gt(0.0))));

User::search(&client).query(q).send().await?;

// Clause style — `filter`/`must_not` don't score, `query`(=must)/`should` do.
User::search(&client)
    .query(User::full_name().matches("ada"))       // scored
    .filter(User::order_count().gte(5))            // filtered, cached, no score
    .must_not(User::email().prefix("test-"))
    .should(User::orders().any(Order::status().eq("delivered")))
    .send()
    .await?;
```

`query`/`filter`/`must_not`/`should` accept anything that is a `Query<Root>`, so
the two styles mix freely.

A built search can also finish as a **count** instead of a page: `.count()` sends
the same query to `_count` and returns just the number of matching documents
(`u64`) — cheaper than `send()` when the hits aren't needed (nothing is scored or
fetched). Sort, `from`/`size`, and `filter_nested` projections are ignored; they
never change which documents match.

```rust
let open_orders: u64 = User::search(&client)
    .filter(User::orders().any(Order::status().eq("open")))
    .count()
    .await?;
```

Or as an **id page**: `.ids()` runs the same search with `_source: false` and
returns `Vec<String>` — the matching document ids (the root primary keys,
stringified), in order, with no sources fetched. Sort and `from`/`size` apply as
in `send()`; `filter_nested` projections are dropped (there's no source to
shape). This is the cheap way to feed another lookup — e.g. search in
OpenSearch, then load the full rows from Postgres:

```rust
let user_ids: Vec<String> = User::search(&client)
    .filter(User::orders().any(Order::status().eq("open")))
    .sort(User::order_count().desc())
    .size(100)
    .ids()
    .await?;
```

### Building a child filter and merging it into the parent

Because the scope is part of the type, a query is a value you can build, name,
store, and reuse — not just an inline expression. A **nested** child struct (one
whose `path` ends in a `nested` array) carries its own field handles, exactly as
the root does, but tagged with the child scope — so they produce `Query<Order>`,
not `Query<Root>`:

```rust
// Built from Order's own handles. Reusable — a plain function returning a query:
fn big_delivered() -> Query<Order> {
    Order::status().eq("delivered")
        .and(Order::total().gt(100.0))
}
```

To merge a child filter into a parent, **lift** it through the nesting that holds
it: `User::orders().any(child)` (or `.all(child)`) takes a `Query<Order>` and
returns a `Query<Root>` — a nested clause at the `orders` path — which then
composes with parent-scope queries like any other:

```rust
let q = User::email().eq("ada@example.com")
    .and(User::orders().any(big_delivered()));   // Query<Order> → lifted → Query<Root>

User::search(&client).filter(q).send().await?;
```

The scope tag is what keeps this honest: `User::email().and(Order::status().eq(…))`
**does not compile** — you can't `and` a `Query<Root>` with a `Query<Order>`; the
child query has to be lifted through `User::orders()` first. A child constraint
can never be silently applied at the wrong level.

Lifting composes through depth too: `Order::items().any(Item::quantity().gt(1))`
is a `Query<Order>`, which `User::orders().any(…)` then lifts the rest of the way
to `Query<Root>`.

### Optional filters

Real callers build queries from optional inputs — request params, form fields —
and the `if let Some(x) = … { q = q.filter(…) }` dance breaks the fluent chain.
The primitive that fixes it: **`Option<Q>` is itself a `Query`**, where `None`
contributes nothing. It adds no constraint, in any clause — `must_not(None)`
excludes nothing, `and(None)` is the identity. So every clause and combinator
already accepts an optional; you just `.map` the value into the handle:

```rust
User::search(&client)
    .filter(params.email.map(|e| User::email().eq(e)))          // skipped when None
    .filter(params.min_orders.map(|n| User::order_count().gte(n)))
    .send().await?;

// Composes inside and/or too — a None branch just drops out:
let q = User::email().eq("ada@example.com")
    .and(params.tier.map(|t| User::account().tier().eq(t)));    // None → just the email clause
```

A named `filter_some(value, |v| …)` sugar that drops the `.map` is an obvious
follow-on — it would be just `filter(value.map(f))` over this primitive — but it's
left out of the first cut.

---

## Filtering nested collections

`orders` is a nested array, and there are two **independent** things you might
want to filter — flusso keeps them separate, because conflating them is how you
end up confused about why a user with no delivered orders still showed up:

- **Filter *by* nested** — choose which *users* come back, based on their orders.
  This is the `any`/`all` you've already seen: it's a `Query`, so it goes in
  `filter`/`query`/etc. A matching user still carries its **whole** `orders`
  array.
- **Filter *of* nested** — shape the `orders` array each user comes back with,
  without changing which users return. This is a separate clause,
  `filter_nested`.

They compose: use either alone, or both together (often with the same predicate).

### `filter_nested` — shaping the returned array

```rust
let page = User::search(&client)
    // filter BY: only users with a delivered order
    .filter(User::orders().any(Order::status().eq("delivered")))
    // filter OF: and within each, keep only the delivered orders, newest first, ≤5
    .filter_nested(
        User::orders()
            .matching(Order::status().eq("delivered"))
            .sort(Order::placed_at().desc())
            .size(5),
    )
    .send().await?;

for hit in &page.hits {
    // By default `source.orders` IS the filtered subset — no extra accessor:
    for order in &hit.source.orders {       // delivered, newest first, ≤ 5
        println!("{} — {}", order.total, order.status);
    }
}
```

`User::orders().matching(q)` is a nested **projection**: `q` is a `Query<Order>`
built from `Order`'s handles, plus optional `.sort(Order::field().desc())`,
`.size(n)`, `.from(n)`. `matching` itself is optional — drop it to keep every
child but still sort or cap the array.

Because `filter_nested` does **not** touch which parents match, a user with no
delivered orders still comes back — with `orders: []`. Pair it with a
`filter(User::orders().any(…))` when you also want to drop those users.

### Where the subset lands — replace by default, opt out to keep both

By default `filter_nested` **replaces** `hit.source.orders` with the matched
subset, so you read it straight off the typed struct. Mechanically the client
fetches the nested matches and substitutes them for that field before
deserializing `User` — `source.orders` reflects the projection, not the stored
array.

When you'd rather keep the stored array intact, opt out per path with
`.keep_source()`: `source.orders` stays the full document array, and the subset
moves to a typed side accessor.

```rust
.filter_nested(
    User::orders()
        .matching(Order::status().eq("delivered"))
        .size(5)
        .keep_source(),         // leave source.orders untouched
)
// …
let all_orders: &[Order] = &hit.source.orders;          // full, as stored
let delivered: &[Order]  = hit.nested(User::orders());  // matched subset
```

`hit.nested(User::orders())` returns `&[Order]` with no turbofish because your
struct declared `orders: Vec<Order>`, so `User::orders()` is `Nested<Root, Order>`
and the subset deserializes into `Order`. For a nested path your struct doesn't
project (a `Nested<Root, AddressFields>`), pass the type explicitly:
`hit.nested::<Address>("addresses")`.

### Depth

`filter_nested` shapes one nested level — `orders`. You can still *match* on
deeper nesting from inside the predicate (`Order::items().any(Item::quantity().gt(1))`),
and the returned orders honor it; returning a filtered `items` array *inside*
each returned order is a deeper inner-hits case left to the `raw` hatch for now.

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

## Binding to the schema

The macro validates against the **resolved mapping** — flusso's
[`IndexMapping`](libs/0-schema/0-core/src/config/index_mapping.rs): every field
with a concrete type, whether it is **nullable**, and its nested `children`, plus
the schema `hash`.

Crucially, flusso's schemas are **self-describing**: every leaf declares its
`type` and whether it's `required`, and joins/groups/aggregates have structural
types — so the mapping resolves with **no database**, exactly as `flusso build`
does when it writes `flusso.lock`. The client reuses that resolution rather than
re-deriving one. (See [`SCHEMA.md`](SCHEMA.md) for the schema format and
[`SOURCES_AND_SINKS.md`](SOURCES_AND_SINKS.md) for how the index is written.)

### The one input: the index name

You never point the macro at a file. You name the **index**, and the macro finds
the schema:

```rust
#[derive(serde::Deserialize, FlussoDocument)]
#[flusso(index = "users")]
pub struct User { /* … */ }
```

At compile time the macro:

1. **Locates `flusso.toml`** by walking up the directory tree from the consuming
   crate's `CARGO_MANIFEST_DIR`, the way cargo finds `Cargo.toml`. (Override with
   `#[flusso(index = "users", config = "…")]` or the `FLUSSO_CONFIG` env var if
   the config lives outside that path — see [`CONFIG.md`](CONFIG.md#the-derive-compile-time)
   for that variable.)
2. **Selects the `[[index]]`** whose `name` matches `"users"` — the reason an
   index name is required at all, since one `flusso.toml` defines several
   (`users`, `products`, `orders`).
3. **Loads that index's `schema:` file** (resolved relative to `flusso.toml`, per
   the config's own rule) and **resolves the `IndexMapping`** in-process — the
   same resolution `flusso build` performs. Because the schemas are
   self-describing, this is hermetic: no database, no network.
4. **Tracks `flusso.toml` and every schema file it read** as build inputs (folded
   in via `include_bytes!`), so editing the config or a schema retriggers
   compilation and a drifted struct fails the next build.

The resolved mapping's content hash is the binding's `SCHEMA_HASH` — the **same
hash** `flusso build` writes into `flusso.lock` and the engine folds into the
physical index name, so binding and index are provably the same schema version.

There is no `build.rs`, no generated `.rs` file to `include!`, and no committed
mapping artifact to keep in sync — the struct is the only file you maintain, and
the derive expands in place against the live schema.

### A nested or group struct names its path

A `group`, an `object` join, or a `nested` join is its own struct, validated
against a dotted **`path`** into the same index — `account`, `orders`,
`orders.items`. It declares the same `index` so the macro resolves the same
config, then walks to that path's `children`:

```rust
#[flusso(index = "users", path = "orders.items")]
pub struct Item { /* … */ }
```

These contribute field validation **and** their own field handles —
`Order::status()`, `Order::total()`, … — producing `Query<Order>` values you can
compose, store, and lift into a parent query (see [Building a child filter and
merging it into the parent](#building-a-child-filter-and-merging-it-into-the-parent)).
Only the **root** struct gets entry points (`get`/`search`) and `SCHEMA_HASH`.

### What the derive expands to

`#[derive(FlussoDocument)]` on the root `User` emits (roughly):

```rust
impl User {
    // Entry points.
    pub fn get(client: &Client, id: i32) -> impl Future<Output = Result<Option<User>>>;
    pub fn search(client: &Client) -> Search<User>;

    // Field handles — one per *schema* field, carrying its type. These are what
    // the query builder consumes; see "What the field type buys you". They exist
    // for every field in the mapping, whether or not `User` projects it.
    pub fn id() -> Number<i32> { /* … */ }
    pub fn email() -> Keyword { /* … */ }
    pub fn full_name() -> Text { /* … */ }
    pub fn account() -> Object { /* … */ }                  // object/to-one join → `Object<Root>` (scope-only; `.exists()`)
    pub fn addresses() -> Nested<Root, AddressFields> { /* … */ } // not projected — generated namespace
    pub fn orders() -> Nested<Root, Order> { /* … */ }      // projected — `Nested<enclosing scope, your struct>`
    pub fn order_count() -> Number<i64> { /* … */ }
    pub fn lifetime_value() -> Number<f64> { /* … */ }
    pub fn avg_order_value() -> Number<f64> { /* … */ }       // not projected by `User`
    pub fn last_order_at() -> Date { /* … */ }                // not projected by `User`
    // …one per schema field.

    /// The physical index this binds to — `get`/`search` use it. Logical name
    /// plus the schema hash, matching what the engine's sink writes.
    pub const INDEX: &str = "users_3f2a1b9c…";
    /// The schema hash this binding was generated from (the `INDEX` suffix).
    pub const SCHEMA_HASH: &str = "3f2a1b9c…";
}

// Each nested path has ONE handle namespace whose associated functions build a
// `Query` in that path's scope. When you wrote a struct for the path, that struct
// IS the namespace — its derive adds the handles, covering the full sub-schema
// (not just the fields it deserializes), producing `Query<Order>`:
// `orders` is a `nested` array, so it introduces its own scope: `Order`'s handles
// are tagged `<Order>` (a `nested` array tags handles with the element type; the
// root and flattened objects stay `<Root>`). They must be lifted before joining a
// root query — see below.
impl Order {
    pub fn status() -> Keyword<Order> { /* … */ }
    pub fn total() -> Number<f64, Order> { /* … */ }
    pub fn placed_at() -> Date<Order> { /* … */ }
    pub fn items() -> Nested<Order, Item> { /* … */ }   // deeper nested: enclosing scope `Order`, child `Item`
    // …one per field at the `orders` path.
}

// For a nested path you DIDN'T give a struct, the root derive generates a
// handles-only namespace named `<Path>Fields`, so it's still queryable:
pub struct AddressFields;
impl AddressFields {
    pub fn city() -> Keyword<AddressFields> { /* … */ }       // nested scope, like `Order`
    pub fn postal_code() -> Keyword<AddressFields> { /* … */ }
    // …one per field at the `addresses` path.
}
```

### What the derive checks

For each field the struct declares, the macro resolves the matching schema field
by its **document key** — honoring `#[serde(rename = "…")]` and a container
`#[serde(rename_all = "…")]` so the struct's serde config and flusso's validation
agree (field names keep their case in the document, so `fullName` is the doc key
and `full_name` the Rust name) — then checks three things:

| Check                | Pass                                              | Compile error                                                        |
| -------------------- | ------------------------------------------------- | -------------------------------------------------------------------- |
| **field exists**     | the doc key is in the schema                       | `no field `totl` in index `users`` (span on the field)              |
| **type matches**     | leaf Rust type matches the field's `type`          | `email is `keyword` → expected `String`, found `i32``               |
| **nullability matches** | `Option<_>` iff the field is nullable           | `email is required → expected `String`, found `Option<String>``     |

The rules that make this **full control rather than a straitjacket**:

- **Partial projections are allowed.** Leaving schema fields off your struct is
  fine — you only deserialize the subset you declare. Omission is never an error;
  only the three checks above fail.
- **Type matching is by leaf identifier + `Option` shape.** The macro can't
  resolve arbitrary type aliases, so it compares the final path segment
  (`String`, `i32`, `f64`, `OffsetDateTime`, …) and the `Option<_>` wrapper
  against the [type table](#flusso-types--rust-types). For a group/`object` field
  it expects a struct, for a `nested` field a `Vec<_>`, and it defers the inner
  field checks to *that* struct's own `FlussoDocument` derive.
- **Escape hatches.** A field typed `serde_json::Value` opts out of type checking
  (it'll deserialize whatever is there). `#[flusso(skip)]` drops a field from
  validation entirely — for a computed or app-only field not backed by the index
  (pair it with `#[serde(skip)]` or `#[serde(default)]` so it deserializes).

### flusso types → Rust types

The type the derive **expects** for each schema `type` (the same bridge table
[`SCHEMA.md`](SCHEMA.md#types) defines, with the Rust side added). Declare
something else and it won't compile (modulo the leaf-identifier rule above).

| flusso `type`     | OpenSearch | Rust type                        | Field handle    |
| ----------------- | ---------- | -------------------------------- | --------------- |
| `text`            | `text`     | `String`                         | `Text`          |
| `identifier`      | `text`     | `String`                         | `Text`          |
| `keyword`         | `keyword`  | `String`                         | `Keyword`       |
| `enum`            | `keyword`  | `String`                         | `Keyword`       |
| `uuid`            | `keyword`  | `String` (or `uuid::Uuid`, feature) | `Keyword`    |
| `boolean`         | `boolean`  | `bool`                           | `Bool`          |
| `short`           | `short`    | `i16`                            | `Number<i16>`   |
| `integer`         | `integer`  | `i32`                            | `Number<i32>`   |
| `long`            | `long`     | `i64`                            | `Number<i64>`   |
| `float`           | `float`    | `f32`                            | `Number<f32>`   |
| `double`          | `double`   | `f64`                            | `Number<f64>`   |
| `decimal`         | `double`   | `f64` *(lossy — see note)*       | `Number<f64>`   |
| `date`            | `date`     | `time::Date` (feature)           | `Date`          |
| `timestamp`       | `date`     | `time::OffsetDateTime` (feature) | `Date`          |
| `binary`          | `binary`   | `String` (base64)                | `Binary`        |
| `json`            | `object`   | `serde_json::Value`              | `Json`          |
| `geo_point`       | `geo_point`| `GeoPoint` (`{ lat, lon }`)      | `Geo`           |
| `custom { opensearch }` | (given) | matching scalar, else `serde_json::Value` | by OS type |
| `group`           | `object`   | a struct                         | `Object`        |
| join `belongs_to` / `has_one` | `object`   | `Option<` a struct `>`           | `Object`        |
| join `has_many` / `many_to_many` | `nested` | `Vec<` a struct `>`  | `Nested<S, T>`  |

**Decimals are lossy by default.** `type: decimal` maps to OpenSearch `double`,
so a money field round-trips as `f64` — fine for most things, less fine for an
accountant. When exactness matters, declare a `custom` `scaled_float` in the
schema (`type: { custom: { postgres: [numeric], opensearch: scaled_float } }`,
`options: { scaling_factor: 100 }`); the derive then accepts
`rust_decimal::Decimal` for that field.

**Dates** are behind a feature so a caller picks `time` or `chrono` (or `String`
for raw ISO-8601) without the crate forcing a dependency; the derive accepts
whichever leaf type the chosen feature settles on.

### Nullability is declared, not guessed

A field is `T` or `Option<T>`, and you must not guess which — the derive
**checks** it against the resolved mapping and rejects a mismatch. Because schemas
are self-describing, nullability comes straight from the schema with no database
round-trip: a leaf states it with `required`, and joins, groups, and aggregates
carry it structurally. `ResolvedField` records the resulting `nullable: bool`; the
derive requires `nullable: false → T`, `nullable: true → Option<T>`.

| Field source                          | `nullable` | Why                                                  |
| ------------------------------------- | ---------- | ---------------------------------------------------- |
| root `primary_key` column             | `false`    | forced non-null — it backs the document id           |
| join `primary_key` field              | `false`    | forced non-null, just like the root key              |
| leaf column, `required: true`         | `false`    | declared non-null                                    |
| leaf column, `required: false`        | `true`     | nullable by default                                  |
| `group` (`object`)                    | `false`    | always assembled from the same row                   |
| join `belongs_to` / `has_one` (`object`)          | `true`     | there may be no related row                          |
| join `has_many` / `many_to_many`     | `false`    | a `Vec`, empty when there are none, never null       |
| aggregate `count`                     | `false`    | a non-null `long` — zero rows is `0`, not null       |
| aggregate `avg`                       | `true`     | a nullable `double` — null over zero rows            |
| aggregate `sum` / `min` / `max`       | `true`     | null over zero rows; the result mirrors the column   |

`required` is rejected by the schema on joins and aggregates precisely because
their nullability is structural — a `count` is never null, a to-many join is never
null, a one-to-one join is nullable even when its key column isn't — so there's
nothing for the author to declare. The read alias remains the one outstanding
requirement the endgame places on the engine.

---

## The escape hatch

Anything the typed builder can't express stays reachable, and still deserializes
into the typed struct:

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

## Resolving the index name

The **physical** index carries the hash suffix (`users_3f2a1b9c`) — exactly what
the OpenSearch sink writes — and rotates on a structural schema change.

Because the binding is **generated from the schema at compile time**, the derive
knows that hash and emits it as a `const`: `User::INDEX` is the physical name
(`users_<hash>`), and `get`/`search` use it. So `User::search(&client)` addresses
the right index directly, with the hash hidden from the caller — no read alias
required.

This is self-correcting: a structural schema change rotates the hash *and* changes
the resolved mapping, so the next `cargo build` regenerates the binding against the
new physical index. (`User::INDEX` and `User::SCHEMA_HASH` are exposed for
logging, admin, or a hand-built `Search`.)

> A read alias (`users` → current physical) is still worthwhile for clients that
> *don't* recompile against the schema — dynamic/scripting use, dashboards. For a
> derived binding it's unnecessary: the compile-time hash is the stable name.

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
| `flusso-search` | Runtime: the `Client` transport, the field-handle/`Query`/`Search` builder, `SearchResponse`. Generic over the developer's document types. Targets OpenSearch / Elasticsearch (shared DSL). Re-exports the derive behind a `derive` feature (serde-style), so callers `use flusso_search::FlussoDocument`. |
| `flusso-derive`  | The `#[derive(FlussoDocument)]` proc-macro crate. At compile time it discovers `flusso.toml`, resolves the named index's [`IndexMapping`](libs/0-schema/0-core/src/config/index_mapping.rs) from the self-describing schema (no database), validates the annotated struct against it, and emits the field handles, entry points, and schema hash. Reuses `schema-config-toml`, `schema-index-yaml`, and `schema-core` to load and resolve. |

The numeric-layer rule still holds: both new crates sit above `schema-core` and
depend only downward. The client crate has no dependency on the engine, the
sources, or the sinks — it shares only the domain model, which is the one thing
the read and write sides must agree on. `flusso-derive` reuses the schema crates
purely to *read and resolve* the schema at compile time — the same resolution
`flusso build` performs to write `flusso.lock` — with no runtime coupling.

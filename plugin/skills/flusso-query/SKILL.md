---
name: flusso-query
description: Query a flusso index from Rust with `flusso-query` and `#[derive(FlussoRoot)]` / `#[derive(FlussoFragment)]`. Use when writing or editing read-side code — document structs, typed queries, sorting, nested filtering, custom value types, multi-index search.
---

# Querying a flusso index

flusso owns the **write** side: it builds an OpenSearch index to match the schema. `flusso-query` is
the **read** side, a typed OpenSearch/Elasticsearch client. Reads go straight to OpenSearch, not
through flusso.

The contract is the schema. `#[derive(FlussoRoot)]` reads the resolved schema at compile time with
no database, and:

1. **Validates** your hand-written struct against it — field exists, leaf Rust type matches,
   nullability matches. A drifted struct stops compiling.
2. **Generates the typed query surface for the whole index** — a handle for every field at every
   level, one namespace per container, plus `get`/`query` and the schema hash that names the physical
   index.

You own the struct as a **projection**: deserialize the subset you want. The query surface covers the
whole schema, so you can filter and sort on fields the struct never reads.

**Exactly one type names an index: the root.** Everything below it is a `#[derive(FlussoFragment)]`,
a shape with no index and no path, validated by whichever root embeds it. So one fragment serves
several paths or indexes; embed it twice and it is checked twice. Generated scope types live in a
`flusso_<root>_query` module (`flusso_user_query::Orders`, `flusso_user_query::OrdersItems`), never
in your namespace, so a struct already named after a level is fine. Import what you query. Rename a
level with `#[flusso(scope = "Purchases")]` on the root field, or the module with
`#[flusso(scope_mod = "user_queries")]`.

## Read next, when the task reaches it

- **Either migration** (adopting flusso onto an existing struct, or moving off the removed
  `FlussoDocument` / `path = "…"` form) → [`migration.md`](migration.md).
- **An option beyond the defaults**, a compound or standalone query type, or `SortBuilder` →
  [`options.md`](options.md).
- **A `map:` field** in the schema → [`maps.md`](maps.md).

## Crates and features

- `flusso-query` — the runtime: `Client`, field handles, `Query`/`Search`, `SearchResponse`.
  Re-exports the derives behind the **`derive`** feature. Two trait imports are method-gated:
  `FlussoRoot` (to call `Type::query()` / `Type::get()`; a root-only supertrait of `FlussoScope`, so
  a fragment **cannot** start a search) and `Sortable` (for `.asc()` / `.desc()`). `FlussoRoot` is
  both a derive and a trait, imported by the same name.
- `flusso-query-derive` — the proc-macros: `FlussoRoot`, `FlussoFragment`, `FlussoMultiDocument`,
  `FlussoValue`, `FlussoMap`.
- Optional features: **`derive`**, **`decimal`** (`rust_decimal::Decimal`), **`chrono`** / **`time`**
  (date leaves; pick one, or `String` for raw ISO-8601), **`uuid`** (`uuid::Uuid` as a `keyword`
  value).

## The shape of a consumer

```rust
use flusso_query::{Client, FlussoFragment, FlussoRoot};

#[derive(Debug, Clone, serde::Deserialize, FlussoRoot)]
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

// A fragment names NO index and NO path. `User` validates it against
// `users.orders`. Handles for that level come from the root.
#[derive(Debug, Clone, serde::Deserialize, FlussoFragment)]
pub struct Order {
    pub status: String,                 // enum → keyword
    pub total: Decimal,                 // decimal (or f64)
}
```

```rust
let client = Client::connect("https://localhost:9200")?
    .basic_auth("admin", std::env::var("OS_PASSWORD")?);

let user: Option<User> = User::get(&client, 42).await?;      // by primary key

let page = User::query()                                     // client-free value
    .filter(User::email().eq("ada@example.com"))             // keyword → exact
    .filter(User::order_count().gte(5))                      // long → range
    .query(User::full_name().matches("ada lovelace"))         // text → analyzed
    .filter(User::orders().any(flusso_user_query::Orders::status().eq("delivered")))
    .sort(User::order_count().desc())
    .from(0).size(20)
    .send(&client).await?;

for hit in page.hits {                  // hit.id, hit.score from the envelope
    let u: &User = &hit.source;
}
```

See [`examples/consumer.rs`](examples/consumer.rs) for a fuller worked file.

## How the derive binds to the schema

`#[flusso(index = "users")]` is the only input. At compile time the macro:

1. Walks **up from `CARGO_MANIFEST_DIR`** for `flusso.toml`, like cargo finds `Cargo.toml`. Override
   with `#[flusso(config = "…")]` or `FLUSSO_CONFIG`.
2. Selects the `[[index]]` whose `name` matches.
3. Loads that index's `schema:` file and resolves the `IndexMapping` in-process, the same resolution
   `flusso build` performs.
4. Tracks `flusso.toml` and every schema file as build inputs, so editing either retriggers
   compilation and a drifted struct fails the next build.

`User::SCHEMA_HASH` is the resolved schema's content hash and `User::INDEX` the logical name, so
`User::physical_index()` is `users_<hash>`, the stable hash alias the sink maintains over the active
generation. `get`/`query` address it directly, so there is no read alias to manage, and a structural
schema change rotates the hash and forces a recompile.

## What each field type lets you write

An operator that doesn't fit a field's type **doesn't exist** on its handle. The mistake is a
compile error, not a 400.

| Handle | Operators |
| --- | --- |
| `Keyword` | `eq` `any_of` `prefix` `wildcard` `regexp` `fuzzy` `exists`; subfields `text()` / `keyword_lowercase()` |
| `Enum` | An `enum` with declared `variants`: `eq` `any_of` `exists`, `keyword()` for the full `Keyword` surface. Sorts by declared order (see Sorting). A bare enum with no order is a plain `Keyword`. |
| `Text` | `matches` `match_phrase` `match_phrase_prefix` `match_bool_prefix` `matches_fuzzy` `any_of` (exact, via `.keyword`) `exists`; subfields `keyword()` / `keyword_lowercase()`. **No exact `eq`** — it's analyzed. |
| `Bool` | `eq` `exists` |
| `Number<K>` | `eq` `any_of` `lt` `lte` `gt` `gte` `between` `exists`. `K` per type (`Byte`…`Decimal`); values widen losslessly, so `eq(5)` works on `long`/`double`/`decimal` while a float on an int field is a compile error. |
| `Date` | `eq` `any_of` `lt` `lte` `gt` `gte` `between` `exists` |
| object namespace | A same-doc sub-object or to-one join. Objects flatten, so the namespace **chains** from its parent: `User::account().tier()`, `User::account().exists()`. |
| `Nested<S,T>` | `any(q)` / `all(q)` to match parents and **lift** a child query into scope `S`; `matching(q)` (+ `.sort`/`.size`/`.from`) to shape the returned array; `exists` |
| `Geo` | `within(Distance::km(12.0), center)` `within_box` `within_polygon` `exists`; `distance_from(center)` / `distance_sort(center, order, DistanceUnit)`. The radius is a typed `Distance`, not a string. |
| map handles | `TextMap` / `KeywordMap` / `NumberMap<K>` / `DateMap` → [`maps.md`](maps.md) |
| `Binary` | `exists` (base64, not searchable) |
| `Json` | `exists` `raw(serde_json::Value)` |

Cross-field: `multi_match("ada", [User::full_name(), User::bio()])`, weight one with `.boosted(3.0)`.

**Subfield accessors.** flusso's sink auto-enriches `text`/`keyword` fields (`auto_subfields`, on by
default) with exact, sortable and searchable subfields, reachable with no string path:
`User::full_name().keyword()` (exact, `wildcard`, `prefix`), `.keyword_lowercase()`
(case-insensitive), `User::email().text()` (full-text over a keyword). A `wildcard` belongs on
`.keyword()`, not the analyzed handle. **Compile-enforced:** the derive stamps a handle with
subfields only when every OpenSearch sink has `auto_subfields` on and the field has no custom
`fields`; otherwise the handle is `…<NoSubfields>` and the accessors don't exist.

What only [`raw`](#the-raw-escape-hatch) reaches: `knn`/vector, `geo_shape`, span, and parent/child
queries — types with no flusso field.

## Sorting

Everything about ordering, in one place.

`sort(…)` takes a sortable handle via `handle.asc()` / `.desc()`, which need
`use flusso_query::Sortable`. `Search::sorts(iter)` takes several at once, and
[`SortBuilder`](options.md#sortbuilder) is the way to map a request onto the array.

**What sorts, and how:**

| Handle | Behavior |
| --- | --- |
| `Number` `Date` `Bool` `Keyword` | Sorts on the field. |
| `Text` | Sorts via the case-insensitive `.keyword_lowercase` subfield automatically. Use `.keyword().desc()` for an exact-case sort. |
| `Enum` with declared `variants` | Sorts by **declared order**, not alphabetically, via a rank prebaked into a `.sort` subfield. No script, nothing extra to write. A stored value outside the list sorts after the declared ones. |
| a field inside a `nested` array | Wraps in the right `nested` chain automatically at any depth, from the handle's scope. Mode defaults from direction (`asc→min`, `desc→max`). Never hand-write `.nested(path)`. |

**What does not sort, and what to use instead:**

| Not `Sortable` | Use |
| --- | --- |
| `Geo` | `Geo::distance_from(center)` for nearest-first |
| a bare map handle | `Type::field().sort_key("it").or("en")` |
| `.key("it")` on a map | the same `sort_key`. A bare `key(..).asc()` would target a nonexistent subfield, so it doesn't compile. |
| `Object` namespaces | sort a leaf inside them |

**Map sort with fallback.** `Type::field().sort_key("it").or("en")` reads as "sort by `it`, else
`en`" and is true fallback, not lexicographic tiers: a row with only `en` still orders by `en`. It
is `Sortable`, so it flows through the normal `.sort(..)` / `.by(handle, dir)`; a single key is
`sort_key("it")` with no `.or`. String maps sort case-insensitively on the key's `.keyword`;
numeric and date maps on the bare key. `missing_first` / `missing_last` / `missing(v)` resolve to a
direction-correct fallback value rather than the `missing` field, which a `_script` sort ignores.
`numeric_type` / `unmapped_type` / `format` don't apply and are dropped. Several map sorts coexist,
deduped by field path.

**Modifiers** on a single key: `.missing_first()` / `.missing_last()` / `.missing(v)`,
`.mode(SortMode::..)`, `.unmapped_type(..)` / `.numeric_type(..)` / `.format(..)`.

## Filtering: which operator for which field

Pick the operator from the field's **type**, not by habit. Get this wrong and you reach for an
escape hatch you don't need.

| Field | Want | Use |
| --- | --- | --- |
| `keyword` / `enum` / `uuid` | exact match | `Type::field().eq(v)` |
| `keyword` / number / date | any of a set | `Type::field().any_of([a, b])` |
| `keyword` | case-insensitive exact | `Type::field().keyword_lowercase().eq(v)` |
| id or foreign key | filter by id | `Type::id().eq(uuid)` — uuid feature, no wrapper struct, no `.to_string()` |
| `text` | full-text | `Type::field().matches(v)` |
| `text` | phrase, terms in order | `Type::field().match_phrase(v)` |
| `text` | exact whole-value | `Type::field().keyword().eq(v)` |
| number / date | range | `.gte(v)` / `.lte(v)` / `.between(a, b)` |

`matches` and `match_phrase` are for **analyzed `text` only**. On a `keyword` a `match_phrase` is
whole-value, behaviorally just `.eq()`, so write `.eq()`.

## Anti-patterns — scan for these before you finish

Each is what an LLM reaches for when it doesn't trust the typed surface. Each typed form is shorter
*and* compile-checked.

1. **String-path handle** — `Keyword::<Root>::at("code")` when a generated `Type::code()` exists. The
   string path **bypasses the compile-time mapping check**, which is the entire point of the derive.
   → `Type::code()`. (`::at` is only for hand-written handles with no derived struct at all.)
2. **`matches` / `match_phrase` on a keyword field** → `.eq()` / `.any_of()`. A legacy
   `match_phrase` on a keyword equals `.eq()`; port it, don't reproduce the JSON.
3. **Hand-rolled `Option` flattening** — `Vec<Option<Query>>` + `.flatten()` + a loop.
   **`Option<Q>` already *is* a `Query`** and `None` adds nothing. → One line per filter:
   `search.filter(params.x.map(|v| Type::x().eq(v)))`.
4. **Wrapper struct just to filter** — inventing `struct Key { id: Uuid }`. → `Type::id().eq(uuid)`.
   The document struct is a projection for *results*, never a filter-input type.
5. **`raw(json!(…))` for something typed** — `eq`, ranges, `matches`, `function_score`, `script`,
   `query_string`, `sort` and `search_after` are all typed ([`options.md`](options.md)). `raw` is
   only for `knn` / `geo_shape` / span / parent-child.
6. **`#[flusso(skip)]` on a `Uuid` or enum keyword** → keep it typed: `Uuid` (uuid feature) or a
   `#[derive(FlussoValue)]` enum.

**Porting a legacy query builder?** Map each clause to its idiomatic typed form and match
**behavior, not byte-identical JSON**. A `term`-vs-`match_phrase` difference selecting the same
documents is not worth an escape hatch plus an apologetic comment. If a real behavioral difference
exists, state it in one line.

**The compiler is the safety net.** Write the typed form and run `cargo check`.

**Self-check before finishing** — these compile, so grep your own diff and justify or fix each hit:

| grep | smell | fix |
| --- | --- | --- |
| `::at("` | string-path handle | the generated `Type::field()` |
| `.raw(` | escape hatch | only `knn`/`geo_shape`/span/parent-child belong |
| `.flatten()` / `Vec<Option<` near filters | hand-rolled optionals | `.filter(opt.map(\|v\| …))` |
| `match_phrase` / `matches` | check the field is **`text`** | keyword → `.eq()` / `.any_of()` |
| a struct only holding filter inputs | wrapper-to-filter | filter via handles |

## Writing readable queries

Compact **and** clear, both at once. Aim to keep a query on one screen, but never buy density with
confusion.

- **The builder chain is the canonical form** — one clause per line (`.filter(..)`, `.query(..)`,
  `.sort(..)`), read top-to-bottom like a spec.
- **One clause, one line, when it fits or almost.** Don't wrap what already fits.
- **Too dense to read at a glance? Bind it to a named `let` first**, then drop the name into the
  chain. A lifted nested query with several conditions, an `or`-group, a `function_score` — give it
  an intent-revealing name so the chain stays scannable and the name says *why*.
  ```rust
  let high_value_delivered = User::orders().any(
      flusso_user_query::Orders::status().eq("delivered")
          .and(flusso_user_query::Orders::total().gte(100.0)),
  );

  let page = User::query()
      .filter(high_value_delivered)
      .filter(User::tier().any_of([Tier::Pro, Tier::Enterprise]))
      .sort(User::order_count().desc())
      .send(&client).await?;
  ```
- **Recurring query → a client-free helper** (`fn busy_users() -> Search<User>`), extended at the
  call site (`busy_users().from(20)`).
- **Conditional filters are one line each** (anti-pattern #3), not a multi-line block.

## Composing — scope is in the type

A handle's operator produces `Query<S>`, carrying the **scope** `S` it was built in. The root and
any flattened `object` or to-one join share `Root`. A **`nested` array introduces a fresh scope**,
tagged with the namespace the root generated for it.

```rust
// within a scope: and / or / not
let q = User::email().eq("ada@x.io").and(User::order_count().gte(5));

// clause style — filter/must_not don't score; query(=must)/should do
User::query()
    .query(User::full_name().matches("ada"))     // scored
    .filter(User::order_count().gte(5))           // filtered, cached, no score
    .must_not(User::email().prefix("test-"))
    .should(User::orders().any(flusso_user_query::Orders::status().eq("delivered")))
    .send(&client).await?;
```

`User::email().and(flusso_user_query::Orders::status().eq(…))` **does not compile**: you can't `and`
a `Query<Root>` with a `Query<flusso_user_query::Orders>`. Lift the child first.
`User::orders().any(child)` takes a `Query<flusso_user_query::Orders>` and returns `Query<Root>`.
Lifting composes through depth.

**Queries are values; the client appears once.** `Type::query()` takes no client, and `Search<T>` is
a plain `Clone` value. Build it in a helper, store it, reuse it, and hand `&Client` to a terminal.

**Terminals:** `.send(&client)` → `SearchResponse<T>`; `.count(&client)` → `u64` (no fetch or
score); `.ids(&client)` → `Vec<String>` (`_source: false`).

**Optional filters:** `Option<Q>` is itself a `Query`, so
`.filter(params.email.map(|e| User::email().eq(e)))` drops out when absent.

## Nested collections — filter *by* vs filter *of*

Two independent things, deliberately separate:

- **Filter BY** — which *parents* return, based on children: `User::orders().any(...)` / `.all(...)`.
  A matching parent still carries its **whole** array. It's a `Query`, so it goes in `filter`,
  `query`, and so on.
- **Filter OF** — shape the array each parent returns, without changing which parents match:
  `.filter_nested(User::orders().matching(q).sort(...).size(...))`.

```rust
let page = User::query()
    .filter(User::orders().any(flusso_user_query::Orders::status().eq("delivered")))   // BY
    .filter_nested(                                                                    // OF
        User::orders().matching(flusso_user_query::Orders::status().eq("delivered"))
            .sort(flusso_user_query::Orders::placed_at().desc()).size(5),
    )
    .send(&client).await?;
```

By default `filter_nested` **replaces** `hit.source.<path>` with the matched subset, read straight
off the struct. A parent with no matches still returns, with `[]`.

## Multi-index

- **One blended list** — `#[derive(FlussoMultiDocument)]` on an enum with one single-field tuple
  variant per document type. `StoreItem::query()…send(&client)` ranks hits together; dispatch by
  matching on `hit.source`. Purely syntactic, so it validates enum shape and duplicate payload types
  but resolves no schema. A *sort* on a field not in every index needs `unmapped_type`.
- **Several searches, one round-trip** — `client.msearch((&q1, &q2))` (tuple arity 1-8) gives one
  typed `SearchResponse` per slot, in order. `client.msearch_all(&searches)` for many of one type.

## Custom value types — `#[derive(FlussoValue)]`

Let a scalar field be your own enum or newtype instead of a bare leaf:

```rust
#[derive(serde::Deserialize, serde::Serialize, FlussoValue)]
#[flusso(keyword)]                       // enum kind: keyword | text — required, no default
enum AccountTier { Free, Pro, Enterprise }
```

A **newtype inherits its inner type's kinds** automatically and needs **no** tag: `struct
Money(Decimal)` is a decimal value, `struct Sku(String)` a keyword and text value, each queryable
and rejected exactly where the inner type would be. An **enum** has no inner type, so it requires an
explicit `#[flusso(keyword)]` or `#[flusso(text)]`; omitting it is a compile error, and numeric or
date tags don't exist (use a newtype). `FlussoValue<K>` has a `serde::Serialize` supertrait, so any
`#[derive(FlussoValue)]` type derives `Serialize` too.

**Enum keyword fields stay typed** — never `#[flusso(skip)]` them. Likewise with the **`uuid`
feature**, `uuid::Uuid` is a `keyword` value, so id and foreign-key fields stay `Uuid` and
`User::owner_id().eq(some_uuid)` works without `.to_string()`.

**Variant coverage is checked.** An enum used as a document field is validated against the schema's
declared `variants:`. A Rust variant the schema never lists is a **compile error**, since it could
never match a document. Covering only *some* of the declared variants is a legal partial projection.

Demand **full** coverage with `#[flusso(keyword, exhaustive)]`. Every embedding then requires the
enum to cover the schema's whole declared set, so adding a variant to the schema breaks the build
until the enum catches up. Enum-only: an untagged newtype inherits the flag from its inner type. On a
field with no declared `variants:` the marker is itself a compile error, so a schema edit dropping
them can't silently disarm the guarantee.

## flusso type → Rust type

| flusso `type` | Rust | Handle |
| --- | --- | --- |
| `text` / `identifier` | `String` | `Text` |
| `keyword` | `String` (or a `FlussoValue` newtype) | `Keyword` |
| `enum` | `String` or a `#[derive(FlussoValue)]` enum | `Keyword` / `Enum` |
| `uuid` | `String`, or `uuid::Uuid` (feature) | `Keyword` |
| `boolean` | `bool` | `Bool` |
| `short` / `integer` / `long` | `i16` / `i32` / `i64` | `Number` |
| `float` / `double` | `f32` / `f64` | `Number` |
| `decimal` | `Decimal` (feature) or `f64` *(lossy storage)* | `Number` |
| `date` | `time::Date` / `chrono` (feature) | `Date` |
| `timestamp` | `time::OffsetDateTime` / `chrono` | `Date` |
| `binary` | `String` (base64) | `Binary` |
| `json` | `serde_json::Value` | `Json` |
| `geo` | `GeoPoint { lat, lon }` | `Geo` |
| `object` / `belongs_to` / `has_one` | struct / `Option<struct>` | object namespace |
| `has_many` / `many_to_many` | `Vec<struct>` | `Nested<S,T>` |
| `map` | `HashMap<String, V>` or a `#[derive(FlussoMap)]` type | see [`maps.md`](maps.md) |
| `ids` aggregate | `Vec<i64>` / `Vec<String>` per `element_type` | `Number` / `Keyword` (term queries match arrays) |

Matching is by **leaf identifier and `Option` shape**: the macro compares the final type segment,
not aliases. For exact money, declare a `custom` `scaled_float` in the schema and the derive accepts
`rust_decimal::Decimal`.

## Nullability is checked, not guessed

`T` vs `Option<T>` must match the schema.

- **Non-null:** root and join `primary_key`, a `required: true` leaf, `object`/group, `count`, `ids`
  (a flat `Vec`, empty but never null), to-many joins (empty `Vec`, never null).
- **Nullable:** a `required: false` leaf, `belongs_to` / `has_one`, `avg` / `sum` / `min` / `max`.

Declaring the wrong shape is a derive compile error. Escape hatches: a `serde_json::Value` field
skips type-checking, and `#[flusso(skip)]` drops a field entirely (pair with `#[serde(skip)]` or
`#[serde(default)]`).

## The raw escape hatch

For the few types with no flusso field (`knn`/vector, `geo_shape`, span, parent/child) and
percolators. Most of what once needed `raw` is now typed ([`options.md`](options.md)).

```rust
User::query().raw(serde_json::json!({
    "knn": { "embedding": { "vector": [/* … */], "k": 10 } }
})).send(&client).await?;     // still deserializes into SearchResponse<User>
```

**Out of scope in v1:** search aggregations and facets (use `raw`), writes (flusso owns the index),
cross-index hit correlation, and a scroll cursor (`from`/`size` and `search_after` ship).

## Working reference

`dev/search-api` (crate `flusso-dev-search-api`, axum) derives `FlussoRoot` for users, products and
orders and `FlussoFragment` for every shape below them. `src/shared.rs` holds one `LineItem`
embedded in **two** indexes, plus `FlussoMultiDocument` and `msearch`. In an exported project,
validate against your own `flusso.toml`, not `dev/`.

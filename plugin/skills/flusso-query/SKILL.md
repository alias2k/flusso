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
- Optional features: **`derive`** (the macros), **`decimal`** (`rust_decimal::Decimal`), **`chrono`** / **`time`** (date leaf types — pick one, or use `String` for raw ISO-8601), **`uuid`** (`uuid::Uuid` as a `keyword` value — see below).

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
    #[serde(rename = "orderIds")]
    pub order_ids: Vec<i64>,            // ids aggregate → flat array of PKs, never null
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

## Migrating an existing struct (don't redesign it)

When the task is "migrate this to flusso" / "switch the existing implementation over," the existing document struct is the **spec**, not a starting suggestion:

- **Edit it in place.** Add `FlussoDocument` to the derive list and `#[flusso(index = "…")]` on the *existing* struct — keep its name, module, and visibility. Do **not** scaffold a new parallel struct alongside it; that leaves two document types and breaks every existing consumer.
- **Preserve every field — especially the `id` / primary key.** A migration must produce the **exact** field set the project already has. Don't drop the `id`, don't drop fields you think are "redundant," don't rename. Match each existing field to a schema field; if the leaf Rust type or `Option` shape disagrees with the schema, fix the *schema* or surface the mismatch — never delete the field to make it compile.
- If the existing primary-key field isn't in the schema yet, add it to the schema (`- <type>: id` + `primary_key: id`) rather than removing it from the struct.
- Keep existing `#[serde(rename = …)]` and field ordering; the derive validates by leaf identifier + `Option` shape, so a faithful copy compiles, and a `cargo check` failure tells you exactly which field drifted.

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
| `Keyword` | `eq` `any_of` `prefix` `wildcard` `regexp` `fuzzy` `exists` `asc`/`desc`; subfields `text()` / `keyword_lowercase()` |
| `Text` | `matches` `match_phrase` `match_phrase_prefix` `match_bool_prefix` `matches_fuzzy` `any_of` (exact, via `.keyword`) `exists` `asc`/`desc` (via `.keyword_lowercase`) — **no exact `eq`** (analyzed); subfields `keyword()` / `keyword_lowercase()` |
| `Bool` | `eq` `exists` `asc`/`desc` |
| `Number<T>` | `eq` `any_of` `lt` `lte` `gt` `gte` `between` `exists` `asc`/`desc` |
| `Date` | `eq` `any_of` `lt` `lte` `gt` `gte` `between` `exists` `asc`/`desc` |
| `Object<S>` | `exists` only (same-doc sub-object / to-one join). Query its sub-fields via the **child struct's** flattened handles (`Account::tier()`), not by chaining off this handle. |
| `Nested<S,T>` | `any(q)` / `all(q)` to match parents and **lift** a child query into scope `S`; `matching(q)` (+ `.sort/.size/.from`) to shape the returned array; `exists` |
| `Geo` | `within(distance, center)` `within_box` `within_polygon` `exists`; `distance_from(center)` / `distance_sort(...)` |
| `Binary` | `exists` (base64, not searchable) |
| `Json` | `exists` `raw(serde_json::Value)` |

`sort(…)` accepts sortable handles (numbers, dates, keywords, bools, and now `text` — `Text::asc`/`desc` sort via the case-insensitive `.keyword_lowercase` subfield automatically; use `.keyword().desc()` for an exact-case sort). Geo sorts with `Geo::distance_from(center)` (nearest-first). Cross-field: `multi_match("ada", [User::full_name(), User::bio()])` (weight one with `.boosted(3.0)`).

**Subfield accessors.** flusso's sink auto-enriches `text`/`keyword` fields (`auto_subfields`, on by default) with exact/sortable/searchable subfields, reachable with **no string path**: `User::full_name().keyword()` (exact/`wildcard`/`prefix`), `.keyword_lowercase()` (case-insensitive match/sort), `User::email().text()` (full-text over a keyword). A `wildcard` belongs on `.keyword()`, not the analyzed handle. Valid when `auto_subfields` is on and the field defines no custom `fields`.

**Options & extra query types — the typed surface is broad** (see next section). What's still only reachable via the [`raw`](#escape-hatch) hatch: `knn`/vector, `geo_shape`, span, and parent/child queries — types with no flusso field.

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

## Query options, compound & extra query types

Each leaf operator returns a small **builder** carrying that query's options plus the universal `boost(f32)` and `name(&str)` (`_name`, surfaced in `matched_queries`). With no option set it renders the DSL shorthand; set one and it expands. A builder *is* an `AsQuery`, so it drops straight into a clause — no `.build()`:

```rust
User::query()
    .should(User::full_name().matches("acme").boost(2.0).fuzziness(Fuzziness::Auto))
    .should(User::code().keyword().wildcard("*acme*").case_insensitive())
    .min_should_match(1)                         // make a should-group a real filter
    .filter(User::owner_id().eq(owner_uuid))     // uuid keyword (feature) — no skip
    .filter(User::tier().eq(Tier::Pro))          // enum keyword
    .sort(User::created_at().desc().missing_first())
    .send(&client).await?;
```

Per-type options (all optional): `case_insensitive` on `term`/`prefix`/`wildcard`/`regexp`; `rewrite` (prefix/wildcard); `flags`/`max_determinized_states` (regexp); `fuzziness`/`prefix_length`/`max_expansions`/`transpositions` (fuzzy); `fuzziness`/`operator`/`minimum_should_match`/`prefix_length`/`analyzer`/`zero_terms_query`/`lenient` (`matches`); `slop`/`analyzer` (phrase); `type`/`operator`/`fuzziness`/`tie_breaker`/`minimum_should_match` (`multi_match`); `format`/`time_zone`/`relation` (range); `distance_type`/`validation_method` (geo `within`); `score_mode`/`ignore_unmapped` (nested `any`).

The enumerable params are **closed enums**, not strings (typo → compile error): `Operator { And, Or }` (`operator`/`default_operator`); `Fuzziness { Auto, AutoBounds(u32,u32), Edits(u32) }`; `MultiMatchType` (`multi_match` `type`); `ZeroTermsQuery { None, All }`; `RangeRelation { Intersects, Contains, Within }`; `ScoreMode`/`BoostMode` (function_score); `NestedScoreMode` (nested — has `None` for a filter-only clause). Open-ended params (`analyzer`/`format`/`time_zone`/`minimum_should_match`/`flags`) stay `String`.

> `.or()` / `.and()` / `.not()` on a **builder** need `use flusso_query::AsQuery;` (provided trait methods; inherent `Query` methods are unaffected). Composing via the `Search` clauses needs no import.

- **Bool / scoring:** `Search::min_should_match(n)` (or `Query::min_should_match` on an `or`-group, plus `Query::boost`) turns a top-level free-text `should` group into a real constraint. Free functions: `constant_score(filter)`, `dis_max([..]).tie_breaker(..)`, `boosting(pos, neg, negative_boost)`, `function_score(q).weight(..)/.weight_when(.., filter)/.boost_mode(..)`.
- **Standalone queries** (free fns, each `AsQuery`): `ids([..])`, `query_string(..)`, `simple_query_string(..)`, `combined_fields(.., [fields])`, `script(..)`, `script_score(q, src)`, `distance_feature(..)`, `rank_feature(..)`, `more_like_this([fields], [like])`. (`match_bool_prefix` is a `Text` operator.)
- **Sort builder:** `.asc()/.desc()` then chain `.missing_first()/.missing_last()/.missing(v)`, `.mode(SortMode::..)`, `.unmapped_type(..)/.numeric_type(..)/.format(..)`, `.nested(path)/.nested_filtered(path, q)`; plus `Sort::score()` and `Sort::script(type, src, order)`.
- **Search-level:** `min_score`, `track_total_hits`, `track_scores`, `search_after([..])` (deep pagination), `collapse(field)`, `post_filter(q)`, `highlight(Highlight::new().field(..).pre_tags(..))`.

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

Then `Account::tier().eq(AccountTier::Pro)` works (`String`/`&str` still do). Kind rules: keyword/text accept a unit enum **or** a newtype; number/date accept a **newtype only**. Query-value wiring is currently keyword-only (`eq`/`any_of`); number/date custom types generalize the **doc side** only. A missing `FlussoValue` impl gives a precise "`T` is not a valid value for a `kind::Keyword` field" error.

**Enum keyword fields stay typed — never `#[flusso(skip)]`** them: derive `FlussoValue` on the enum and keep it as the field type. Likewise, with the **`uuid` feature**, `uuid::Uuid` is a `keyword` value — id / foreign-key fields stay as `Uuid` (no skip, no `Keyword::at("…")`), and `User::owner_id().eq(some_uuid)` works without `.to_string()` (the derive defers a `FlussoValue<Keyword>` bound, satisfied by the feature impl).

## flusso type → Rust type (what the derive expects)

| flusso `type` | Rust | Handle |
| --- | --- | --- |
| `text` / `identifier` | `String` | `Text` |
| `keyword` | `String` (or a `FlussoValue` newtype) | `Keyword` |
| `enum` | `String` or a `#[derive(FlussoValue)]` enum | `Keyword` |
| `uuid` | `String`, or `uuid::Uuid` (`uuid` feature) | `Keyword` |
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
| `ids` aggregate | `Vec<i64>` / `Vec<String>` (per `element_type`) | `Number<T>` / `Keyword` (scalar handle — term queries match arrays) |

Matching is by **leaf identifier + `Option` shape** — the macro compares the final type segment, not aliases. Exact money: declare a `custom` `scaled_float` in the schema and the derive accepts `rust_decimal::Decimal` (with the `decimal` feature).

## Nullability is checked, not guessed

`T` vs `Option<T>` must match the schema. Non-null: root/join `primary_key`, `required: true` leaf, `object`/group, `count`, `ids` (a flat `Vec`, empty never null), to-many joins (empty `Vec`, never null). Nullable: `required: false` leaf, `belongs_to`/`has_one`, `avg`/`sum`/`min`/`max`. Declaring the wrong shape is a derive compile error.

Escape hatches from validation: a `serde_json::Value` field skips type-checking; `#[flusso(skip)]` drops a field entirely (pair with `#[serde(skip)]`/`#[serde(default)]`).

## <a id="escape-hatch"></a>The raw escape hatch

For the few types with no flusso field (`knn`/vector, `geo_shape`, span, parent/child) and percolators. Most of what once needed `raw` — `function_score`, `script`, `constant_score`, `query_string`, `search_after`, … — is now in the typed surface.

```rust
User::query().raw(serde_json::json!({
    "knn": { "embedding": { "vector": [/* … */], "k": 10 } }
})).send(&client).await?;     // still deserializes into SearchResponse<User>
```

## Out of scope (v1)

Search aggregations/facets (use `raw`), writes (flusso owns the index — query-only by construction), cross-index hit correlation, and a scroll cursor (`from`/`size` and `search_after` ship).

## Working reference

`dev/search-api` (crate `flusso-dev-search-api`, axum) derives `FlussoDocument` for users/products/orders, plus `FlussoMultiDocument` (`/search`) and `msearch` (`/overview`). Read it for a real consumer — but in an exported project, validate against your own `flusso.toml`, not `dev/`.

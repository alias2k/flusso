# flusso-query — the typed query client

flusso keeps an OpenSearch index in sync with Postgres from a declarative schema
(the write side; see [`README.md`](https://github.com/alias2k/flusso/blob/main/README.md)). That schema is a contract: it
fixes the shape of every document — which fields exist, their types, which are
nested arrays, which are scalars. flusso enforces that contract on the **write**
side; `flusso-query` enforces it on the **read** side.

`flusso-query` does **not** generate the document types for you. You write the
document struct by hand and keep full control — its derives, field types, which
fields it projects, how doc keys map to Rust names. `#[derive(FlussoRoot)]`
then, at compile time and with no database, does two things against the resolved
schema:

1. **Validates** that every struct field lines up with the schema — the field
   exists, its Rust type matches, its nullability matches. A struct that has
   drifted *stops compiling*, pointing at the offending field.
2. **Generates the typed query surface** from the schema — field handles,
   `get`/`query`, the schema hash — so a service queries the index like a typed
   function: field names checked at compile time, operators that only exist for
   the field types that support them, results that deserialize into your struct.

The query surface comes from the **full schema**, not from the struct — so you can
filter or sort on a field even if your struct doesn't deserialize it. When the
schema changes and the index is rebuilt, anything that no longer fits stops
compiling at `cargo build`.

### One root, many fragments

Exactly one type binds to an index: the **root**, `#[derive(FlussoRoot)]`. It is
the only thing that reads `flusso.toml`, and it owns the *whole* typed surface for
that index — a handle for every field at every level, not just its own.

Every other shape is a **fragment**, `#[derive(FlussoFragment)]`: no index, no
path, no schema of its own. It describes a shape and nothing more, so one
declaration covers every place that shape appears — two paths in one index, two
different indexes, or a shared crate other services depend on. Each root that
embeds it validates it against the mapping *at that path*, recursively.

```rust
#[derive(serde::Deserialize, FlussoFragment)]         // no location at all
struct Address { city: String, zip: Option<String> }

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    billing_address:  Address,     // checked against users.billingAddress
    shipping_address: Address,     // checked against users.shippingAddress
}
```

Retype one of `Address`'s fields and you get **two** errors — one per embedding.
That is the whole point: the shape is written once, and nothing can drift.

## Quick reference

| Looking for… | Jump to |
| --- | --- |
| A full worked example (structs + query) | [A query, from the caller's seat](#a-query-from-the-callers-seat) |
| Which operators each field type exposes | [What the field type buys you](#what-the-field-type-buys-you) |
| `and`/`or`/`not`, clause vs combinator style, `count`/`ids` | [Composing queries](#composing-queries) |
| Per-query options, compound/scoring, standalone, sort, search-level | [Query options and extra query types](#query-options-and-extra-query-types) |
| Nested arrays — filter *by* vs filter *of* (`any`/`all`, `filter_nested`) | [Filtering nested collections](#filtering-nested-collections) |
| Several searches, one round-trip | [`_msearch`](#several-searches-one-round-trip-_msearch) |
| One blended result list across indexes | [Combined search](#one-blended-result-list-combined-search) |
| How the macro finds the schema; `path` for nested structs | [Binding to the schema](#binding-to-the-schema) |
| flusso `type` → Rust type → handle | [flusso types → Rust types](#flusso-types--rust-types) |
| Reusing a shape across paths or indexes | [Fragments](#fragments-a-shape-with-no-location) |
| Coming from `path = "…"` | [Migrating from `path`](#migrating-from-path) |
| Anything the typed builder can't express | [The escape hatch](#the-escape-hatch) |
| Reading a prefixed deployment | [Resolving the index name](#resolving-the-index-name) |

---

## A query, from the caller's seat

A service that searches the `users` index from the [schema guide](https://alias2k.github.io/flusso/guides/schema-authoring.html). You write
the structs; `#[derive(FlussoRoot)]` validates them and generates the query
surface. The only input is the **index name** — it finds `flusso.toml` itself (see
[Binding to the schema](#binding-to-the-schema)).

### The document structs

```rust
use flusso_query::{Client, FlussoFragment, FlussoRoot};

/// A `users` document — *you* write this. It's a **projection**: it deserializes
/// the fields below and omits the rest of the index (addresses, profile,
/// avgOrderValue, …), which the derive allows. The derive checks every field
/// against the `users` schema and hangs the typed query surface off `User`.
#[derive(Debug, Clone, serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]                     // ← the only input: which index
pub struct User {
    pub id: i32,                                // primary key (integer) — never null
    pub email: String,                          // keyword, required → never null
    #[serde(rename = "fullName")]
    pub full_name: Option<String>,              // text, not required → nullable
    pub account: Account,                       // an object — always assembled
    pub orders: Vec<Order>,                     // has_many join → nested, never null
    #[serde(rename = "orderCount")]
    pub order_count: i64,                        // count aggregate → long, never null
    #[serde(rename = "lifetimeValue")]
    pub lifetime_value: Option<f64>,            // sum aggregate → nullable
}

/// The `account` object — a same-row sub-object. A fragment names no location:
/// it is validated wherever `User` puts it, and has no entry points of its own.
#[derive(Debug, Clone, serde::Deserialize, FlussoFragment)]
pub struct Account {
    pub tier: String,                           // enum → keyword, required
    pub country: Option<String>,                // keyword, not required
    #[serde(rename = "createdAt")]
    pub created_at: time::OffsetDateTime,       // timestamp, required
}

#[derive(Debug, Clone, serde::Deserialize, FlussoFragment)]
pub struct Order {
    pub status: String,                         // enum, required
    pub total: Decimal,                         // decimal column (`decimal` feature); or `f64`
    #[serde(rename = "placedAt")]
    pub placed_at: time::OffsetDateTime,        // timestamp, required
    pub items: Vec<Item>,                       // a deeper has_many → nested
}

/// Reached by recursion from `Order` — `Item` never names `orders.items` either.
#[derive(Debug, Clone, serde::Deserialize, FlussoFragment)]
pub struct Item {
    #[serde(rename = "productId")]
    pub product_id: i32,
    pub quantity: i32,
    #[serde(rename = "unitPrice")]
    pub unit_price: Decimal,
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
    let page = User::query()
        .filter(User::email().eq("ada@example.com"))      // keyword → exact
        .filter(User::order_count().gte(5))               // long → range
        .filter(User::account().tier().eq("gold"))        // into the object's handles
        .query(User::full_name().matches("ada lovelace")) // text → analyzed match
        .filter(User::orders().any(UserOrders::status().eq("delivered")))  // nested, via the generated namespace
        .filter(User::addresses().any(UserAddresses::city().eq("Boston"))) // never projected — still queryable
        .sort(User::order_count().desc())
        .from(0)
        .size(20)
        .send(&client)
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

The compiler refuses the mistakes:

- `User::email().matches(..)` — `matches` is a text operator and `email` is a
  `keyword`.
- `User::full_name().eq(..)` — `full_name` is analyzed `text`, so it has no exact
  `eq`.
- `User::nmae()` — there is no such handle.
- `hit.source.totl` — no such field.

And the struct itself can't drift. Declaring `email: i32`, or `email:
Option<String>` when the field is `required`, or a field `totl` the schema doesn't
have, is a **compile error from the derive**. The schema has been lifted into the
type system, so the compiler checks both your queries and your struct against it.

> `OS_PASSWORD` is just a variable name picked for the example. The full env-var
> story (secrets, the reserved deployment overrides, `FLUSSO_CONFIG`) lives in the
> [configuration guide](https://alias2k.github.io/flusso/guides/configuration.html).

---

## What the field type buys you

The handle a schema field generates determines the operators in scope: an operator
that doesn't make sense for a field's type *doesn't exist* on its handle, so the
mistake is a compile error rather than a 400 from OpenSearch.

| Handle          | Operators                                                                 |
| --------------- | ------------------------------------------------------------------------- |
| `Keyword`       | `eq`, `any_of`, `prefix`, `wildcard`, `regexp`, `fuzzy`, `exists`         |
| `Enum`          | An ordered-enum keyword (a field with a declared `variants` order): `eq`, `any_of`, `exists`, `keyword()` (the full `Keyword` surface), plus `asc`/`desc` that sort by the **declared order** (not alphabetically) via the prebaked `.sort` subfield. A bare enum with no declared order is a plain `Keyword`. |
| `Text`          | `matches`, `match_phrase`, `match_phrase_prefix`, `matches_fuzzy`, `any_of` (exact, via `.keyword`), `exists` — *no* exact `eq` (it's analyzed) |
| `Bool`          | `eq`, `exists`, `asc`/`desc`                                              |
| `Number`        | `eq`, `any_of`, `lt`, `lte`, `gt`, `gte`, `between`, `exists`             |
| `Date`          | `eq`, `any_of`, `lt`, `lte`, `gt`, `gte`, `between`, `exists`             |
| object namespace | A same-document sub-object — an `object` field or a to-one join (`belongs_to`/`has_one`). Objects are *flattened*, so the namespace **chains** from its parent and its fields query in the enclosing scope: `User::account().tier()`, `User::account().exists()` |
| `Nested<S, T>`  | `any(q)` / `all(q)` to match parents and **lift** the child query into scope `S` — `q` is a child query built from `T`'s handles ([merging](#building-a-child-filter-and-merging-it-into-the-parent)); `matching(q)` (with `.sort`/`.size`) to shape what's returned — see [Filtering nested collections](#filtering-nested-collections); plus `exists` |
| `Geo`           | `within(Distance::km(12.0), center)`, `within_box`, `within_polygon`, `exists`; sort with `distance_from(center)` (nearest-first sugar) or `distance_sort(center, order, DistanceUnit)` |
| `TextMap`       | `key(k)` → a `Text` leaf for one key; `search(q)` (cross-key, `.prefer(key, weight)` / `.only_preferred()`); `has_key(k)`, `exists` |
| `KeywordMap`    | `key(k)` → a `Keyword` leaf for one key; `has_key(k)`, `exists` — *no* `search` (exact-match, use `key(..).eq(..)`) |
| `NumberMap`     | `key(k)` → a `Number` leaf for one key (`eq`/`lt`/`gte`/`between`/…); `has_key(k)`, `exists` |
| `DateMap`       | `key(k)` → a `Date` leaf for one key (`eq`/`gte`/`between`/…); `has_key(k)`, `exists` |
| `Binary`        | `exists` — base64-encoded, not searchable                                 |
| `Json`          | `exists`, `raw(serde_json::Value)` — the untyped fallback                 |

Each operator's argument is typed by **kind**, not by one fixed Rust type — a
handle accepts any value implementing `FlussoValue<kind::…>` (which requires
`serde::Serialize`):

- **Numerics are split per type** (`kind::Byte`/`Short`/`Integer`/`Long`/`Float`/
  `Double`/`Decimal`), and a value is accepted only if it widens into that kind
  **without loss**. So a `Long` field takes any integer, a `Double` takes `f32`/
  `f64` or a small int, a `Decimal` takes `Decimal` (`decimal` feature) or an
  integer — but a float on an integer field, or an `i64` on a `Short`, is a
  *compile error*. Bare literals work where lossless: `eq(5)` on `long`/`integer`/
  `double`/`decimal`, `eq(5.0)` on `float`/`double`. A `byte`/`short` field needs
  a typed literal (`eq(5i16)`), since `5` defaults to `i32` (which would narrow).
- `Keyword` takes a `String`/`&str` or a `#[derive(FlussoValue)]` keyword
  enum/newtype (matched against its serde string form).
- `Bool` takes a `bool` or a bool `#[derive(FlussoValue)]` newtype.
- `Date` takes a `String`/`&str` ISO-8601 literal or — with the `chrono` feature
  — a `NaiveDate` / `NaiveDateTime` / `DateTime<Utc>`.

A custom money/quantity type queries with no cast — `Order::total().eq(Money(d))`
— as long as it's a `FlussoValue` of the field's kind (see below). A value of the
wrong kind is a compile error (`Order::created_at().gte(5)`, `Order::age().eq(1.5)`).

**Subfield accessors.** flusso's sink auto-enriches `text`/`keyword` fields
(`auto_subfields`, on by default) with exact / sortable / searchable subfields,
and the handles expose them — no `Keyword::at("code.keyword")` string path:

```rust
User::full_name()                      // Text   → analyzed full-text match
User::full_name().keyword()            // Keyword → exact / wildcard / prefix
User::full_name().keyword_lowercase()  // Keyword → case-insensitive match / sort
User::email().text()                   // Text    → full-text over a keyword field
```

(A `wildcard` belongs on `.keyword()`, not the analyzed handle, which matches
tokens not the whole value.) These accessors exist only when the subfield is
actually provisioned — and the derive **enforces it at compile time**: a
`text`/`keyword` handle is stamped with subfields only when every OpenSearch
sink has `auto_subfields` on and the field declares no custom `fields`.
Otherwise the handle is `…<NoSubfields>` and `.keyword()` / `.text()` /
`.keyword_lowercase()` (and the `Text::any_of` / `Text::asc` sugar built on
them) simply don't exist — calling one is a compile error, not a runtime 400.

Sorting is similar — `sort(…)` accepts handles whose type is sortable (numbers,
dates, keywords, booleans, and `text`). `Text::asc`/`desc` is sugar that sorts
via the case-insensitive `.keyword_lowercase` subfield automatically; reach for
`User::full_name().keyword().desc()` when you want an exact-case sort instead.
A `geo_point` sorts by proximity with `User::location().distance_from(center)`.

A few clauses span more than one field, so they're free functions:
`multi_match("ada", [User::full_name(), User::bio()])` runs one analyzed query
across several `Text` fields (weight one with `User::full_name().boosted(3.0)`).

The typed surface is broad — see [Query options and extra query
types](#query-options-and-extra-query-types) for the per-query options
(`boost`/`case_insensitive`/`fuzziness`/…), the compound/scoring queries
(`constant_score`/`dis_max`/`function_score`/`boosting`), the standalone ones
(`ids`/`query_string`/`simple_query_string`/`script_score`/…), and the
search-level controls (`min_score`/`collapse`/`search_after`/`highlight`/…).
What's left for the [`raw`](#the-escape-hatch) hatch is `knn`, `geo_shape`, span
and parent/child queries — types with no corresponding flusso field.

### Composing queries

A handle's operator produces a `Query<S>` — carrying the **scope** it was built
in. The root document and any flattened `object`/to-one-join sub-field share the
scope `Root`; only a `nested` array introduces a fresh scope, tagged with its
element type (`Query<Order>`).

Queries compose with `and` / `or` / `not` *within a scope*, and the `Search`
builder exposes the bool-query clauses directly:

```rust
// Combinator style.
let q = User::email().eq("ada@example.com")
    .and(User::order_count().gte(5))
    .and(User::orders().any(Order::status().eq("delivered").and(Order::total().gt(0.0))));

User::query().query(q).send(&client).await?;

// Clause style — `filter`/`must_not` don't score, `query`(=must)/`should` do.
User::query()
    .query(User::full_name().matches("ada"))       // scored
    .filter(User::order_count().gte(5))            // filtered, cached, no score
    .must_not(User::email().prefix("test-"))
    .should(User::orders().any(Order::status().eq("delivered")))
    .send(&client)
    .await?;
```

`query`/`filter`/`must_not`/`should` accept anything that is a `Query<Root>`, so
the two styles mix freely. Which clause to reach for:

| Clause | Scores? | Bool slot | Use for |
| --- | --- | --- | --- |
| `query` | yes | `must` | full-text relevance you want ranked |
| `filter` | no (cached) | `filter` | exact constraints — terms, ranges, nested `any` |
| `must_not` | no | `must_not` | exclusions |
| `should` | yes | `should` | optional boosts; a real constraint with `min_should_match` |

A built search can finish as a **count** instead of a page: `.count()` sends the
same query to `_count` and returns the number of matching documents (`u64`) —
cheaper than `send()` when the hits aren't needed. Sort, `from`/`size`, and
`filter_nested` projections are ignored; they never change which documents match.

```rust
let open_orders: u64 = User::query()
    .filter(User::orders().any(Order::status().eq("open")))
    .count(&client)
    .await?;
```

Or as an **id page**: `.ids()` runs the same search with `_source: false` and
returns `Vec<String>` — the matching document ids (root primary keys,
stringified), in order, no sources fetched. Sort and `from`/`size` apply as in
`send()`; `filter_nested` projections are dropped. The cheap way to feed another
lookup — e.g. search in OpenSearch, then load the full rows from Postgres:

```rust
let user_ids: Vec<String> = User::query()
    .filter(User::orders().any(Order::status().eq("open")))
    .sort(User::order_count().desc())
    .size(100)
    .ids(&client)
    .await?;
```

### Query options and extra query types

Each leaf operator returns a small **builder** that carries that query's options
plus the universal `boost(f32)` and `name(&str)` (`_name`, surfaced in a hit's
`matched_queries`). With no option set it renders the DSL shorthand; set one and
it expands. A builder drops straight into a clause (it's an `AsQuery`), so no
`.build()` is needed:

```rust
User::query()
    .should(User::full_name().matches("acme").boost(2.0))         // weighted text
    .should(User::full_name().keyword().wildcard("*acme*").case_insensitive())
    .should(User::full_name().matches("acme").fuzziness(Fuzziness::Auto))  // typo-tolerant
    .min_should_match(1)                                          // make should a real filter
    .filter(User::owner_id().eq(owner_uuid))                      // uuid keyword (feature)
    .filter(User::tier().eq(Tier::Pro))                           // enum keyword
    .sort(User::created_at().desc().missing_first())              // null-aware sort
    .send(&client).await?;
```

The options per query type (all optional; plus the universal `boost`/`name` on every builder):

| Query | Options |
| --- | --- |
| `term` / `prefix` / `wildcard` / `regexp` | `case_insensitive` |
| `prefix` / `wildcard` | `rewrite` |
| `regexp` | `flags`, `max_determinized_states` |
| `fuzzy` | `fuzziness`, `prefix_length`, `max_expansions`, `transpositions` |
| `matches` | `fuzziness`, `operator`, `minimum_should_match`, `prefix_length`, `analyzer`, `zero_terms_query`, `lenient` |
| phrase matches | `slop`, `analyzer` |
| `multi_match` | `type`, `operator`, `fuzziness`, `tie_breaker`, `minimum_should_match` |
| range | `format`, `time_zone`, `relation` |
| `within` (geo) | `distance_type`, `validation_method` |
| nested `any` | `score_mode`, `ignore_unmapped` |

The enumerable options are **closed enums**, not strings — a typo is a compile
error, not a 400. `operator`/`default_operator` take `Operator { And, Or }`;
`fuzziness` takes `Fuzziness { Auto, AutoBounds(u32, u32), Edits(u32) }`;
`multi_match`'s `type` takes `MultiMatchType`; `zero_terms_query` takes
`ZeroTermsQuery { None, All }`; a range `relation` takes `RangeRelation
{ Intersects, Contains, Within }`; `function_score`'s `score_mode`/`boost_mode`
take `ScoreMode`/`BoostMode`; a nested `score_mode` takes `NestedScoreMode`
(which, unlike `ScoreMode`, has `None` for a filter-only nested clause). Geo's
`distance_type`/`validation_method` take `DistanceType`/`ValidationMethod`, and
a sort's `numeric_type` / `Sort::script` type take `NumericType` /
`ScriptSortType`. `minimum_should_match` takes a `MinimumShouldMatch`
(`2`/`.into()` for a count, `MinimumShouldMatch::percent(75)`, or `::raw("3<90%")`
for the combining mini-language). Genuinely open-ended params (`analyzer`,
`format`, `time_zone`, `unmapped_type`, `flags`) stay `String`.

> **`.or()` / `.and()` on a builder** need `use flusso_query::AsQuery;` in scope
> (the combinators are provided methods on that trait). Composing via the
> `Search` clauses (`.should()`/`.filter()`/…) needs no import.

**Bool / compound & scoring.** `Search::min_should_match(n)` (or
`Query::min_should_match` on an `or`-group) turns a top-level free-text `should`
group into a real constraint instead of scoring-only. The scoring wrappers are
free functions: `constant_score(filter)`, `dis_max([..]).tie_breaker(..)`,
`boosting(positive, negative, negative_boost)`, and
`function_score(query).weight(..)/.weight_when(.., filter)/.boost_mode(..)`.

**Standalone query types** (free functions, each an `AsQuery`): `ids([..])`
(get-many-by-`_id`), `query_string(..)` / `simple_query_string(..)` /
`combined_fields(.., [fields])` (user-facing full-text), and the relevance ones
`script(..)`, `script_score(query, source)`, `distance_feature(..)`,
`rank_feature(..)`, `more_like_this([fields], [like])`. `match_bool_prefix` is a
`Text` operator (search-as-you-type).

**Sort.** `.asc()`/`.desc()` on a sortable handle (number, date, keyword, bool,
`text` — the last routing through `.keyword_lowercase` — or an ordered `enum`,
which sorts by its declared `variants` order via the prebaked `.sort` subfield)
return a `Sort`
builder: chain `.missing_first()`/`.missing_last()`/`.missing(v)`,
`.mode(SortMode::..)`, `.unmapped_type(..)`/`.numeric_type(..)`/`.format(..)`, or
`.nested(path)`/`.nested_filtered(path, q)`. Also `Sort::score()` (by `_score`),
`Sort::script(ScriptSortType, source, order)`, and `Geo::distance_from(center)`
/ `Geo::distance_sort(center, order, DistanceUnit)` for proximity sorting. A
geo radius is a typed `Distance` (`Distance::km(12.0)` / `::miles(5.0)` /
`::meters(800.0)` / …), so a malformed `"12 km"` can't reach the query.

**Search-level** controls on the `Search` builder: `min_score`,
`track_total_hits`, `track_scores`, `search_after([..])` (deep pagination),
`collapse(field)`, `post_filter(q)`, and `highlight(Highlight::new().field(..))`.

### Queries are values; the client appears once

`Type::query()` takes no client: a `Search<T>` is a plain, `Clone`able value with
no lifetime. Build it anywhere — a helper, a struct field, a cached "prepared
search" — and hand a `&Client` to the terminal (`send` / `ids` / `count`, all
`&self`) when it's time to run. One built query can run many times, against
different clients, or with per-call tweaks via `clone()`:

```rust
fn busy_users() -> Search<User> {                      // no client in sight
    User::query().filter(User::order_count().gte(5))
}

let page = busy_users().send(&client).await?;          // run it
let next = busy_users().from(20).send(&client).await?; // tweak a copy
```

### Several searches, one round-trip (`_msearch`)

Independent typed searches — different indexes, different document types — can
share one HTTP round-trip. `client.msearch(…)` takes a tuple of `&Search<T>`
(arity 1–8) and returns one typed `SearchResponse` per slot, in order:

```rust
let users_q  = User::query().query(User::full_name().matches(&q)).size(10);
let orders_q = Order::query().filter(Order::status().eq("open")).size(5);

let (users, orders) = client.msearch((&users_q, &orders_q)).await?;
```

The "search page with separate sections" primitive: each slot keeps its own query,
sort, pagination, and `filter_nested` projections, and decodes into its own type.
A slot-level failure fails the whole call with an error naming the slot (no partial
results). For many searches of *one* type there's
`client.msearch_all(&searches)` → `Vec<SearchResponse<T>>`.

### One blended result list (combined search)

The other multi-index shape: **one** query over several indexes, hits ranked
together in a single list — the "one search box over everything" primitive. You
declare which document types blend by writing an enum with one variant per type:

```rust
/// One item in the storefront's global search.
#[derive(Debug, FlussoMultiDocument)]          // the `derive` feature, like FlussoRoot
enum StoreItem {
    User(User),
    Order(Order),
}

let page = StoreItem::query()                  // MultiSearch<StoreItem> — client-free too
    .query(multi_match("ada", [User::full_name(), Order::customer_name()]))
    .size(20)
    .send(&client)
    .await?;

for hit in page.hits {
    match hit.source {                         // dispatched by the hit's `_index`
        StoreItem::User(u) => render_user(u, hit.score),
        StoreItem::Order(o) => render_order(o, hit.score),
    }
}
```

Root-scope queries compose across document types (`Query<Root>` carries no
document type), and each hit is decoded into the variant that owns its index.
The query goes through the stable `{INDEX}_{HASH}` alias, but OpenSearch reports
the concrete index behind it (`{INDEX}_{HASH}_{generation}`) in each hit — so the
crate normalizes that generation suffix before dispatch; you just match on
`hit.source`. A hit from an index no variant claims is an error, not a skip.
`count(&client)` works on the union too.

Two semantics to know:

- A *query* on a field that exists in only one of the indexes is fine — it just
  doesn't match in the others.
- A *sort* on such a field is **rejected by OpenSearch** unless it carries an
  `unmapped_type`. Sort the blended list by relevance, or on fields all the
  union's indexes share.

The derive validates the enum's shape (single-field tuple variants, no duplicate
payload types) and generates the trait's two members. Without the `derive`
feature, the impl is two short members written by hand — a `TARGETS` const listing
each variant's `(INDEX, SCHEMA_HASH)` and a `decode` that matches on
`Type::physical_index()`.

### Building a child filter and merging it into the parent

Because the scope is part of the type, a query is a value you can build, name,
store, and reuse. A **nested** array introduces its own scope, and the root
generates a namespace for it — `UserOrders` for `users.orders` — whose handles are
tagged with that scope, so they produce `Query<UserOrders>`, not `Query<Root>`:

```rust
// Built from the generated namespace. Reusable — a plain function returning a query:
fn big_delivered() -> Query<UserOrders> {
    UserOrders::status().eq("delivered")
        .and(UserOrders::total().gt(100.0))
}
```

To merge a child filter into a parent, **lift** it through the nesting that holds
it: `User::orders().any(child)` (or `.all(child)`) takes a `Query<UserOrders>` and
returns a `Query<Root>` — a nested clause at the `orders` path — which composes
with parent-scope queries like any other:

```rust
let q = User::email().eq("ada@example.com")
    .and(User::orders().any(big_delivered()));   // Query<UserOrders> → lifted → Query<Root>

User::query().filter(q).send(&client).await?;
```

The scope tag keeps this honest: `User::email().and(UserOrders::status().eq(…))`
**does not compile** — you can't `and` a `Query<Root>` with a `Query<UserOrders>`;
the child query has to be lifted through `User::orders()` first. A child constraint
can never be silently applied at the wrong level.

Lifting composes through depth: `UserOrders::items().any(UserOrdersItems::quantity().gt(1))`
is a `Query<UserOrders>`, which `User::orders().any(…)` then lifts the rest of the
way to `Query<Root>`.

### Optional filters

Callers build queries from optional inputs — request params, form fields — and the
`if let Some(x) = … { q = q.filter(…) }` dance breaks the fluent chain. The
primitive that fixes it: **`Option<Q>` is itself a `Query`**, where `None`
contributes nothing in any clause — `must_not(None)` excludes nothing, `and(None)`
is the identity. So every clause and combinator accepts an optional; you `.map` the
value into the handle:

```rust
User::query()
    .filter(params.email.map(|e| User::email().eq(e)))          // skipped when None
    .filter(params.min_orders.map(|n| User::order_count().gte(n)))
    .send(&client).await?;

// Composes inside and/or too — a None branch just drops out:
let q = User::email().eq("ada@example.com")
    .and(params.tier.map(|t| User::account().tier().eq(t)));    // None → just the email clause
```

A named `filter_some(value, |v| …)` sugar that drops the `.map` is an obvious
follow-on, left out of the first cut.

---

## Filtering nested collections

`orders` is a nested array, and there are two **independent** things you might
filter — flusso keeps them separate:

- **Filter *by* nested** — choose which *users* come back, based on their orders.
  The `any`/`all` you've already seen: it's a `Query`, so it goes in
  `filter`/`query`/etc. A matching user still carries its **whole** `orders` array.
- **Filter *of* nested** — shape the `orders` array each user comes back with,
  without changing which users return. A separate clause, `filter_nested`.

They compose: use either alone, or both together (often with the same predicate).

### `filter_nested` — shaping the returned array

```rust
let page = User::query()
    // filter BY: only users with a delivered order
    .filter(User::orders().any(Order::status().eq("delivered")))
    // filter OF: and within each, keep only the delivered orders, newest first, ≤5
    .filter_nested(
        User::orders()
            .matching(Order::status().eq("delivered"))
            .sort(Order::placed_at().desc())
            .size(5),
    )
    .send(&client).await?;

for hit in &page.hits {
    // `source.orders` IS the filtered subset — no extra accessor:
    for order in &hit.source.orders {       // delivered, newest first, ≤ 5
        println!("{} — {}", order.total, order.status);
    }
}
```

`User::orders().matching(q)` is a nested **projection**: `q` is a `Query<Order>`
built from `Order`'s handles, plus optional `.sort(Order::field().desc())`,
`.size(n)`, `.from(n)`. `matching` itself is optional — drop it to keep every child
but still sort or cap the array.

Because `filter_nested` does **not** touch which parents match, a user with no
delivered orders still comes back — with `orders: []`. Pair it with a
`filter(User::orders().any(…))` when you also want to drop those users.

`filter_nested` always **replaces** `hit.source.<path>` with the matched subset:
the client fetches the nested matches and substitutes them for that field before
deserializing.

> **Not yet built:** a `.keep_source()` opt-out that leaves the stored array intact
> and a typed `hit.nested(handle)` side accessor. Today `filter_nested` always
> replaces the array in `source`.

### Depth

`filter_nested` shapes one nested level — `orders`. You can still *match* on deeper
nesting from inside the predicate
(`Order::items().any(Item::quantity().gt(1))`), and the returned orders honor it;
returning a filtered `items` array *inside* each returned order is a deeper
inner-hits case left to the `raw` hatch for now.

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
struct you wrote. There is no `serde_json::Value` in the common path.

---

## Binding to the schema

The macro validates against the **resolved mapping** — flusso's
[`IndexMapping`](https://github.com/alias2k/flusso/blob/main/libs/0-core/src/config/index_mapping.rs): every field with a
concrete type, whether it is **nullable**, its nested `children`, and the schema
`hash`.

flusso's schemas are **self-describing**: every leaf declares its `type` and
whether it's `required`, and joins/objects/aggregates have structural types — so the
mapping resolves with **no database**, exactly as `flusso build` does when it writes
`flusso.lock`. The client reuses that resolution. (See the
[schema guide](https://alias2k.github.io/flusso/guides/schema-authoring.html) for the
schema format and the [configuration guide](https://alias2k.github.io/flusso/guides/configuration.html)
for how the index is written.)

### The one input: the index name

You never point the macro at a file. You name the **index**, and the macro finds
the schema:

```rust
#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
pub struct User { /* … */ }
```

At compile time the macro:

1. **Locates `flusso.toml`** by walking up from the consuming crate's
   `CARGO_MANIFEST_DIR`. (Override with `#[flusso(index = "users", config = "…")]`
   or the `FLUSSO_CONFIG` env var — see the
   [configuration guide](https://alias2k.github.io/flusso/guides/configuration.html).)
2. **Selects the `[[index]]`** whose `name` matches `"users"` — the reason an index
   name is required, since one `flusso.toml` defines several.
3. **Loads that index's `schema:` file** (resolved relative to `flusso.toml`) and
   **resolves the `IndexMapping`** in-process — the same resolution `flusso build`
   performs. Hermetic: no database, no network.
4. **Tracks `flusso.toml` and every schema file it read** as build inputs (via
   `include_bytes!`), so editing the config or a schema retriggers compilation and
   a drifted struct fails the next build.

The resolved mapping's content hash is the binding's `SCHEMA_HASH` — the **same
hash** `flusso build` writes into `flusso.lock` and the engine folds into the
physical index name, so binding and index are provably the same schema version.

There is no `build.rs`, no generated `.rs` file to `include!`, and no committed
mapping artifact to keep in sync — the struct is the only file you maintain.

### Fragments: a shape with no location

Everything below the root is a **fragment**. It names no index and no path:

```rust
#[derive(serde::Deserialize, FlussoFragment)]
pub struct Item { /* … */ }
```

A fragment never resolves a schema. The root that embeds it hands it the resolved
mapping level for the path it landed on, and it checks itself against that —
recursively, so `User { orders: Vec<Order> }` and `Order { items: Vec<Item> }`
validate `Item` against `users.orders.items` without either struct naming it.

Because the location comes from the use site, **one fragment serves every place
the shape appears**:

```rust
#[derive(serde::Deserialize, FlussoFragment)]
pub struct LineItem { /* … */ }

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
pub struct User { orders: Vec<UserOrder> }   // → users.orders.items

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "orders")]
pub struct Order { items: Vec<LineItem> }    // → orders.items
```

Both are checked. If the two schemas drift apart, whichever no longer fits fails
the build, pointing at that embedding.

**Embedding is checked by default.** A field whose type is a struct must be a
fragment (or a value type). To embed a plain, un-derived struct deliberately, mark
the field `#[flusso(opaque)]` — that skips the shape check, though the field must
still exist in the mapping.

**A flattened group** works the same way, against the enclosing level:

```rust
#[derive(serde::Deserialize, FlussoFragment)]
struct Timestamps { created_at: String, updated_at: String }

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    #[serde(flatten)]
    ts: Timestamps,      // `createdAt`/`updatedAt` are checked at the ROOT level
}
```

Only the root gets entry points (`get`/`query`) and `SCHEMA_HASH`. A fragment
cannot start a search — it has no index to search.

### Migrating from `path`

Nothing is deleted: `#[flusso(path = "…")]` and `#[derive(FlussoDocument)]` are
both **deprecated but still working**, so existing declarations keep compiling
and warn with the replacement. A `path` struct still validates against its
level — it just generates no handles and no `query`/`get`, since the surface
lives on the root now. So the migration is mechanical, and the compiler walks
you through it:

| Before | After |
| ------ | ----- |
| `#[derive(FlussoDocument)] #[flusso(index = "users")]` on the root | `#[derive(FlussoRoot)] #[flusso(index = "users")]` |
| `#[derive(FlussoDocument)] #[flusso(index = "users", path = "orders")]` | `#[derive(FlussoFragment)]` — drop the whole attribute |
| `Order::status()` (child struct's handle) | `UserOrders::status()` (root-generated namespace) |
| `Account::tier()` (object child struct) | `User::account().tier()` (chains from the parent) |

Both deprecations carry the replacement in their note, so `cargo build` names
what to change and where. Only the call sites in the last two rows are a hard
error; the struct declarations keep compiling until you get to them.

### What the derive expands to

`#[derive(FlussoRoot)]` on `User` emits (roughly):

```rust
impl User {
    // Entry points (from the `FlussoRoot` trait).
    pub fn get(client: &Client, id: i32) -> impl Future<Output = Result<Option<User>>>;
    pub fn query() -> Search<User>;   // client-free: a plain, reusable value

    // Field handles — one per *schema* field, carrying its type. These exist for
    // every field in the mapping, whether or not `User` projects it.
    pub fn id() -> Number<kind::Integer> { /* … */ }
    pub fn email() -> Keyword { /* … */ }
    pub fn full_name() -> Text { /* … */ }
    pub fn account() -> UserAccount { /* … */ }             // object → its namespace, chained
    pub fn addresses() -> Nested<Root, UserAddresses> { /* … */ }
    pub fn orders() -> Nested<Root, UserOrders> { /* … */ }
    pub fn order_count() -> Number<kind::Long> { /* … */ }
    // …one per schema field.
}

impl FlussoRoot for User {
    const INDEX: &str = "users";
    const SCHEMA_HASH: &str = "3f2a1b9c…";   // the physical index is INDEX_HASH
}
```

Plus **one namespace per container level**, generated from the schema and named
root-prefixed so two roots in the same module can't collide.

An **object** flattens into its enclosing scope, so its namespace chains from the
parent — its fields are `&self` methods returning handles in that same scope:

```rust
pub struct UserAccount;
impl UserAccount {
    pub fn exists(&self) -> Query<Root> { /* … */ }
    pub fn tier(&self) -> Enum<Root> { /* … */ }        // at "account.tier"
    pub fn country(&self) -> Keyword<Root> { /* … */ }
}
// → User::account().tier().eq("gold")
```

A **nested** array introduces its own scope, so its namespace is a named type with
associated fns, and it carries the `PATH` a nesting-aware sort reads:

```rust
pub struct UserOrders;
impl FlussoScope for UserOrders {
    const PATH: &[Segment] = &[Segment { name: "orders", kind: SegmentKind::Nested }];
}
impl UserOrders {
    pub fn status() -> Enum<UserOrders> { /* … */ }
    pub fn total() -> Number<kind::Decimal, UserOrders> { /* … */ }
    pub fn items() -> Nested<UserOrders, UserOrdersItems> { /* … */ }   // deeper nesting
    pub fn shipping() -> UserOrdersShipping { /* … */ }                 // object inside nested
}
// → User::orders().any(UserOrders::total().gt(100))
// → User::orders().any(UserOrders::shipping().carrier().eq("dhl"))
```

That last one — an object *inside* a nested array — works because the root knows
the scope tag. A child struct never could, which is why it used to be rejected.

Namespace names are root-prefixed so two roots in one module can't collide. If a
generated name still clashes with a type you already have — or a deep chain gets
unwieldy — rename it on the field, and everything under it follows:

```rust
#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    #[flusso(scope = "Purchases")]
    orders: Vec<Order>,      // → `Purchases::total()`, `PurchasesItems::quantity()`
}
```

Finally, the root bakes the resolved mapping into const data and drives the check
into every shape it embeds:

```rust
const _: () = {
    const __FLUSSO_LEVEL: &[FieldSpec] = &[ /* the whole resolved subtree */ ];
    Account::__flusso_check(children(__FLUSSO_LEVEL, "account"));
    UserOrder::__flusso_check(children(__FLUSSO_LEVEL, "orders"));
};
```

### What the derive checks

For each field the struct declares, the macro resolves the matching schema field by
its **document key** — honoring `#[serde(rename = "…")]` and a container
`#[serde(rename_all = "…")]`, so the struct's serde config and flusso's validation
agree — then checks three things:

| Check                | Pass                                              | Compile error                                                        |
| -------------------- | ------------------------------------------------- | -------------------------------------------------------------------- |
| **field exists**     | the doc key is in the schema                       | `` no field `totl` in index `users` `` (span on the field)          |
| **type matches**     | leaf Rust type matches the field's `type`          | `` email is `keyword` → expected `String`, found `i32` ``           |
| **nullability matches** | `Option<_>` iff the field is nullable           | `` email is required → expected `String`, found `Option<String>` `` |

The rules that make this **full control rather than a straitjacket**:

- **Partial projections are allowed.** Leaving schema fields off your struct is
  fine — you only deserialize the subset you declare. Only the three checks above
  fail.
- **Type matching is by leaf identifier + `Option` shape.** The macro can't resolve
  arbitrary type aliases, so it compares the final path segment (`String`, `i32`,
  `f64`, `OffsetDateTime`, …) and the `Option<_>` wrapper against the [type
  table](#flusso-types--rust-types). For an `object` field it expects a struct,
  for a `nested` field a `Vec<_>`, and hands the inner check to *that* fragment.
- **Escape hatches.** A field typed `serde_json::Value` opts out of type checking.
  `#[flusso(skip)]` drops a field from validation entirely — for a computed or
  app-only field not backed by the index (pair it with `#[serde(skip)]` or
  `#[serde(default)]`). `#[flusso(opaque)]` keeps the field checked against the
  mapping but skips the *shape* check, for a plain un-derived struct.
- **`#[flusso(scope = "…")]`** renames the namespace generated for a container
  field, and everything under it. Only a root field can carry it — a level the
  root doesn't project has nowhere to hang an attribute, and keeps the generated
  name.

#### What a fragment adds

A fragment has no schema of its own, so its checks run where it is **embedded**.
The root bakes the resolved level into const data and hands it down; the fragment
holds one assertion per declared field, with the message baked in at macro time.

The error's primary span is the embedding, with a note chain down to the field —
so you see both which use site broke and which field is wrong, however deep:

```
error[E0080]: evaluation panicked: fragment `Geo`: field `lat` has a Rust type
              that cannot hold the schema field at this path
   |
27 |     billing_address: Address,
   |                      ^^^^^^^ evaluation of `_` failed inside this call
note: inside `Address::__flusso_check`
 7 |     geo: Geo,
note: inside `Geo::__flusso_check`
12 |     lat: f64,
```

On top of the three checks above, a fragment field also verifies **enum variants**:
a `#[derive(FlussoValue)]` enum declaring a variant the schema's `variants:` never
lists is a compile error, because it could never match a document. Covering only
*some* of the schema's variants is fine — that's a partial projection.

Two limits worth knowing, both from `panic!` in a const context taking a literal:
messages can't interpolate the schema's own type name, and there are **no
warnings** — a check either passes or fails the build.

### flusso types → Rust types

The type the derive **expects** for each schema `type` (the same bridge the
[schema guide](https://alias2k.github.io/flusso/guides/schema-authoring.html) defines, with the Rust side added). Declare something
else and it won't compile (modulo the leaf-identifier rule above).

| flusso `type`     | OpenSearch | Rust type                        | Field handle    |
| ----------------- | ---------- | -------------------------------- | --------------- |
| `text`            | `text`     | `String`                         | `Text`          |
| `identifier`      | `text`     | `String`                         | `Text`          |
| `keyword`         | `keyword`  | `String` (or a `FlussoValue` newtype) | `Keyword`  |
| `enum`            | `keyword`  | `String` or a `#[derive(FlussoValue)]` enum | `Enum` if it declares a `variants` order (order-correct sort), else `Keyword` |
| `uuid`            | `keyword`  | `String`, or `uuid::Uuid` (`uuid` feature) | `Keyword` |
| `boolean`         | `boolean`  | `bool`                           | `Bool`          |
| `short`           | `short`    | `i16`                            | `Number<kind::Short>`   |
| `integer`         | `integer`  | `i32`                            | `Number<kind::Integer>` |
| `long`            | `long`     | `i64`                            | `Number<kind::Long>`    |
| `float`           | `float`    | `f32`                            | `Number<kind::Float>`   |
| `double`          | `double`   | `f64`                            | `Number<kind::Double>`  |
| `decimal`         | `double`   | `Decimal` (`decimal` feature) or `f64` | `Number<kind::Decimal>` |
| `date`            | `date`     | `time::Date` (feature)           | `Date`          |
| `timestamp`       | `date`     | `time::OffsetDateTime` (feature) | `Date`          |
| `binary`          | `binary`   | `String` (base64)                | `Binary`        |
| `json`            | `object`   | `serde_json::Value`              | `Json`          |
| `geo_point`       | `geo_point`| `GeoPoint` (`{ lat, lon }`)      | `Geo`           |
| `custom { opensearch }` | (given) | matching scalar, else `serde_json::Value` | by OS type |
| `object`          | `object`   | a struct                         | `Object`        |
| join `belongs_to` / `has_one` | `object`   | `Option<` a struct `>`           | `Object`        |
| join `has_many` / `many_to_many` | `nested` | `Vec<` a struct `>`  | `Nested<S, T>`  |

**Decimals query without a cast.** `type: decimal` maps to OpenSearch `double`
(lossy in storage) but gets a `Number<kind::Decimal>` handle, distinct from a
true `double`'s `Number<kind::Double>` — so it accepts a `Decimal` or a
losslessly-widening integer (`eq(5)`), but *not* an `f64` (that's lossy, a
compile error). To pass a `rust_decimal::Decimal`, **enable the `decimal`
feature** on `flusso-query` (it re-exports `Decimal` and implements
`FlussoValue<kind::Decimal>` for it); the document field can be `Decimal` or
`f64` (both deserialize from the stored double). Query precision is `f64`-bound
(serde_json has no arbitrary precision) — fine for a `double`-stored column; if
you need exact querying, declare a `custom` `scaled_float` (also `kind::Decimal`)
and note this limit.

**Dates** are behind a feature so a caller picks `time` or `chrono` (or `String`
for raw ISO-8601); the derive accepts whichever leaf type the chosen feature settles
on.

**Custom value types — `#[derive(FlussoValue)]`.** A newtype **inherits its inner
type's kinds** automatically, so `struct Money(Decimal)` is a `decimal` value and
`struct Sku(String)` a keyword + text value — *no kind tag needed*, and each is
queryable and rejected exactly where the inner type would be. `FlussoValue`
requires `serde::Serialize` (a supertrait), so derive it too. An **enum** has no
inner type, so it needs an explicit string kind — `#[flusso(keyword)]` (default)
or `#[flusso(text)]`; numeric/date tags don't exist (use a newtype, which
inherits). Enum keyword fields stay typed — never `#[flusso(skip)]` them.

```rust
// Newtype: inherits Decimal's kinds — a `decimal`/`scaled_float` value.
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize, FlussoValue)]
struct Money(Decimal);

// Enum: string-valued, so a kind tag is required (keyword default).
#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[serde(rename_all = "camelCase")]
#[flusso(keyword)]
enum Tier { Pro, Enterprise, Free }

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "customers")]
struct Customer { tier: Tier, balance: Money /* … */ }

Customer::tier().eq(Tier::Pro);              // term against "pro"
Customer::balance().gte(Money(Decimal::ONE)); // a decimal value, no cast
```

**`uuid::Uuid` is a keyword value behind the `uuid` feature** — id / foreign-key
fields stay in the struct as `Uuid` (no `#[flusso(skip)]`, no
`Keyword::at("…")`), and `Customer::owner_id().eq(some_uuid)` works without
`.to_string()`.

### Nullability is declared, not guessed

A field is `T` or `Option<T>`, and the derive **checks** it against the resolved
mapping. Nullability comes straight from the schema with no database round-trip: a
leaf states it with `required`, and joins, objects, and aggregates carry it
structurally. `ResolvedField` records the resulting `nullable: bool`; the derive
requires `nullable: false → T`, `nullable: true → Option<T>`.

| Field source                          | `nullable` | Why                                                  |
| ------------------------------------- | ---------- | ---------------------------------------------------- |
| root `primary_key` column             | `false`    | forced non-null — it backs the document id           |
| join `primary_key` field              | `false`    | forced non-null, just like the root key              |
| leaf column, `required: true`         | `false`    | declared non-null                                    |
| leaf column, `required: false`        | `true`     | nullable by default                                  |
| `object`                              | `false`    | always assembled from the same row                   |
| join `belongs_to` / `has_one` (`object`)          | `true`     | there may be no related row                          |
| join `has_many` / `many_to_many`     | `false`    | a `Vec`, empty when there are none, never null       |
| aggregate `count`                     | `false`    | a non-null `long` — zero rows is `0`, not null       |
| aggregate `avg`                       | `true`     | a nullable `double` — null over zero rows            |
| aggregate `sum` / `min` / `max`       | `true`     | null over zero rows; the result mirrors the column   |

`required` is rejected by the schema on joins and aggregates precisely because
their nullability is structural — so there's nothing for the author to declare.

---

## The escape hatch

Anything the typed builder can't express stays reachable, and still deserializes
into the typed struct:

```rust
let page: SearchResponse<User> = User::query()
    .raw(serde_json::json!({
        "knn": { "embedding": { "vector": [/* … */], "k": 10 } }
    }))
    .send(&client)
    .await?;
```

`raw` takes the OpenSearch query DSL verbatim — the pressure-release valve for the
few types with no flusso field (`knn`/vector, `geo_shape`, span and parent/child
queries) and for percolators — without dropping to an untyped client or losing
typed results. Most of what used to need it (`function_score`, `script`,
`constant_score`, `query_string`, `search_after`, …) is now in the typed surface.

---

## Resolving the index name

The **physical** index carries the hash suffix (`users_3f2a1b9c`) — exactly what
the OpenSearch sink writes — and rotates on a structural schema change.

Because the binding is **generated from the schema at compile time**, the derive
knows that hash and emits it as a `const`: `User::INDEX` is the physical name, and
`get`/`query` use it. So `User::query()` addresses the right index directly, with
the hash hidden from the caller — no convenience alias required.

This is self-correcting: a structural schema change rotates the hash *and* changes
the resolved mapping, so the next `cargo build` regenerates the binding against the
new physical index. (`User::INDEX` and `User::SCHEMA_HASH` are exposed for logging,
admin, or a hand-built `Search`.)

> The convenience alias (`users` → current generation) is still worthwhile for
> clients that *don't* recompile against the schema — dynamic/scripting use,
> dashboards. For a derived binding it's unnecessary: the compile-time hash is the
> stable name.

### Reading a prefixed deployment

If flusso runs with an [index prefix](https://alias2k.github.io/flusso/guides/configuration.html) (`FLUSSO_INDEX_PREFIX`,
e.g. `dev_` so it writes `dev_users_<hash>`), tell the client the same prefix:

```rust
let client = Client::connect("https://localhost:9200")?
    .index_prefix(std::env::var("FLUSSO_INDEX_PREFIX").unwrap_or_default());
```

The prefix is applied **at runtime**, on the transport — the derive still bakes the
unprefixed `User::INDEX`/`SCHEMA_HASH`, and the client prepends the prefix to every
request path. So **one compiled consumer binary serves every environment**: point it
at dev or staging by setting `FLUSSO_INDEX_PREFIX`, no rebuild. It must match the
writer's prefix exactly, or queries hit an empty (or wrong) index.

---

## Out of scope for the first cut

- **Search aggregations** (facets, histograms, cardinality). The typed surface is
  filter/query/sort + typed hits first; aggregations need their own typed result
  tree, and the `raw` hatch covers them in the meantime.
- **Writes.** flusso owns the index; the client never upserts or deletes.
- **Correlating hits across indexes.** Both multi-index shapes ship — [one blended
  result list](#one-blended-result-list-combined-search) and independent searches
  via [`_msearch`](#several-searches-one-round-trip-_msearch) — but *correlating*
  hits across indexes remains the caller's job.
- **Scroll pagination.** `from`/`size` and `search_after` (deep pagination) ship;
  a scroll cursor is a follow-on.
- **Generating the document struct.** By design — the developer owns the struct.

---

## Where this lands in the workspace

| Crate            | Role                                                                                  |
| ---------------- | ------------------------------------------------------------------------------------- |
| `flusso-query` | Runtime: the `Client` transport, the field-handle/`Query`/`Search` builder, `SearchResponse`. Generic over the developer's document types. Targets OpenSearch / Elasticsearch (shared DSL). Re-exports the derives behind a `derive` feature, so callers `use flusso_query::{FlussoRoot, FlussoFragment}`. |
| `flusso-query-derive`  | The proc-macro crate. `#[derive(FlussoRoot)]` discovers `flusso.toml` at compile time, resolves the named index's [`IndexMapping`](https://github.com/alias2k/flusso/blob/main/libs/0-core/src/config/index_mapping.rs) from the self-describing schema (no database), validates the annotated struct, and emits the whole handle tree plus entry points and schema hash. `#[derive(FlussoFragment)]` emits a location-free shape check the root drives. Reuses `schema-config-toml`, `schema-index-yaml`, and `schema-core`. |

Both crates sit above `schema-core` and depend only downward — no dependency on the
engine, the sources, or the sinks. They share only the domain model, the one thing
the read and write sides must agree on.

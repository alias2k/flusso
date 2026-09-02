# Composing queries and options

Combine handle queries with `and`/`or`/`not` or the bool clauses, set per-query options, reach the compound and standalone query types, and build searches as plain values.

## Scopes

A handle's operator returns a `Query<S>` tagged with the **scope** it was built in. The root and every flattened object share `Root`; a nested array introduces its own scope. Queries combine only within a scope; a child query is lifted into the parent with `any`/`all`. See [Nested collections](nested.md).

## Two styles

```rust
// Combinator style.
let q = User::email().eq("ada@example.com")
    .and(User::order_count().gte(5))
    .and(User::orders().any(Orders::status().eq("delivered").and(Orders::total().gt(0))));
User::query().query(q).send(&client).await?;

// Clause style.
User::query()
    .query(User::full_name().matches("ada"))         // scored (must)
    .filter(User::order_count().gte(5))              // cached, no score
    .must_not(User::email().prefix("test-"))
    .should(User::orders().any(Orders::status().eq("delivered")))
    .send(&client)
    .await?;
```

| Clause | Scores | Bool slot | Use for |
| --- | --- | --- | --- |
| `query` | yes | `must` | relevance you want ranked |
| `filter` | no, cached | `filter` | exact constraints: terms, ranges, nested `any` |
| `must_not` | no | `must_not` | exclusions |
| `should` | yes | `should` | optional boosts; a real constraint with `min_should_match` |

`.and()`/`.or()` on a **builder** (the value an operator returns) need `use flusso_query::AsQuery;`; the clauses need no import.

## Optional filters

`Option<Q>` is itself a query where `None` contributes nothing, so a request's optional parameters `.map` straight into the chain:

```rust
User::query()
    .filter(params.email.map(|e| User::email().eq(e)))
    .filter(params.min_orders.map(|n| User::order_count().gte(n)))
    .send(&client).await?;
```

`and(None)` is the identity and `must_not(None)` excludes nothing.

## Per-query options

Every operator returns a small builder carrying its options plus the universal `boost(f32)` and `name(&str)`. With none set it renders the DSL shorthand. A builder drops into a clause directly; no `.build()`.

```rust
User::query()
    .should(User::full_name().matches("acme").boost(2.0))
    .should(User::full_name().keyword().wildcard("*acme*").case_insensitive())
    .should(User::full_name().matches("acme").fuzziness(Fuzziness::Auto))
    .min_should_match(1)
    .send(&client).await?;
```

| Query | Options |
| --- | --- |
| `term`, `prefix`, `wildcard`, `regexp` | `case_insensitive` |
| `prefix`, `wildcard` | `rewrite` |
| `regexp` | `flags`, `max_determinized_states` |
| `fuzzy` | `fuzziness`, `prefix_length`, `max_expansions`, `transpositions` |
| `matches` | `fuzziness`, `operator`, `minimum_should_match`, `prefix_length`, `analyzer`, `zero_terms_query`, `lenient` |
| phrase matches | `slop`, `analyzer` |
| `multi_match` | `type`, `operator`, `fuzziness`, `tie_breaker`, `minimum_should_match` |
| range | `format`, `time_zone`, `relation` |
| geo `within` | `distance_type`, `validation_method` |
| nested `any` | `score_mode`, `ignore_unmapped` |

Enumerable options are **closed enums**, so a typo is a compile error: `Operator { And, Or }`, `Fuzziness { Auto, AutoBounds(u32, u32), Edits(u32) }`, `MultiMatchType`, `ZeroTermsQuery`, `RangeRelation`, `ScoreMode`/`BoostMode`, `NestedScoreMode` (with `None` for filter-only), `DistanceType`, `ValidationMethod`, `NumericType`, `ScriptSortType`. `MinimumShouldMatch` takes a count, `::percent(75)`, or `::raw("3<90%")`. Open-ended params (`analyzer`, `format`, `time_zone`, `unmapped_type`, `flags`) stay `String`.

## Compound, scoring, and standalone

- **Bool.** `Search::min_should_match(n)` (or `Query::min_should_match` on an `or` group) turns a `should` group into a constraint.
- **Scoring wrappers**, free functions: `constant_score(filter)`, `dis_max([..]).tie_breaker(..)`, `boosting(positive, negative, negative_boost)`, `function_score(query).weight(..).weight_when(.., filter).boost_mode(..)`.
- **Standalone**, each an `AsQuery`: `ids([..])`, `query_string(..)`, `simple_query_string(..)`, `combined_fields(.., [fields])`, `script(..)`, `script_score(query, source)`, `distance_feature(..)`, `rank_feature(..)`, `more_like_this([fields], [like])`.
- **Search-level** on `Search`: `min_score`, `track_total_hits`, `track_scores`, `search_after([..])`, `collapse(field)`, `post_filter(q)`, `highlight(Highlight::new().field(..))`, `from`, `size`.

What's left for [`raw`](results-and-escape-hatch.md#the-escape-hatch): `knn`, `geo_shape`, span, and parent/child queries, types with no flusso field.

## Queries are values

`Type::query()` takes no client. A `Search<T>` is a plain `Clone` value with no lifetime: build it in a helper, store it, hand a `&Client` to the terminal (`send`, `ids`, `count`) when it's time.

```rust
fn busy_users() -> Search<User> {
    User::query().filter(User::order_count().gte(5))
}
let page = busy_users().send(&client).await?;
let next = busy_users().from(20).send(&client).await?;
```

## Related

- [Results and the escape hatch](results-and-escape-hatch.md) for `send`, `ids`, `count`, and `raw`.

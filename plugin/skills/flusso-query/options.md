# Query options, compound queries, and SortBuilder

The long tail of the typed surface. Reach for this file when a plain operator isn't enough: an
option beyond the defaults, a compound/scoring query, a standalone query type, or mapping a request
onto the `sort` array.

Everything here is typed. If you are about to write `raw(json!(…))` for something on this page,
that is the anti-pattern the `flusso-query` skill lists as #5.

## Builders and the universal options

Each leaf operator returns a small **builder** carrying that query's options plus the universal
`boost(f32)` and `name(&str)` (`_name`, surfaced in `matched_queries`). With no option set it
renders the DSL shorthand; set one and it expands. A builder *is* an `AsQuery`, so it drops straight
into a clause with no `.build()`:

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

> `.or()` / `.and()` / `.not()` on a **builder** need `use flusso_query::AsQuery;` (provided trait
> methods). Inherent `Query` methods are unaffected, and composing through the `Search` clauses
> needs no import.

## Per-type options

All optional:

| Query | Options |
| --- | --- |
| `term` / `prefix` / `wildcard` / `regexp` | `case_insensitive` |
| `prefix` / `wildcard` | `rewrite` |
| `regexp` | `flags`, `max_determinized_states` |
| `fuzzy` | `fuzziness`, `prefix_length`, `max_expansions`, `transpositions` |
| `matches` | `fuzziness`, `operator`, `minimum_should_match`, `prefix_length`, `analyzer`, `zero_terms_query`, `lenient` |
| phrase | `slop`, `analyzer` |
| `multi_match` | `type`, `operator`, `fuzziness`, `tie_breaker`, `minimum_should_match` |
| range | `format`, `time_zone`, `relation` |
| geo `within` | `distance_type`, `validation_method` |
| nested `any` | `score_mode`, `ignore_unmapped` |

## The enumerable params are closed enums

A typo is a compile error, not a 400: `Operator { And, Or }`; `Fuzziness { Auto, AutoBounds(u32,u32),
Edits(u32) }`; `MultiMatchType`; `ZeroTermsQuery { None, All }`; `RangeRelation { Intersects,
Contains, Within }`; `ScoreMode` / `BoostMode` (function_score); `NestedScoreMode` (has `None` for a
filter-only clause); `DistanceType` / `ValidationMethod`; `NumericType` / `ScriptSortType` (sort);
`MinimumShouldMatch` (`2` or `.into()` for a count, `::percent(75)`, `::raw("3<90%")`).

Open-ended params stay `String`: `analyzer`, `format`, `time_zone`, `unmapped_type`, `flags`.

## Bool and scoring

`Search::min_should_match(n)` (or `Query::min_should_match` on an `or`-group, plus `Query::boost`)
turns a top-level free-text `should` group into a real constraint.

Free functions: `constant_score(filter)`, `dis_max([..]).tie_breaker(..)`,
`boosting(pos, neg, negative_boost)`,
`function_score(q).weight(..)` / `.weight_when(.., filter)` / `.boost_mode(..)`.

## Standalone queries

Free functions, each an `AsQuery`: `ids([..])`, `query_string(..)`, `simple_query_string(..)`,
`combined_fields(.., [fields])`, `script(..)`, `script_score(q, src)`, `distance_feature(..)`,
`rank_feature(..)`, `more_like_this([fields], [like])`.

(`match_bool_prefix` is a `Text` operator, not a free function.)

## Search-level controls

`min_score`, `track_total_hits`, `track_scores`, `search_after([..])` (deep pagination),
`collapse(field)`, `post_filter(q)`, `highlight(Highlight::new().field(..).pre_tags(..))`.

## SortBuilder

Maps a request onto the `sort` array, one verb per concern, each absorbing its own optionality. The
sorting *rules* (which handles sort, how nesting and maps behave) are in the skill's `## Sorting`
section; this is the builder's surface.

| Verb | Does |
| --- | --- |
| `.by(handle, dir)` | `dir` is a `SortOrder`, an `OrderBy`, or an `Option` of either. A `None` skips the field, so a request's `Option<dir>` flows straight in. |
| `.near(geo, center)` | geo distance; skips on `None` |
| `.score()` / `.score_if(cond)` | relevance |
| `.tiebreak(handle)` | stable final key |
| `.or_default(sort)` | fallback when the builder would otherwise be empty |
| `.raw(sort)` | escape hatch, exempt from dedup |
| `.build()` / `IntoIterator` | hand to `Search::sorts(..)`, also `MultiSearch` / `NestedProjection` |

`by` / `near` / `tiebreak` / `or_default` dedup by sort key, first wins. Convert your own direction
enum once with `impl From<MyDir> for OrderBy`. `OrderBy::asc()` / `::desc()` carry the same
modifiers as a field sort (`missing_*`, `mode`, `numeric_type`, `unmapped_type`, `format`).

For a single key outside a builder, `Sort::score()` and `Sort::script(type, src, order)` exist;
inside a builder use `.raw(..)` for those.

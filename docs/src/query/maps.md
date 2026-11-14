# Maps

A schema `map` field has runtime keys and one declared value type. Each key is mapped like a plain field of that type, so a map handle offers the plain handle twice: on one key, and across every key at once.

## Handles

`handle_fn` picks the handle from the map's `values`: `TextMap` for `text`/`identifier`, `KeywordMap` for `keyword`/`enum`/`uuid`, `DateMap` for dates, `NumberMap` for the numerics.

```rust
Product::name().key("it").matches("scarpe")            // one key: a Text leaf
Product::name().matches("scarpe")                      // every key
Product::prices().between(10, 50)                      // a price in any currency
Product::name().has_key("it")                          // presence of one key
Product::name().exists()                               // any key
```

## One key

`.key(..)` takes a `&str` and returns the handle a plain field of the value kind gets: `Text`, `Keyword`, `Number<K>`, `Date`. A `TextMap`/`KeywordMap` carries the field's subfield marker (`WithSubfields` when every OpenSearch sink has `auto_subfields` on), so a key has the subfield accessors exactly when a plain field would:

```rust
Product::name().key("en").matches("cafe")                         // finds "Café"
Product::name().key("en").keyword_lowercase().wildcard("*excav*") // part of a word
Product::labels().key("color").eq("Dark Red")                     // exact term on a keyword
```

## Every key

The map handle mirrors the plain handle of its value kind: same operator names, same arguments, applied across all keys. A document matches when **any** key matches. For every operator except the analyzed family, that is exactly the documents an `or` of the operator on each key returns. The analyzed family (`matches`, the phrase and prefix forms, `matches_fuzzy`) also reaches each key's subfields by design, so it can find a little more; see [How it renders](#how-it-renders).

| Handle | Across every key |
| --- | --- |
| `TextMap` | `matches`, `match_phrase`, `match_phrase_prefix`, `match_bool_prefix`, `matches_fuzzy`; with subfields `any_of` (exact, via `.keyword`) |
| `KeywordMap` | `eq`, `any_of`, `prefix`, `wildcard`, `regexp`, `fuzzy` (with subfields, on each key's exact `.keyword`) |
| `NumberMap<K>`, `DateMap` | `eq`, `any_of`, `lt`, `lte`, `gt`, `gte`, `between` |

Each takes two map-only options:

```rust
Product::name().matches("scarpe").prefer("it", 3.0).prefer("en", 1.0)
Product::name().match_phrase("scarpe rosse").prefer("it", 1.0).only_preferred()
```

- **`prefer(key, weight)`** weights one key (`name.it^3`). Add several to rank keys, the user's locale first. In a filter (a range, an exact value) it changes nothing; it only matters where the clause scores.
- **`only_preferred()`** drops the all-keys fallback, so only the preferred keys are queried. With no `prefer`, nothing is left to query and the clause matches nothing (it renders `match_none`).

The builders keep their plain counterpart's options where the underlying query has them: `operator`, `fuzziness`, `slop`, `minimum_should_match` on the analyzed family (plus `tie_breaker` and `match_type`), `time_zone` on ranges, `rewrite` on `prefix`/`wildcard`, `fuzziness`/`prefix_length`/`max_expansions`/`transpositions` on `fuzzy`. `case_insensitive` and the regex `flags` have no cross-key form; use a subfield view or `.key(..)`.

### How it renders

Keys aren't known when the query is built, so each operator renders as a query that takes a field pattern:

- **The analyzed family** is a `multi_match` over `name.*` (`best_fields`, `phrase`, `phrase_prefix`, `bool_prefix`), so the best-scoring key wins. The pattern also reaches each key's subfields, so a query equal to a whole value (`matches("Zebra red")`) gets an extra exact-match hit from `.keyword`/`.keyword_lowercase` and scores higher. That boost is intended. A phrase prefix renders `lenient: true`, because OpenSearch refuses a phrase prefix on a keyword subfield; those subfields are skipped.
- **Every other operator** is a `query_string` over one explicit pattern (for a `KeywordMap` with subfields, `labels.*.keyword`; see [below](#subfield-views)), the value escaped so it's matched literally: `"a" OR "b"` for `eq`/`any_of`, `["10" TO "50"]` (or `{"3" TO *]`) for ranges, `v*` for `prefix`, `/re/` for `regexp`, `v~` for `fuzzy`. Date bounds take date math (`"now-1d"`). A value that starts with `AND`, `OR` or `NOT` gets its first letter escaped (`\AND~`), so it stays a value, not an operator. In a `wildcard` pattern, `\` escapes the next character as on one key, and a trailing lone `\` is a literal backslash. An empty `eq`/`fuzzy` value matches the empty value, as on one key.

Edge cases worth knowing:

- **`match_phrase_prefix` runs leniently.** The keys' keyword subfields are skipped rather than failing the query, so a phrase prefix gets no exact-value boost.
- **`fuzziness` doesn't combine with the phrase operators.** The analyzed builders share one type, as on a plain `Text`, so `match_phrase(..).fuzziness(..)` compiles, but OpenSearch rejects the request. Use `fuzziness` with `matches` or `match_bool_prefix` only.
- **A leading wildcard needs it allowed.** A cluster with `indices.query.query_string.allowLeadingWildcard=false` refuses a cross-key pattern starting with `*` or `?`: a `wildcard("*x")`, and every `contains`.

A pattern that matches no field (a map with no keys indexed yet) matches nothing; it isn't an error. Inside a `nested` array the cross-key query lifts through `Nested::any`/`all` like any other.

### Subfield views

With subfields, a string map also mirrors the plain handle's subfield accessors. Each returns a map handle over that subfield of every key:

```rust
Product::name().keyword().eq("Red Shoes")          // TextMap → exact, any key
Product::labels().keyword_lowercase().eq("sale")   // KeywordMap → case-insensitive, any key
Product::labels().text().matches("sale")           // KeywordMap → analyzed, any key
```

**Why a `KeywordMap` with subfields runs its exact operators on `.keyword`.** `labels.*` matches every key *and* every key's `.text` and `.keyword_lowercase`, and a field pattern can't leave them out: `eq("sale")` over it would also match "big sale now" and "Sale". So the sink gives every key of a keyword map an exact `.keyword` subfield (see [Objects and maps](../reference/objects-and-maps.md#map)), and `eq`, `any_of`, `prefix`, `wildcard`, `regexp` and `fuzzy` render over `labels.*.keyword`. `Product::labels().keyword()` is that view. Without subfields, `labels.*` is exactly the keys, so the operators render there.

## Part of a word, across keys

```rust
Product::query().query(Product::name().contains("excav"))   // "Excavator", in any key
```

`contains` (on `TextMap` and `KeywordMap`, with subfields) is a case- and accent-insensitive substring match over every key's `.keyword_lowercase`: a `query_string` of `*excav*` on `name.*.keyword_lowercase`, the value escaped so it's matched literally. `prefer` and `only_preferred` work as above. An empty or whitespace-only value matches nothing (it renders `match_none`), like a full-text query with no terms. A leading wildcard scans the terms, so it's slower than `matches` on a large index; a value over 256 characters isn't indexed in the subfield and never matches.

## Document types

A map field deserializes into `HashMap<String, V>` or `BTreeMap<String, V>`, with `V` checked against the declared value kind like a scalar. `V` may be `Option<V>`, and a map over a `jsonb` column that can hold `{"x": null}` needs it: see [What the derive trusts](document-structs.md#what-the-derive-trusts). A type of your own stands in for the whole map with `#[derive(FlussoMap)]` and a **required** value-kind tag, `#[flusso(keyword)]` or `#[flusso(text)]`; untagged is a compile error. Any shape that reads one JSON object works: a newtype over a `HashMap`/`BTreeMap` (or over another map type, or `serde_json::Value`), or named fields whose on-disk form is a flat object of same-kind values. A shape that certainly reads something else is a compile error where it is declared: a unit struct, a tuple of several fields, or a newtype over a `Vec`, an `Option`, a single value, an array newtype or a `FlussoFragment` struct (whose fields serde requires: declare the ones you need on the map type, beside a `#[serde(flatten)]` map). So is `#[serde(from)]`/`try_from` on it, since serde then reads another type. The tag goes on the type, once, and everything the type holds is checked against it:

- a newtype's map: `V` in `HashMap<String, V>` (or `Option<V>`) is a value of the tag (`i64` in a `keyword` map is a compile error), and another wrapped type is a map of the tag, so a type the derive can't see into (`Box<…>`, a plain struct, a `type` alias of a map) is a compile error;
- a named field: a map may lack any key, so it is `Option<V>`, `V` a value of the tag (which also reads a `null` value); `#[serde(deny_unknown_fields)]` and a field's `#[serde(alias)]` are compile errors, since a map holds any key, and may hold both a key and its alias;
- a `#[serde(flatten)]` field: a map of the tag, as a newtype's.

Every map's keys are `String`. The fields take no `#[flusso(…)]`. What the derive takes on trust is [one list](document-structs.md#what-the-derive-trusts).

```rust
#[derive(serde::Deserialize, FlussoMap)]
#[flusso(text)]
struct Translation {
    fallback: Option<String>,
    #[serde(flatten)]
    langs: BTreeMap<String, Option<String>>,   // a jsonb column may hold {"it": null}
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Localized { name: Translation }
```

Use the derive rather than a hand-written `impl FlussoMap<K>`: a fragment validates through const metadata the derive emits, so a hand impl alone compiles at a root but fails inside a `FlussoFragment`, with the error pointing at the derive. Writing that metadata by hand works too: `FlussoValueMeta` with `KINDS = [Object]` and `MAP_VALUES` set to the value kind, plus [`embeddable!`](enums-and-values.md#hand-written-values). The value kind is checked symmetrically: a `text` wrapper at a `keyword` map fails the build. Number and date maps have no derive tag; use `HashMap`/`BTreeMap` for those.

## Sorting by key

`Product::name().sort_key("it").or("en")` reads as "sort by `it`, else `en`". It returns a `MapKeySort` that implements `Sortable`, so it flows through `SortBuilder::by` like any field sort. It renders a `_script` sort whose painless source walks the keys in order and sorts by the first one a document has: true fallback, not lexicographic tiers. String maps sort case- and accent-insensitively on each key's `.keyword_lowercase`, like other string sorts, so they need the subfields: `TextMap::sort_key` exists only with them, and a `KeywordMap` without them sorts on the bare key, case-insensitively. Number and date maps sort on the bare key. It's nesting-aware. `missing_first`/`missing_last`/`missing(v)` redirect into the script's parameters with a direction-correct sentinel.

**A single key sorts like a plain field.** `Product::name().key("it").asc()` sorts on `name.it.keyword_lowercase`, exactly as a plain `text` field does; without subfields a text key, like a plain text field, has no `.asc()`. Keyword, number, and date keys sort on their own path. Use `sort_key` when you want the fallback.

## Upgrading from 0.16

- `TextMap::search` and its `MapSearch` builder are gone; the mirrored `matches` replaces it with the same `prefer`/`only_preferred` and the `multi_match` options. `search(q)` becomes `matches(q)`, `search(q).match_type(MultiMatchType::BoolPrefix)` becomes `match_bool_prefix(q)`, and a `PhrasePrefix` type becomes `match_phrase_prefix(q)`.
- `MapKey` is gone: a key is a plain `Text`/`Keyword`. Code that named `Text<S, MapKey>` names `Text<S>` (or `Text<S, NoSubfields>`).
- `TextMap`/`KeywordMap` take a second type parameter, the subfield marker, defaulting to `WithSubfields`.
- A `decimal` map's keys take the decimal kind, like a plain `decimal` field: the handle is `NumberMap<kind::Decimal>` (was `kind::Double`) and the document type `HashMap<String, Decimal>`. A float no longer compiles: `Product::costs().key("eur").eq(9.99)` becomes `.eq(Decimal::new(999, 2))` (or an integer).
- A `KeywordMap` with subfields has the exact operators across keys. Code that worked around their absence with `.keyword_lowercase().eq(..)` (case-insensitive) or an `or` of `.key(k).eq(..)` can call `eq(..)` directly.
- String map sorts read `.keyword_lowercase`, and the exact operators on a `KeywordMap` with subfields read each key's `.keyword`; both exist only in the index 0.17 rebuilds. Deploy a rebuilt consumer once that index is seeded; the order is in [Upgrade to 0.17](../operate/upgrade-to-0-17.md).

## Related

- [Objects and maps](../reference/objects-and-maps.md#map) for the schema side.
- [Sorting](sorting.md).
- [Upgrade to 0.17](../operate/upgrade-to-0-17.md) for the index rebuild and the consumer rollout.

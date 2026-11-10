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

The map handle mirrors the plain handle of its value kind: same operator names, same arguments, applied across all keys. A document matches when **any** key matches, exactly as an `or` of the operator on each key would.

| Handle | Across every key |
| --- | --- |
| `TextMap` | `matches`, `match_phrase`, `match_phrase_prefix`, `match_bool_prefix`, `matches_fuzzy`; with subfields `any_of` (exact, via `.keyword`) |
| `KeywordMap` without subfields | `eq`, `any_of`, `prefix`, `wildcard`, `regexp`, `fuzzy` |
| `KeywordMap` with subfields | none of the exact operators: reach the keys through a subfield view (below) |
| `NumberMap<K>`, `DateMap` | `eq`, `any_of`, `lt`, `lte`, `gt`, `gte`, `between` |

Each takes two map-only options:

```rust
Product::name().matches("scarpe").prefer("it", 3.0).prefer("en", 1.0)
Product::name().match_phrase("scarpe rosse").prefer("it", 1.0).only_preferred()
```

- **`prefer(key, weight)`** weights one key (`name.it^3`). Add several to rank keys, the user's locale first. In a filter (a range, an exact value) it changes nothing; it only matters where the clause scores.
- **`only_preferred()`** drops the all-keys fallback, so only the preferred keys are queried.

The builders keep their plain counterpart's options where the underlying query has them: `operator`, `fuzziness`, `slop`, `minimum_should_match` on the analyzed family (plus `tie_breaker` and `match_type`), `time_zone` on ranges, `rewrite` on `prefix`/`wildcard`, `fuzziness`/`prefix_length`/`max_expansions`/`transpositions` on `fuzzy`. `case_insensitive` and the regex `flags` have no cross-key form; use a subfield view or `.key(..)`.

### How it renders

Keys aren't known when the query is built, so each operator renders as a query that takes a field pattern:

- **The analyzed family** is a `multi_match` over `name.*` (`best_fields`, `phrase`, `phrase_prefix`, `bool_prefix`), so the best-scoring key wins. The pattern also reaches each key's subfields, so a query equal to a whole value (`matches("Zebra red")`) gets an extra exact-match hit from `.keyword`/`.keyword_lowercase` and scores higher. That boost is intended. A phrase prefix renders `lenient: true`, because OpenSearch refuses a phrase prefix on a keyword subfield; those subfields are skipped.
- **Every other operator** is a `query_string` over one explicit pattern, the value escaped so it's matched literally: `"a" OR "b"` for `eq`/`any_of`, `["10" TO "50"]` (or `{"3" TO *]`) for ranges, `v*` for `prefix`, `/re/` for `regexp`, `v~` for `fuzzy`. Date bounds take date math (`"now-1d"`).

A pattern that matches no field (a map with no keys indexed yet) matches nothing; it isn't an error. Inside a `nested` array the cross-key query lifts through `Nested::any`/`all` like any other.

### Subfield views

With subfields, a string map also mirrors the plain handle's subfield accessors. Each returns a map handle over that subfield of every key:

```rust
Product::name().keyword().eq("Red Shoes")          // TextMap → exact, any key
Product::labels().keyword_lowercase().eq("sale")   // KeywordMap → case-insensitive, any key
Product::labels().text().matches("sale")           // KeywordMap → analyzed, any key
```

**Why a `KeywordMap` with subfields has no cross-key `eq`.** `labels.*` matches every key *and* every key's `.text` and `.keyword_lowercase`, and a field pattern can't leave them out. `eq("sale")` over it would also match "big sale now" and "Sale". Rather than return wrong hits, the exact operators (`eq`, `any_of`, `prefix`, `wildcard`, `regexp`, `fuzzy`) don't exist there: calling one is a compile error. Use `keyword_lowercase().eq(..)` for case-insensitive equality across keys, or `.key(k).eq(..)` for an exact match on one key. Without subfields, `labels.*` is exactly the keys, so the operators are there.

## Part of a word, across keys

```rust
Product::query().query(Product::name().contains("excav"))   // "Excavator", in any key
```

`contains` (on `TextMap` and `KeywordMap`, with subfields) is a case- and accent-insensitive substring match over every key's `.keyword_lowercase`: a `query_string` of `*excav*` on `name.*.keyword_lowercase`, the value escaped so it's matched literally. `prefer` and `only_preferred` work as above. An empty or whitespace-only value matches nothing (it renders `match_none`), like a full-text query with no terms. A leading wildcard scans the terms, so it's slower than `matches` on a large index; a value over 256 characters isn't indexed in the subfield and never matches.

## Document types

A map field deserializes into `HashMap<String, V>` or `BTreeMap<String, V>`, with `V` checked against the declared value kind like a scalar. A type of your own stands in for the whole map with `#[derive(FlussoMap)]` and a **required** value-kind tag, `#[flusso(keyword)]` or `#[flusso(text)]`; untagged is a compile error. Any shape works: a newtype, or named fields whose on-disk form is a flat object of same-kind values.

```rust
#[derive(serde::Deserialize, FlussoMap)]
#[flusso(text)]
struct Translation {
    fallback: Option<String>,
    #[serde(flatten)]
    langs: BTreeMap<String, String>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Localized { name: Translation }
```

Use the derive rather than a hand-written `impl FlussoMap<K>`: a fragment validates through const metadata only the derive emits, so a hand impl compiles at a root but fails inside a `FlussoFragment`, with the error pointing at the derive. The value kind is checked symmetrically: a `text` wrapper at a `keyword` map fails the build. Number and date maps have no derive tag; use `HashMap`/`BTreeMap` for those.

## Sorting by key

`Product::name().sort_key("it").or("en")` reads as "sort by `it`, else `en`". It returns a `MapKeySort` that implements `Sortable`, so it flows through `SortBuilder::by` like any field sort. It renders a `_script` sort whose painless source walks the keys in order and sorts by the first one a document has: true fallback, not lexicographic tiers. String maps sort case- and accent-insensitively on each key's `.keyword_lowercase`, like other string sorts, so they need the subfields: `TextMap::sort_key` exists only with them, and a `KeywordMap` without them sorts on the bare key, case-insensitively. Number and date maps sort on the bare key. It's nesting-aware. `missing_first`/`missing_last`/`missing(v)` redirect into the script's parameters with a direction-correct sentinel.

**A single key sorts like a plain field.** `Product::name().key("it").asc()` sorts on `name.it.keyword_lowercase`, exactly as a plain `text` field does; without subfields a text key, like a plain text field, has no `.asc()`. Keyword, number, and date keys sort on their own path. Use `sort_key` when you want the fallback.

## Upgrading from 0.16

- `TextMap::search` and its `MapSearch` builder are gone; the mirrored `matches` replaces it with the same `prefer`/`only_preferred` and the `multi_match` options. `search(q)` becomes `matches(q)`, `search(q).match_type(MultiMatchType::BoolPrefix)` becomes `match_bool_prefix(q)`, and a `PhrasePrefix` type becomes `match_phrase_prefix(q)`.
- `MapKey` is gone: a key is a plain `Text`/`Keyword`. Code that named `Text<S, MapKey>` names `Text<S>` (or `Text<S, NoSubfields>`).
- `TextMap`/`KeywordMap` take a second type parameter, the subfield marker, defaulting to `WithSubfields`.
- A `decimal` map's keys take the decimal kind, like a plain `decimal` field: the handle is `NumberMap<kind::Decimal>` (was `kind::Double`) and the document type `HashMap<String, Decimal>`. A float no longer compiles: `Product::costs().key("eur").eq(9.99)` becomes `.eq(Decimal::new(999, 2))` (or an integer).
- String map sorts read `.keyword_lowercase`, which exists only in the index 0.17 rebuilds. Deploy a rebuilt consumer once that index is seeded; the order is in [Upgrade to 0.17](../operate/upgrade-to-0-17.md).

## Related

- [Objects and maps](../reference/objects-and-maps.md#map) for the schema side.
- [Sorting](sorting.md).
- [Upgrade to 0.17](../operate/upgrade-to-0-17.md) for the index rebuild and the consumer rollout.

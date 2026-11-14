# Dynamic-key `map` fields

Reach for this file when the schema has a `map:` field. A `map` is a `jsonb`-backed object whose
**keys are runtime** but whose values all share **one leaf kind**: translations
(`name: {"it": "ciao", "en": "hi"}`), per-currency prices, per-region dates.

Schema side is `- map: name` plus a required `values:` leaf type (`text` / `keyword` / a number /
`date`). The sink maps **every key like a plain field of the `values` type** (same analyzer, same
subfields). The derive gives it one handle per value kind: `TextMap`, `KeywordMap`, `NumberMap<K>`,
`DateMap`. Runtime keys, compile-time value type. That split is the whole point.

## Doc side

The field type is `HashMap<String, V>` (or `BTreeMap`) where `V` is a value of the declared kind, or
`Option<V>`: `HashMap<String, Option<String>>` for a text or keyword map, `HashMap<String,
Option<f64>>` for a `double` map. A map over a `jsonb` column is indexed as it is, `{"x": null}`
included, and only `Option` values read that null. A nullable map is `Option<HashMap<…>>`.

```rust
#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "products")]
struct Product {
    sku: String,
    title: HashMap<String, Option<String>>,          // map<text>    → TextMap
    codes: Option<HashMap<String, Option<String>>>,  // map<keyword> → KeywordMap (nullable)
    prices: Option<HashMap<String, Option<f64>>>,    // map<double>  → NumberMap
}
```

A whole-map wrapper opts in with `#[derive(FlussoMap)]` and a **required** kind tag
(`#[flusso(text)]` or `#[flusso(keyword)]`, matching the schema's `values:`). There is no default;
an untagged wrapper is a compile error. The wrapper takes any shape that reads one JSON object, newtype
over a map or named fields, so a translations type can carry a `fallback` field beside its language
keys; a newtype over a `Vec`, an `Option` or a single value is a compile error. What it holds is
checked against the tag: a newtype's `HashMap<String, V>` needs `V` of that kind, and a named field
is `Option<V>` (a map may lack any key); a map's keys are `String`. What the derive checks and what it trusts (a flattened map,
a `json` field, …) is one list in the manual:
<https://alias2k.github.io/flusso/main/query/document-structs.html#what-the-derive-trusts>.

A hand-written `impl FlussoMap<K>` works at a root but fails inside a `FlussoFragment` unless the
type also implements `FlussoValueMeta` (`KINDS = [Object]`, `MAP_VALUES` = the value kind) and
calls `flusso_query::embeddable!`. Use the derive; the E0277 note points at it.

## Query side

**One key** gives the plain-field handle of the value kind, with everything a plain field of
that kind has: subfield accessors (`TextMap`/`KeywordMap` carry the same
`WithSubfields`/`NoSubfields` marker as a plain `text`/`keyword` field), `wildcard`, `prefix`,
phrase variants, and `.asc()`/`.desc()` where a plain field of the kind sorts.

```rust
Product::title().key("it").matches("ciao")                      // text, accent-folded
Product::title().key("it").keyword_lowercase().prefix("cia")    // part of a word, one key
Product::codes().key("ean").eq("0049")                          // keyword exact term
Product::prices().key("usd").gte(9.99)                          // number range
```

**Presence** is `Product::title().has_key("it")` (that key non-null) or `Product::title().exists()`
(any key).

**Every key**: the map handle has the operators of the plain handle of its value kind, same
names and arguments, matching a document when any key matches. Outside the analyzed family that
is the same hits as an `or` of the operator on each key; the analyzed family also reaches the
keys' subfields (below), so it can find a little more:

```rust
Product::title().matches("ciao").prefer("it", 3.0).prefer("en", 2.0)  // TextMap, multi_match
Product::title().match_phrase("ciao mondo").prefer("it", 1.0).only_preferred()
Product::prices().between(10, 50)                                     // NumberMap, any currency
Product::codes().eq("Sale")                                           // exact, any key
Product::codes().keyword_lowercase().eq("sale")                       // case-insensitive, any key
```

| Handle | Cross-key operators |
| --- | --- |
| `TextMap` | `matches` `match_phrase` `match_phrase_prefix` `match_bool_prefix` `matches_fuzzy`; with subfields `any_of`, views `keyword()` / `keyword_lowercase()` |
| `KeywordMap` | `eq` `any_of` `prefix` `wildcard` `regexp` `fuzzy`; with subfields also views `keyword()` / `text()` / `keyword_lowercase()` |
| `NumberMap<K>` / `DateMap` | `eq` `any_of` `lt` `lte` `gt` `gte` `between` |

- `prefer(key, weight)` weights a key; `only_preferred()` drops the `path.*` fallback. Both exist on
  every cross-key query; on a filter (`between`, `eq`) `prefer` is accepted but scores nothing.
  `only_preferred()` with no `prefer` matches nothing (`match_none`).
- The analyzed family is a `multi_match` over `path.*`, which also reaches each key's subfields:
  an exact whole-value hit scores higher, by design. Everything else is a `query_string` over one
  pattern, value escaped (a leading `AND` / `OR` / `NOT` too; a `wildcard` keeps `\x` escapes).
- `match_phrase_prefix` runs `lenient`, skipping the keys' keyword subfields (no exact-value
  boost). `fuzziness` on a phrase operator compiles but OpenSearch rejects it. A cluster with
  `indices.query.query_string.allowLeadingWildcard=false` refuses a cross-key pattern starting with
  `*` / `?` (every `contains`).
- **On a `KeywordMap` with subfields the exact family runs over `codes.*.keyword`**, an exact
  subfield the sink gives every key of a keyword map: `codes.*` would also hit each key's `.text`
  and `.keyword_lowercase` (`eq("sale")` matching `"big sale"`). `codes().keyword()` is that view.
  Without subfields it runs over `codes.*`, which is exactly the keys.
- Options a cross-key query type can't carry (`case_insensitive`, regex `flags`, range `format` /
  `relation`) are absent; use `.key(k)`.

**Upgrading from `search`**: `TextMap::search` / `MapSearch` were removed in `flusso-query` 0.17.
`search(q)` → `matches(q)`; `search(q).match_type(MultiMatchType::BoolPrefix)` →
`match_bool_prefix(q)`; `PhrasePrefix` → `match_phrase_prefix(q)`. `prefer` / `only_preferred` and
the `multi_match` options carry over.

**Part of a word, any key**, `TextMap` and `KeywordMap` with subfields:
`Product::title().contains("excav")` is a case- and accent-insensitive substring match (a
`query_string` of the escaped `*excav*` over `title.*.keyword_lowercase`); `prefer` /
`only_preferred` as above.

**Sort by key with fallback** is `Product::title().sort_key("it").or("en")`. The rules live in the
skill's `## Sorting` section. Two things to keep straight:

- `key("it").asc()` sorts that one key like a plain field; `sort_key` is for fallback.
- `prefer(key, weight)` is search *scoring* (blend across keys). `sort_key("it").or("en")` is
  ordered *fallback* (take the first key present). Don't reach for weights when you mean fallback.

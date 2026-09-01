# Dynamic-key `map` fields

Reach for this file when the schema has a `map:` field. A `map` is a `jsonb`-backed object whose
**keys are runtime** but whose values all share **one leaf kind**: translations
(`name: {"it": "ciao", "en": "hi"}`), per-currency prices, per-region dates.

Schema side is `- map: name` plus a required `values:` leaf type (`text` / `keyword` / a number /
`date`). The derive gives it one handle per value kind: `TextMap`, `KeywordMap`, `NumberMap<K>`,
`DateMap`. Runtime keys, compile-time value type. That split is the whole point.

## Doc side

The field type is `HashMap<String, V>` (or `BTreeMap`) where `V` is a value of the declared kind, by
blanket impl and no derive: `HashMap<String, String>` for a text or keyword map,
`HashMap<String, f64>` for a `double` map. A nullable map is `Option<HashMap<…>>`.

```rust
#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "products")]
struct Product {
    sku: String,
    title: HashMap<String, String>,          // map<text>    → TextMap
    codes: Option<HashMap<String, String>>,  // map<keyword> → KeywordMap (nullable)
    prices: Option<HashMap<String, f64>>,    // map<double>  → NumberMap
}
```

A whole-map wrapper opts in with `#[derive(FlussoMap)]` and a **required** kind tag
(`#[flusso(text)]` or `#[flusso(keyword)]`, matching the schema's `values:`). There is no default;
an untagged wrapper is a compile error. The wrapper takes any shape, newtype or named fields, so a
translations type can carry a `fallback` field beside its language keys.

A hand-written `impl FlussoMap<K>` works at a root but fails inside a `FlussoFragment`. Use the
derive; the E0277 note points at it.

## Query side

**One key** gives a fully-typed leaf of the value kind, queried like any field of that kind:

```rust
Product::title().key("it").matches("ciao")     // text
Product::codes().key("ean").eq("0049")         // keyword exact
Product::prices().key("usd").gte(9.99)         // number range
```

**Presence** is `Product::title().has_key("it")` (that key non-null) or `Product::title().exists()`
(any key).

**Cross-key full-text**, `TextMap` only:

```rust
Product::title().search("ciao").prefer("it", 3.0).prefer("en", 2.0)
```

One `multi_match best_fields` over the preferred keys (each `key^weight`) plus a `title.*` fallback,
so the best-scoring key wins. `.only_preferred()` drops the fallback. `operator` / `fuzziness` /
`minimum_should_match` behave as on `matches`. Exact-match maps have no `search`; use
`key(..).eq(..)`.

**Sort by key with fallback** is `Product::title().sort_key("it").or("en")`. The rules live in the
skill's `## Sorting` section. Two things to keep straight:

- `.key("it")` is **not** sortable. A bare `key(..).asc()` would target a subfield that doesn't
  exist, so it doesn't compile.
- `prefer(key, weight)` is search *scoring* (blend across keys). `sort_key("it").or("en")` is
  ordered *fallback* (take the first key present). Don't reach for weights when you mean fallback.

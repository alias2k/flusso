# Maps

A schema `map` field has runtime keys and one declared value type. Its handle gives each key a fully typed leaf, a text map searches across keys with per-key preference, and a sort can fall back through a key list.

## Handles

`handle_fn` picks the handle from the map's `values`: `TextMap` for `text`/`identifier`, `KeywordMap` for `keyword`/`enum`/`uuid`, `DateMap` for dates, `NumberMap` for the numerics.

```rust
Product::name().key("it").matches("scarpe")            // TextMap → Text leaf
Product::labels().key("color").eq("red")               // KeywordMap → Keyword leaf
Product::prices().key("eur").between(10, 50)           // NumberMap → Number leaf
Product::name().has_key("it")                          // presence
Product::name().exists()
```

Runtime keys, compile-time value type: `.key(..)` takes a `&str`, and the leaf has exactly the operators of its kind.

## Cross-key search

```rust
Product::query().query(
    Product::name().search("scarpe")
        .prefer("it", 3.0)
        .prefer("en", 1.0)
)
```

`TextMap::search` builds a `best_fields` `multi_match` over the preferred keys plus a `name.*` fallback. `.only_preferred()` drops the fallback. `KeywordMap` has no `search`; use `.key(..).eq(..)`.

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

`Product::name().sort_key("it").or("en")` reads as "sort by `it`, else `en`". It returns a `MapKeySort` that implements `Sortable`, so it flows through `SortBuilder::by` like any field sort. It renders a `_script` sort whose painless source walks the keys in order and sorts by the first one a document has: true fallback, not lexicographic tiers. String maps sort case-insensitively on the dynamic `.keyword` subfield; number and date maps on the bare key. It's nesting-aware. `missing_first`/`missing_last`/`missing(v)` redirect into the script's parameters with a direction-correct sentinel.

**A single string key is not sortable.** `TextMap::key`/`KeywordMap::key` return `MapKey`-marked leaves without `Sortable`, because a plain `.asc()` would target a nonexistent `name.it.keyword_lowercase` and 400 at runtime. Use `sort_key("it")`. Number and date map keys sort directly; their bare path is doc-valued.

## Related

- [Objects and maps](../reference/objects-and-maps.md#map) for the schema side.
- [Sorting](sorting.md).

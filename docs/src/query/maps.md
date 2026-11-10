# Maps

A schema `map` field has runtime keys and one declared value type. Each key is mapped like a plain field of that type, so its handle is a plain-field handle; the string maps also search and match part of a word across keys, and a sort can fall back through a key list.

## Handles

`handle_fn` picks the handle from the map's `values`: `TextMap` for `text`/`identifier`, `KeywordMap` for `keyword`/`enum`/`uuid`, `DateMap` for dates, `NumberMap` for the numerics.

```rust
Product::name().key("it").matches("scarpe")            // TextMap → Text leaf
Product::labels().key("color").eq("red")               // KeywordMap → Keyword leaf
Product::prices().key("eur").between(10, 50)           // NumberMap → Number leaf
Product::name().has_key("it")                          // presence
Product::name().exists()
```

Runtime keys, compile-time value type: `.key(..)` takes a `&str`, and the leaf is the same handle a plain field of the kind gets. A `TextMap`/`KeywordMap` carries the field's subfield marker (`WithSubfields` when every OpenSearch sink has `auto_subfields` on), so a key has the subfield accessors exactly when a plain field would:

```rust
Product::name().key("en").matches("cafe")                       // finds "Café"
Product::name().key("en").keyword_lowercase().wildcard("*excav*") // part of a word
Product::labels().key("color").eq("Dark Red")                   // exact term on a keyword
```

## Cross-key search

```rust
Product::query().query(
    Product::name().search("scarpe")
        .prefer("it", 3.0)
        .prefer("en", 1.0)
)
```

`TextMap::search` builds a `multi_match` over the preferred keys plus a `name.*` fallback, `best_fields` unless `.match_type(..)` picks another: `MultiMatchType::BoolPrefix` matches the start of a word, as typed in a search box. `.only_preferred()` drops the fallback. `KeywordMap` has no `search`; use `.key(..).eq(..)`.

## Part of a word, across keys

```rust
Product::query().query(Product::name().contains("excav"))   // "Excavator", in any key
```

`contains` (on `TextMap` and `KeywordMap`, with subfields) is a case- and accent-insensitive substring match over every key's `.keyword_lowercase`: a `query_string` of `*excav*` on `name.*.keyword_lowercase`, the value escaped so it's matched literally. `.prefer(key, weight)` and `.only_preferred()` work as on `search`. A leading wildcard scans the terms, so it's slower than `search` on a large index; a value over 256 characters isn't indexed in the subfield and never matches.

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

- `MapKey` is gone: a key is a plain `Text`/`Keyword`. Code that named `Text<S, MapKey>` names `Text<S>` (or `Text<S, NoSubfields>`).
- `TextMap`/`KeywordMap` take a second type parameter, the subfield marker, defaulting to `WithSubfields`.
- A `decimal` map's keys take the decimal kind, like a plain `decimal` field: the handle is `NumberMap<kind::Decimal>` (was `kind::Double`) and the document type `HashMap<String, Decimal>`. A float no longer compiles: `Product::costs().key("eur").eq(9.99)` becomes `.eq(Decimal::new(999, 2))` (or an integer).
- String map sorts read `.keyword_lowercase`, which exists only once the index is rebuilt with 0.17 (it is, once; see [Objects and maps](../reference/objects-and-maps.md#map)).

## Related

- [Objects and maps](../reference/objects-and-maps.md#map) for the schema side.
- [Sorting](sorting.md).

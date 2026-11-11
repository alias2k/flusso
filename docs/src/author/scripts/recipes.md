# Script recipes

Copy-ready script fields for the common jobs; every one here runs in flusso's test suite against the same sample product, so what's shown is what works.

The sample is a product with a `sku` of `"  ab-12 "`, a `name` of `"Red Mug"`, a `decimal` `price` of `12.50`, `stock: 3`, a translated `title` map (`en`, `it`), three `tags` (one repeated), and three `variants` with their own `color`, `price` and `stock`. The inputs a recipe reads but the index doesn't need (`sku`, `price`, `title`, `tags`) are [`emit: false`](../../reference/scripts.md#emit). The source of every recipe is [`libs/2-script/tests/recipes/`](https://github.com/alias2k/flusso/tree/main/libs/2-script/tests/recipes).

## Text

A display label: trim, case, and interpolation. `trim` works in place, hence the variable ([why](language.md#strings)). Gives `"AB-12 · Red Mug"`.

```yaml
{{#include ../../../../libs/2-script/tests/recipes/recipes.schema.yml:label}}
```

A slug. Gives `"red-mug"`.

```yaml
{{#include ../../../../libs/2-script/tests/recipes/recipes.schema.yml:slug}}
```

A fallback when a column is null. Gives `"Red Mug"`, since `nickname` is null.

```yaml
{{#include ../../../../libs/2-script/tests/recipes/recipes.schema.yml:display_name}}
```

## Flags and bands

A boolean from two columns. `price` is a `decimal`, so the threshold is a decimal too ([why](language.md#numbers-and-decimals)). Gives `true`.

```yaml
{{#include ../../../../libs/2-script/tests/recipes/recipes.schema.yml:on_sale}}
```

A band as an ordered `enum`, so the query side sorts it `out < low < plenty`. Gives `"low"`.

```yaml
{{#include ../../../../libs/2-script/tests/recipes/recipes.schema.yml:stock_band}}
```

## Money and scores

A gross price, exact to the cent. Gives `15.25`.

```yaml
{{#include ../../../../libs/2-script/tests/recipes/recipes.schema.yml:gross_price}}
```

A popularity score from two aggregates (an `avg` and a `count` over the reviews), ready for a `function_score` or a sort. A product with no reviews has a null `avgRating`, hence the `??`. Gives `8.5`.

```yaml
{{#include ../../../../libs/2-script/tests/recipes/recipes.schema.yml:popularity}}
```

## Dates

Dates and timestamps arrive as RFC 3339 strings. The year, for a facet. Gives `2024`.

```yaml
{{#include ../../../../libs/2-script/tests/recipes/recipes.schema.yml:created_year}}
```

## Translations

The first language a `map` has, in order of preference, with a last resort. Gives `"Tazza"`. For sorting by language with a fallback at query time instead, see [Sorting](../../query/sorting.md).

```yaml
{{#include ../../../../libs/2-script/tests/recipes/recipes.schema.yml:title_fallback}}
```

## Reshaping a related list

A to-many join arrives as an array of objects. Unique, sorted tag labels as a `keyword` array, a cheap facet without a `nested` query. Gives `["kitchen", "sale"]`.

```yaml
{{#include ../../../../libs/2-script/tests/recipes/recipes.schema.yml:tag_list}}
```

The same labels as one line of text. Gives `"kitchen, sale, kitchen"`.

```yaml
{{#include ../../../../libs/2-script/tests/recipes/recipes.schema.yml:tag_line}}
```

The cheapest variant in stock, or null when none is. Gives `"blue"`.

```yaml
{{#include ../../../../libs/2-script/tests/recipes/recipes.schema.yml:cheapest}}
```

A count with a condition, which an `count` aggregate's [filters](../../reference/aggregates.md) can also do in SQL. Gives `2`.

```yaml
{{#include ../../../../libs/2-script/tests/recipes/recipes.schema.yml:in_stock_count}}
```

## Per element of a join

Inside a `has_many`'s `fields`, a script runs once per child: `this` is that child, `root` the whole product. A variant SKU from the product's and the variant's own data; the blue variant gets `"ab-12-blue"`.

```yaml
{{#include ../../../../libs/2-script/tests/recipes/recipes.schema.yml:per_element}}
```

## Related

- [The Rhai you need](language.md): the language behind these.
- [Test and debug a script](debugging.md): run one over a real row.
- [Script fields](../../reference/scripts.md).

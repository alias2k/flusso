# Field handles

Each schema field generates a handle whose type decides which operators exist. An operator that makes no sense for the field's type isn't there, so the mistake is a compile error, not a 400.

## Handles and operators

| Handle | Operators |
| --- | --- |
| `Keyword` | `eq`, `any_of`, `prefix`, `wildcard`, `regexp`, `fuzzy`, `exists` |
| `Enum` | `eq`, `any_of`, `exists`, `keyword()` for the full `Keyword` surface, and `asc`/`desc` by **declared order** |
| `Text` | `matches`, `match_phrase`, `match_phrase_prefix`, `match_bool_prefix`, `matches_fuzzy`, `any_of` (exact, via `.keyword`), `exists`. No `eq`: it's analyzed. |
| `Bool` | `eq`, `exists`, `asc`/`desc` |
| `Number<K>` | `eq`, `any_of`, `lt`, `lte`, `gt`, `gte`, `between`, `exists` |
| `Date` | `eq`, `any_of`, `lt`, `lte`, `gt`, `gte`, `between`, `exists` |
| object namespace | chains from its parent: `User::account().tier()`, `User::account().exists()` |
| `Nested<S, T>` | `any(q)`, `all(q)` to match parents and lift a child query; `matching(q)` to shape the returned array; `exists`. See [Nested collections](nested.md). |
| `Geo` | `within(Distance::km(12.0), center)`, `within_box`, `within_polygon`, `exists`; sort with `distance_from(center)` or `distance_sort(center, order, unit)` |
| `TextMap` | `key(k)` → `Text`; `search(q)` with `.prefer(key, weight)` / `.only_preferred()`; `has_key`, `exists` |
| `KeywordMap` | `key(k)` → `Keyword`; `has_key`, `exists`. No `search`. |
| `NumberMap`, `DateMap` | `key(k)` → `Number` / `Date`; `has_key`, `exists` |
| `Binary` | `exists` |
| `Json` | `exists`, `raw(serde_json::Value)` |

## Values are typed by kind

An operator's argument is any type implementing `FlussoValue<kind::…>` for the handle's kind (which requires `serde::Serialize`), not one fixed Rust type.

- **Numerics are split per type** (`kind::Byte`, `Short`, `Integer`, `Long`, `Float`, `Double`, `Decimal`) and a value is accepted only if it widens **losslessly**. A `Long` field takes any integer; a `Double` takes `f32`, `f64`, or a small int; a `Decimal` takes `Decimal` or an integer. A float on an integer field, or an `i64` on a `Short`, is a compile error. Bare `eq(5)` works on `long`/`integer`/`double`/`decimal`; `short` needs `eq(5i16)`, since `5` defaults to `i32`.
- `Keyword` takes `&str`/`String` or a `#[derive(FlussoValue)]` keyword enum or newtype, matched against its serde string form.
- `Bool` takes `bool` or a bool newtype.
- `Date` takes an ISO-8601 `&str`, or with the `chrono` feature a `NaiveDate`/`NaiveDateTime`/`DateTime<Utc>`.

A custom money type queries with no cast (`Order::total().eq(Money(d))`) as long as it's a `FlussoValue` of the field's kind; see [Enums and custom values](enums-and-values.md).

## Subfield accessors

The sink enriches `text`/`keyword` fields with subfields (`auto_subfields`, on by default), and the handles expose them typed:

```rust
User::full_name()                      // Text    → analyzed match
User::full_name().keyword()            // Keyword → exact, wildcard, prefix
User::full_name().keyword_lowercase()  // Keyword → case-insensitive match and sort
User::email().text()                   // Text    → full-text over a keyword
```

A `wildcard` belongs on `.keyword()`, not the analyzed handle, which matches tokens. The accessors exist **only when the subfield is provisioned**, and the derive enforces it: a handle is stamped `WithSubfields` only when every OpenSearch sink has `auto_subfields` on and the field declares no custom `fields`. Otherwise the handle is `NoSubfields` and `.keyword()`/`.text()`/`.keyword_lowercase()` (and the `Text::any_of` and `Text::asc` sugar built on them) don't exist.

## Across several fields

`multi_match("ada", [User::full_name(), User::bio()])` runs one analyzed query over several `Text` fields; weight one with `User::full_name().boosted(3.0)`. Options (`type`, `operator`, `fuzziness`, `tie_breaker`, `minimum_should_match`) are on its builder; see [Composing queries and options](composing.md).

## Related

- [Sorting](sorting.md) for which handles sort and how.
- [Field types](../reference/field-types.md) for the schema side of each handle.

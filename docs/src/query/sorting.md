# Sorting

Sort on any sortable handle with `.asc()`/`.desc()`, tune the resulting `Sort`, and collapse a request's optional sort parameters into one list with `SortBuilder`.

## What sorts

`Sortable` is implemented for `Keyword`, `Text`, `Number`, `Date`, `Bool`, and `Enum`; not for `Geo`, objects, or string map keys.

| Handle | `.asc()` / `.desc()` sorts on |
| --- | --- |
| `Keyword`, `Number`, `Date`, `Bool` | the field |
| `Text` | `.keyword_lowercase`, case-insensitive; use `.keyword().desc()` for exact-case |
| `Enum` with declared variants | the prebaked `.sort` subfield, so the **declared order**, no script |
| `Geo` | `distance_from(center)` nearest-first, or `distance_sort(center, order, DistanceUnit)` |
| a map, by key | `sort_key("it").or("en")`; see [Maps](maps.md#sorting-by-key) |

## Tuning a Sort

```rust
User::query()
    .sort(User::created_at().desc().missing_first())
    .sort(User::full_name().asc())
```

On a `Sort`: `.missing_first()`, `.missing_last()`, `.missing(v)`, `.mode(SortMode::..)`, `.unmapped_type(..)`, `.numeric_type(..)`, `.format(..)`, `.nested(path)`, `.nested_filtered(path, q)`. Also `Sort::score()` and `Sort::script(ScriptSortType, source, order)`. `Search`, `MultiSearch`, and `NestedProjection` take plural `.sorts(..)` too.

## Nesting-aware

Every generated scope carries its path from the root as const data, and a sort reads it. `Orders::placed_at().desc()` used at the top level renders the recursive `nested: { path: "orders", nested: {…} }` wrapper with the mode defaulted from the direction, so it's correct without hand-writing the path. Inside a `filter_nested` projection the wrapper is stripped, since the sort is already inside the nested context.

## SortBuilder

A request usually has an optional sort field, a direction, and a fallback. `SortBuilder` collapses that into one list, deduping by field:

```rust
let sorts = SortBuilder::new()
    .by(User::created_at(), params.order)        // Option<dir> skips itself
    .near(User::location(), params.center)       // optional geo distance
    .score_if(params.q.is_some())
    .tiebreak(User::id().asc())
    .or_default(User::created_at().desc())
    .build();

User::query().sorts(sorts).send(&client).await?;
```

`by` takes anything `Sortable` plus an `Into<MaybeOrderBy>`: a bare direction, an `OrderBy` with `missing` handling, or an `Option` of either. `raw(..)` accepts a hand-built sort.

## Related

- [Enums and custom values](enums-and-values.md) for why an enum sorts by declared order.

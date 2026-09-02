# Nested collections

A `has_many` or `many_to_many` is a nested array with its own scope. Filter *by* it to choose parents; filter *of* it to shape the array each parent returns. The two are independent and compose.

## The generated scope

The root generates one namespace per container level into `flusso_<root>_query`. For `users.orders` that's `flusso_user_query::Orders`, whose handles produce `Query<Orders>`, not `Query<Root>`:

```rust
use flusso_user_query::Orders;

fn big_delivered() -> Query<Orders> {
    Orders::status().eq("delivered").and(Orders::total().gt(100))
}
```

An `object` inside the array chains as usual: `Orders::shipping().carrier().eq("dhl")`. A deeper array gets its own namespace, `OrdersItems`.

## Filter by: any and all

`User::orders().any(child)` (or `.all(child)`) takes a `Query<Orders>` and returns a `Query<Root>`, a nested clause at the `orders` path, which composes with root queries like any other:

```rust
let q = User::email().eq("ada@example.com")
    .and(User::orders().any(big_delivered()));
User::query().filter(q).send(&client).await?;
```

The scope tag keeps this honest. `User::email().eq(..).and(Orders::status().eq(..))` **does not compile**: a child constraint can't be applied at the wrong level by accident. Lifting composes through depth: `Orders::items().any(OrdersItems::quantity().gt(1))` is a `Query<Orders>`, which `User::orders().any(..)` lifts the rest of the way.

A matching parent still carries its **whole** array.

## Filter of: filter_nested

`filter_nested` shapes the array each parent comes back with, without changing which parents match:

```rust
let page = User::query()
    .filter(User::orders().any(Orders::status().eq("delivered")))     // by
    .filter_nested(                                                   // of
        User::orders()
            .matching(Orders::status().eq("delivered"))
            .sort(Orders::placed_at().desc())
            .size(5),
    )
    .send(&client).await?;

for hit in &page.hits {
    for order in &hit.source.orders {   // delivered, newest first, at most 5
    }
}
```

`matching(q)` takes a `Query<Orders>`; `.sort`, `.size`, `.from` are optional, and so is `matching` itself (drop it to keep every child but sort or cap). The client fetches the nested matches through `inner_hits` and **replaces** `hit.source.orders` with the subset before deserializing. A parent with no matching children still comes back, with an empty array; pair with `any` to drop those too.

## Depth

`filter_nested` shapes one level. Matching on deeper nesting inside the predicate works (`Orders::items().any(..)`) and the returned orders honor it, but returning a filtered `items` array inside each order is left to the `raw` hatch.

## Sorting on nested fields

A nested field sort is nesting-aware: `Orders::placed_at().desc()` at the top level renders the `nested: { path }` chain itself. See [Sorting](sorting.md).

## Related

- [Composing queries and options](composing.md) for `score_mode` and `ignore_unmapped` on `any`.
- [Joins](../reference/joins.md) for the schema side.

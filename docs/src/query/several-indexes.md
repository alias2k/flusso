# Several indexes

Two multi-index shapes: independent searches sharing one round-trip, and one query blended across indexes into a single ranked list.

## Several searches, one round-trip

`client.msearch(..)` takes a tuple of `&Search<T>` (arity 1 to 8, mixed document types) and returns one typed response per slot, in order. Each slot keeps its own query, sort, pagination, and `filter_nested`.

```rust
let users_q  = User::query().query(User::full_name().matches(&q)).size(10);
let orders_q = Order::query().filter(Order::status().eq("open")).size(5);

let (users, orders) = client.msearch((&users_q, &orders_q)).await?;
```

A slot-level failure fails the whole call naming the slot; no partial results. For many searches of **one** type, `client.msearch_all(&searches)` returns `Vec<SearchResponse<T>>`.

## One blended list

Declare which document types blend as an enum with one variant per type:

```rust
#[derive(Debug, FlussoMultiDocument)]
enum StoreItem {
    User(User),
    Order(Order),
}

let page = StoreItem::query()
    .query(multi_match("ada", [User::full_name(), Order::customer_name()]))
    .size(20)
    .send(&client)
    .await?;

for hit in page.hits {
    match hit.source {
        StoreItem::User(u) => …,
        StoreItem::Order(o) => …,
    }
}
```

Root-scope queries compose across types because `Query<Root>` carries no document type. Each hit is decoded into the variant owning its `_index`. `count(&client)` works on the union too.

Two semantics:

- A **query** on a field that exists in only one index is fine; it doesn't match in the others.
- A **sort** on such a field is rejected by OpenSearch unless it carries an `unmapped_type`. Sort blended results by relevance, or on shared fields.

## Generation suffix normalization

The query goes through the hash alias `{logical}_{hash}`, but OpenSearch reports the concrete generation `{logical}_{hash}_{n}` in each hit's `_index`. The decoder collapses that suffix back to the union's known targets before dispatch, anchored on those targets rather than a blind trailing-digits trim, because an eight-hex hash can itself be all digits. A hit from an index no variant claims is an error, not a skip. The client's index prefix is stripped the same way.

## Without the derive

`FlussoMultiDocument` is two members: a `TARGETS` const listing each variant's `(INDEX, SCHEMA_HASH)`, and a `decode` matching on the physical index. The derive validates the enum's shape (single-field tuple variants, no duplicate payload types) and writes both.

## Related

- [Results and the escape hatch](results-and-escape-hatch.md) for the response types.
- [Binding to the schema](binding.md#reading-a-prefixed-deployment).

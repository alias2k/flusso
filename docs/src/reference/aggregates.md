# Aggregates

An aggregate reduces rows of a related table to one value. The operation is the type key.

| Type key | Result | Nullable | Takes |
| --- | --- | --- | --- |
| `count` | `long` | no; zero rows is `0` | no `column`, no `value_type` |
| `sum` | `value_type` | yes; null over zero rows | `column` + `value_type` |
| `avg` | `double` | yes | `column` |
| `min` | `value_type` | yes | `column` + `value_type` |
| `max` | `value_type` | yes | `column` + `value_type` |
| `ids` | array of `element_type` | no; empty is `[]` | `element_type` |

| Sibling | Type | Applies to | Meaning |
| --- | --- | --- | --- |
| `table` | [Postgres identifier](identifiers.md) | all | The related table. Required. |
| `foreign_key` | Postgres identifier | all | The related table's column pointing back at the parent. Exactly one of `foreign_key` or `through`. |
| `through` | table | all | A junction, as in [Joins](joins.md#through), including its own `filters`. Exactly one of `foreign_key` or `through`. See [Over a junction](#over-a-junction). |
| `distinct` | bool, default `false` | aggregates over `through` | `true` counts each target row once instead of once per junction row. Rejected on a `foreign_key` aggregate. |
| `column` | Postgres identifier | `sum`, `avg`, `min`, `max` | The column to reduce. Required there; `count` doesn't read it; `ids` rejects it. |
| `value_type` | a scalar type key | `sum`, `min`, `max` | The result type; it mirrors the column. Required there; `count` and `avg` don't read it; `ids` rejects it. |
| `element_type` | a scalar type key | `ids` | The type of each collected key, usually `long` or `keyword`; `geo` and `custom` are rejected. Required on `ids`, rejected on every other op. |
| `filters` | list | all | Which rows of `table` count. Junction rows are filtered by `through.filters`. See [Filters](filters-and-soft-delete.md). |

`required` is rejected on aggregates; their nullability is structural, as in the first table.

## ids

`ids` collects the related table's **primary key** into a flat scalar array. It takes no `column`. OpenSearch has no array type, so the mapping is the element type and the value is multi-valued. Project it as a bare `Vec<_>` on the query side, never `Option<Vec<_>>`.

## Over a junction

An aggregate over `through` reads the target `table` via a junction. Three rules decide what it computes:

- **One value per junction row, by default.** Two junction rows pointing at the same target row count it twice. That's what line-level totals want: two lines of the same product weigh twice.
- **`distinct: true` counts each target row once**, however many junction rows reach it. It dedups by the target's primary key, not by value, so two different products of equal weight both count. On `min`/`max` it changes nothing; on `ids` it drops repeated ids.
- **A junction row whose target row is missing never counts**, for any op, `ids` included.

Two filter lists apply, each scoped by where it sits:

- `filters` on the aggregate narrows the **target** rows.
- `filters` inside `through` narrows the **junction** rows.

A change to a junction row rebuilds its parent through `left_key`, so flipping a junction column a filter reads (a line's `status`, say) re-evaluates the aggregate.

```yaml
# orders → order_items → products, skipping cancelled lines and archived products.
- sum: totalWeight
  table: products
  column: weight
  value_type: double
  filters:
    - { column: archived, op: eq, value: false }
  through:
    table: order_items
    left_key: order_id
    right_key: product_id
    filters:
      - { column: status, op: neq, value: cancelled }

# How many distinct products the order holds.
- count: productCount
  table: products
  distinct: true
  through: { table: order_items, left_key: order_id, right_key: product_id }
```

## Example

```yaml
- count: orderCount
  table: orders
  foreign_key: user_id

- sum: lifetimeValue
  table: orders
  column: total
  value_type: decimal
  foreign_key: user_id
  filters:
    - { column: status, op: eq, value: paid }

- max: lastOrderAt
  table: orders
  column: placed_at
  value_type: timestamp
  foreign_key: user_id

- ids: tagIds
  table: tags
  through: { table: post_tags, left_key: post_id, right_key: tag_id }
  element_type: long
```

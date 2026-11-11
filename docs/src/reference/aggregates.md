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
| `exists` | `boolean` | no; zero rows is `false` | no `column`, no `value_type` |

| Sibling | Type | Applies to | Meaning |
| --- | --- | --- | --- |
| `table` | [Postgres identifier](identifiers.md) | all | The related table. Required. |
| `foreign_key` | Postgres identifier | all | The related table's column pointing back at the parent. Exactly one of `foreign_key` or `through`. |
| `through` | table | all | A junction, as in [Joins](joins.md#through), including its own `filters`. Exactly one of `foreign_key` or `through`. See [Over a junction](#over-a-junction). |
| `via` | list of hops | all | The tables between the parent and `table`, in order from the parent. With `via`, `foreign_key` (or `through.left_key`) points at the last hop instead of the parent. See [Across several hops](#across-several-hops). |
| `distinct` | bool, default `false` | aggregates over `through` | `true` counts each target row once instead of once per junction row. Rejected on a `foreign_key` aggregate, with or without `via`. |
| `column` | Postgres identifier | `sum`, `avg`, `min`, `max` | The column to reduce. Required there; `count` doesn't read it; `ids` and `exists` reject it. |
| `value_type` | a scalar type key | `sum`, `min`, `max` | The result type; it mirrors the column. Required there; `count` and `avg` don't read it; `ids` and `exists` reject it. |
| `element_type` | a scalar type key | `ids` | The type of each collected key, usually `long` or `keyword`; `geo` and `custom` are rejected. Required on `ids`, rejected on every other op. |
| `filters` | list | all | Which rows of `table` count. Junction rows are filtered by `through.filters`. See [Filters](filters-and-soft-delete.md). |

`required` is rejected on aggregates; their nullability is structural, as in the first table.

## ids

`ids` collects the related table's **primary key** into a flat scalar array. It takes no `column`. OpenSearch has no array type, so the mapping is the element type and the value is multi-valued. Project it as a bare `Vec<_>` on the query side, never `Option<Vec<_>>`.

## exists

`exists` is `true` when at least one row of `table` matches, after every filter, and `false` otherwise. It is never null, so project it as a bare `bool`. It works on a `foreign_key`, `through`, or `via` aggregate alike; use it instead of a `count` when the list only filters on presence.

## Across several hops

`via` reaches a table more than one hop away: the user's tasks through their projects, say. Each entry is one table between the parent and `table`, in order from the parent:

| Key | Type | Meaning |
| --- | --- | --- |
| `table` | Postgres identifier | The table this hop passes through. Required. |
| `foreign_key` | Postgres identifier | The column on this hop's table pointing at the table before it (the parent, for the first hop). Required. |
| `filters` | list | Which rows of this hop's table the path passes through. See [Filters](filters-and-soft-delete.md). |

- **Hops go down foreign keys only.** Each hop's table holds the key. A `column` on a hop (the `belongs_to` direction) and a `through` on a hop (a junction in the middle) are rejected at load.
- **The last hop can still be a junction.** Write it as the aggregate's own `through`, whose `left_key` then points at the last `via` table.
- **Each row counts once.** A row has one parent along a foreign-key chain, so no path reaches it twice; that's why `distinct` stays rejected without `through`. A final junction can reach a target row once per junction row, so `distinct: true` keeps its meaning there: each target row once.
- **[`flusso check`](cli.md#check) doesn't verify the hops.** It doesn't check that a hop's `table`, `foreign_key`, or filter columns exist, same as a one-hop `foreign_key`, so a typo surfaces when a document is built.
- **Every hop rebuilds the parent.** Inserting, deleting, or re-parenting a row on any hop, or changing a column a hop's `filters` read, rebuilds the parent document, the old parent included (each hop's `foreign_key` must be in its table's replica identity; see [Deleted and re-parented rows](source-postgres.md#deleted-and-re-parented-rows)).

```yaml
# users → projects → tasks: does the user have a completed task in a live project?
- exists: hasCompletedTask
  table: tasks
  foreign_key: project_id        # points at the last hop, projects
  filters:
    - { column: status, op: eq, value: completed }
  via:
    - table: projects
      foreign_key: user_id       # points at the parent, users
      filters:
        - { column: archived, op: eq, value: false }

# users → teams → projects → tasks → task_tags → tags: distinct tags on the user's tasks.
- count: tagCount
  table: tags
  distinct: true
  through: { table: task_tags, left_key: task_id, right_key: tag_id }
  via:
    - { table: teams, foreign_key: owner_id }
    - { table: projects, foreign_key: team_id }
    - { table: tasks, foreign_key: project_id }
```

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

- exists: hasOpenTicket
  table: tickets
  foreign_key: user_id
  filters:
    - { column: status, op: eq, value: open }
```

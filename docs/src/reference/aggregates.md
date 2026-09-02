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
| `through` | table | all | A junction, as in [Joins](joins.md#through). Exactly one of `foreign_key` or `through`. |
| `column` | Postgres identifier | `sum`, `avg`, `min`, `max` | The column to reduce. Required there; `count` and `ids` don't read it. |
| `value_type` | a scalar type key | `sum`, `min`, `max` | The result type; it mirrors the column. Required there; the other ops don't read it. |
| `element_type` | a scalar type key | `ids` | The type of each collected key, usually `long` or `keyword`; `geo` and `custom` are rejected. Required on `ids`, rejected on every other op. |
| `filters` | list | all | Which rows count. See [Filters](filters-and-soft-delete.md). |

`required` is rejected on aggregates; their nullability is structural, as in the first table.

## ids

`ids` collects the related table's **primary key** into a flat scalar array. It takes no `column`. OpenSearch has no array type, so the mapping is the element type and the value is multi-valued. Project it as a bare `Vec<_>` on the query side, never `Option<Vec<_>>`.

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

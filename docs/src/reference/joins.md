# Joins

A join folds rows from a related table into the document. The relationship verb is the type key, and the verb names which table holds the key.

| Type key | The key lives on… | Reads as | Renders as |
| --- | --- | --- | --- |
| `belongs_to` | **this** table (`column`) | my column points at the related row | object, nullable; `required: true` makes it non-null |
| `has_one` | the **related** table (`foreign_key`) | one related row points back at me | object, nullable; `required: true` makes it non-null |
| `has_many` | the **related** table (`foreign_key`) | many related rows point back at me | nested array, never null |
| `many_to_many` | a junction table (`through`) | we connect through a junction | nested array, never null |

| Sibling | Type | Applies to | Meaning |
| --- | --- | --- | --- |
| `table` | [Postgres identifier](identifiers.md) | all | The related table. Required. |
| `primary_key` | Postgres identifier | all | The related table's primary key. Required. The projected field reading it is forced non-null. |
| `column` | Postgres identifier | `belongs_to` | This table's column pointing at the related row. Defaults to the document key. |
| `foreign_key` | Postgres identifier | `has_one`, `has_many` | The related table's column pointing back at the parent. Required. |
| `through` | table | `many_to_many` | The junction. Required. See [through](#through). |
| `fields` | list | all | The fields projected from each related row. Required. |
| `required` | bool | `belongs_to`, `has_one` | `true` maps the object non-null. Rejected on to-many verbs. |
| `filters` | list | all | Narrow which related rows fold in. See [Filters](filters-and-soft-delete.md). |
| `order_by` | list of `{ column, direction }` | `has_one`, `has_many`, `many_to_many` | Ordering; `direction` is `asc` (default) or `desc`. On `has_one` it picks which row becomes the object. Rejected on `belongs_to`. |
| `limit` | int ≥ 1 | `has_many`, `many_to_many` | Cap the rows folded in. The to-one verbs imply `LIMIT 1`. |

**Key arity rule.** A join takes exactly the key sibling its verb implies. Anything else is a load-time error naming the right one.

## through

| Key | Meaning |
| --- | --- |
| `table` | The junction table. |
| `left_key` | Junction column pointing at the parent. |
| `right_key` | Junction column pointing at the related table. |

## What a related change rebuilds

Every join is resolved in reverse. A changed related row is mapped back to the parent documents through the key the verb names, and each is rebuilt from the current rows. A `belongs_to` target that changes or is deleted re-emits every document pointing at it: the referrers are found on the parent table (`WHERE column = <key>`), so a deleted target rebuilds them with a null object rather than leaving them stale. Every joined table must be in the publication; see [Source: Postgres](source-postgres.md#capture).

## Example

```yaml
# My column points at them.
- belongs_to: created_by
  table: users
  primary_key: id
  fields:
    - keyword: email
      required: true

# Their column points at me.
- has_many: orders
  table: orders
  foreign_key: user_id
  primary_key: id
  filters:
    - { column: status, op: neq, value: cancelled }
  order_by:
    - { column: placed_at, direction: desc }
  limit: 5
  fields:
    - decimal: total
      required: true
    - keyword: status
      required: true

# Through a junction.
- many_to_many: tags
  table: tags
  through:
    table: post_tags
    left_key: post_id
    right_key: tag_id
  primary_key: id
  fields:
    - keyword: name
      required: true
```

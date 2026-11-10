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
| `indexed` | bool | all | `false` keeps the rows in the stored document without indexing them. Default `true`. See [indexed](#indexed). |

**Key arity rule.** A join takes exactly the key sibling its verb implies. Anything else is a load-time error naming the right one.

## through

| Key | Meaning |
| --- | --- |
| `table` | The junction table. |
| `left_key` | Junction column pointing at the parent. |
| `right_key` | Junction column pointing at the related table. |
| `filters` | Narrow the junction rows, e.g. skip a removed membership. The join's own `filters` narrow the related table. See [Filters](filters-and-soft-delete.md). |

The same `through` shape keys an aggregate; see [Aggregates](aggregates.md#over-a-junction) for what a junction aggregate counts.

## indexed

`indexed: false` keeps a subtree in the document without indexing it. It applies to every join verb and to a `json` field.

```yaml
- has_many: priceHistory
  table: price_changes
  foreign_key: product_id
  primary_key: id
  indexed: false
  fields:
    - decimal: amount
      required: true
    - timestamp: changedAt
      column: changed_at
      required: true
```

- **Built and stored as usual.** The rows are read, folded in, and kept live by the same change capture; they land in `_source` and come back with every hit.
- **Mapped as one field.** The OpenSearch sink maps the whole subtree as `{"type": "object", "enabled": false}`, whatever the verb (a to-many join is not `nested`). Nothing under it counts toward the mapping limits, and nothing under it is parsed, so a value a mapped field would refuse is accepted.
- **Not searchable.** Nothing under it can be queried, filtered, sorted, or aggregated. On the query side the field has no handle; the struct field still deserializes. See [Document structs](../query/document-structs.md#stored-only-fields).
- **Fields under it keep their meaning for the document**: columns, transforms, nested joins, and filters all apply. Their mapping-only settings (`options`, `subfields`, `indexed`, an `enum`'s `variants` order, a `map`'s typed keys) have no effect and are accepted, so toggling `indexed` never forces an edit below.
- **No `options` beside it.** A field with `indexed: false` rejects `options` at load: the disabled object owns its mapping.
- **Aggregates are separate fields.** A `count` or `sum` over the same table is its own leaf on the parent and stays searchable.
- **Changing it** moves the schema to a new content hash, so the index is rebuilt once. Omitted or `true` keeps the subtree indexed.
- **Anywhere else** (`object`, a scalar, a `map`, an aggregate) the key is rejected at load.

## What a related change rebuilds

Every join is resolved in reverse. A changed related row is mapped back to the parent documents through the key the verb names, and each is rebuilt from the current rows. A `belongs_to` target that changes or is deleted re-emits every document pointing at it: the referrers are found on the parent table (`WHERE column = <key>`), so a deleted target rebuilds them with a null object rather than leaving them stale. Every joined table must be in the publication; see [Source: Postgres](source-postgres.md#capture).

A join can point back at the index's own table: a tree of `items` whose `has_many: children` reads `items` over `parent_id`. A change on that table rebuilds its own document **and** every document that embeds the row, to the depth the schema declares. `children` nested once under `children` rebuilds the parent and the grandparent, never further. `belongs_to`, `has_one`, `many_to_many`, and aggregates over the root table behave the same way. A [`soft_delete`](filters-and-soft-delete.md#soft_delete) applies to the root row only: a soft-deleted child tombstones its own document but still appears under its parent unless the join carries its own `filters` on the soft-delete column. A deleted or re-parented row leaves its old parent like any other child, so the root table's replica identity has to carry the link column too; see [Deleted and re-parented rows](source-postgres.md#deleted-and-re-parented-rows). A schema with no join back at its own table pays nothing for this.

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

# Filters and soft_delete

Filters narrow which rows a join or aggregate sees, which rows a `soft_delete` applies to, and, at the top level, which root rows become documents at all. Three forms, one grammar.

| Form | Shape | Example |
| --- | --- | --- |
| Raw SQL | `{ raw: "<condition>" }` | `{ raw: "amount > 0 AND currency = 'USD'" }` |
| Null check | `{ column, op }` with `op` in `is_null`, `is_not_null` | `{ column: deleted_at, op: is_null }` |
| Value comparison | `{ column, op, value }` | `{ column: status, op: in, value: [paid, shipped] }` |

| `op` | `value` shape |
| --- | --- |
| `eq`, `neq`, `lt`, `lte`, `gt`, `gte`, `like`, `ilike` | one scalar |
| `in`, `not_in` | a list |
| `between` | a list of exactly two, `[lower, upper]` |
| `is_null`, `is_not_null` | none |

A list is `AND`ed: every filter must hold. A value op without a value, a list op given a scalar, or a `between` with other than two values is a load-time error.

## Root filters

The top-level `filters` key scopes which root rows are documents.

```yaml
version: 1
table: item
primary_key: id
filters:
  - { column: item_type, op: eq, value: serialized }
  - { column: archived_at, op: is_null }
```

A row outside the set never produces a document. A row that **leaves** the set emits a tombstone on its next rebuild, exactly like `soft_delete`: both fold into the document query's `WHERE`, so "no row came back" means "this document should not exist". A row that enters the set upserts. Backfill walks the whole root table and lets the same predicate decide.

## soft_delete

| Key | Required | Meaning |
| --- | --- | --- |
| `column` **or** `field` | exactly one | The column ([Postgres identifier](identifiers.md)) or mapped field (document key) that signals deletion. |
| `when` | no | Filters; the soft-delete applies only to matching rows. |

A row matching the condition emits a **tombstone** (a `delete` to the sink) instead of an upsert. Clearing the marker restores the document on the next rebuild.

```yaml
# Off a column.
soft_delete:
  column: deleted

# Off a mapped field, narrowed.
soft_delete:
  field: status
  when:
    - { column: archived, op: eq, value: true }
```

The marker column is read from the current row when the document is rebuilt, so a boolean flag and a `deleted_at` timestamp both work. If the row is deleted outright, the tombstone comes from the WAL delete and needs the table's row identity; see [Source: Postgres](source-postgres.md#server-requirements).

## Example

```yaml
- has_many: orders
  table: orders
  foreign_key: user_id
  primary_key: id
  filters:
    - { column: status, op: not_in, value: [cancelled, refunded] }
    - { column: total, op: gt, value: 0 }
  fields:
    - decimal: total
      required: true
```

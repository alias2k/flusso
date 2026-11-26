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

## How a value is compared

Postgres compares a value with the column, as the column's type **without its size**: `varchar` for a `varchar(5)`, `numeric` for a `numeric(10,2)`, the base type for a domain. A value is never cut, padded, or rounded to fit, so one that doesn't fit is simply a value no row holds:

| Column | Filter | Matches a row holding |
| --- | --- | --- |
| `varchar(5)` | `{ column: status, op: eq, value: archived }` | nothing (not `archi`) |
| `varchar(5)` | `{ column: status, op: neq, value: archived }` | every non-null value, `archi` included |
| `numeric(10,2)` | `{ column: total, op: eq, value: 1.005 }` | nothing (not `1.01`) |
| `numeric(10,2)` | `{ column: total, op: gt, value: 1.005 }` | `1.01` and up |
| `bit(5)` | `{ column: flags, op: eq, value: "101" }` | nothing (not `10100`) |
| `time(0)` | `{ column: opens, op: eq, value: "09:00:00.6" }` | nothing (not `09:00:01`) |
| `char(5)` | `{ column: code, op: eq, value: "ab" }` | `ab` |

`char(n)` keeps its own equality: trailing spaces never count, so `ab` equals the stored `ab   `, and `abcde  ` equals `abcde`. Any other character past the size is a value no row holds. On `varchar(n)` trailing spaces count: `'archi  '` doesn't equal a stored `archi`. The same holds for `varbit(n)`, the time types with a precision (`timestamp(3)`), an `interval` with a precision or fields (`interval year`), and a domain over any of them, whose `CHECK` isn't applied to the value either.

A filter on an array of a domain over a sized type (`short_text[]` over `varchar(5)`) still fails the build when an element doesn't fit: the array cast applies the element domain's size and refuses the value instead of cutting it, as before 0.18.

Before 0.18 the value was cast to the sized type, which cut or rounded it: see [Upgrade to 0.18](../operate/upgrade-to-0-18.md#a-filter-value-is-compared-as-written).

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

### How the marker is verified

`field` resolves to the first field of that name, depth-first in declaration order, joins included; the column behind it is read on the **root** row, even when the field sits inside a join. The marker and its `when` are checked by [`flusso check`](checks.md#what-is-checked) and at every [`flusso run` start](checks.md#validation-at-start), each reported under `soft_delete`:

| Problem | Message | Needs the database |
| --- | --- | --- |
| No field has that name | `` `field` names `deleted`, but no field has that name `` | no |
| The first field of that name is a join, a group, or anything but a column | `` `field` names `team`, but the first field of that name is not a column `` | no |
| The marker column isn't on the root table | `` `column` references unknown column `public.users.deleted_at` `` | yes |
| A field found inside a join names a column the root table lacks | `` `field` references unknown column `public.users.status` `` | yes |
| A value or null-check `when` filter names a column the root table lacks | `` `when` references unknown column `public.users.stat` `` | yes |

All are errors. A `field` that reads no column used to be ignored without a word, marker and `when` both, so soft-deleted rows stayed indexed; now `check` fails on it, `--offline` included, and `run` refuses to start. A `raw` `when` filter is passed to the database unread and isn't checked.

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

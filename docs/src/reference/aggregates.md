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
| `table` | [Postgres identifier](identifiers.md) | all | The related table. Required. It needs a primary key, as does every `via` table; a `through` junction doesn't ([details](source-postgres.md#tables-without-a-primary-key)). |
| `foreign_key` | Postgres identifier | all | The related table's column pointing back at the parent. Exactly one of `foreign_key` or `through`. |
| `through` | table | all | A junction, as in [Joins](joins.md#through), including its own `filters`. Exactly one of `foreign_key` or `through`. See [Over a junction](#over-a-junction). |
| `via` | list of hops | all | The tables between the parent and `table`, in order from the parent. With `via`, `foreign_key` (or `through.left_key`) points at the last hop instead of the parent. See [Across several hops](#across-several-hops). |
| `on` | list of `{ local, related }` | `foreign_key` aggregates | Extra column equalities ANDed with the key. Rejected over `through`. See [on](#on). |
| `distinct` | bool, default `false` | aggregates over `through` | `true` counts each target row once instead of once per junction row. Rejected on a `foreign_key` aggregate, with or without `via`. |
| `column` | Postgres identifier, or an [arithmetic expression](#arithmetic-input) | `sum`, `avg`, `min`, `max` | What to reduce: a column of `table`, or arithmetic over its columns. Required there; `count` doesn't read it; `ids` and `exists` reject it. |
| `value_type` | a scalar type key | `sum`, `min`, `max` | The result type; it mirrors the column, and must be numeric over an [expression](#arithmetic-input). Required there; `count` and `avg` don't read it; `ids` and `exists` reject it. |
| `element_type` | a scalar type key | `ids` | The type of each collected key, usually `long` or `keyword`; `geo` and `custom` are rejected. Required on `ids`, rejected on every other op. |
| `filters` | list | all | Which rows of `table` count. Junction rows are filtered by `through.filters`. See [Filters](filters-and-soft-delete.md). |

`required` is rejected on aggregates; their nullability is structural, as in the first table.

## Arithmetic input

`column` on `sum`, `avg`, `min` and `max` takes a column name or an arithmetic expression over the columns of `table`. Postgres evaluates it once per aggregated row, inside the aggregate: `sum(quantity * unit_price)`.

```yaml
# An order's value: quantity × price over its live lines.
- sum: linesValue
  table: order_lines
  foreign_key: order_id
  column: quantity * unit_price
  value_type: double
  filters:
    - { column: archived_at, op: is_null }
```

| Element | Written as |
| --- | --- |
| Column | A column of `table`, following the [identifier](identifiers.md) rules (lowercased on load). |
| Number | Digits, optionally a `.` and more digits: `2`, `0.5`, `100.25`. No exponent, no leading `.`. |
| Operators | `+ - * /`, and unary minus (`-quantity`). |
| Grouping | Parentheses. |

- **Usual precedence.** Unary minus binds tightest, then `*` and `/`, then `+` and `-`. Operators of equal precedence group from the left: `a - b - c` is `(a - b) - c`.
- **Columns of the aggregated table only.** No parent-row, `via`-hop, or junction column; for those, filter or join instead.
- **No functions, casts, or other operators.** `round(…)`, `::numeric`, `%` and `||` are rejected.
- **Postgres arithmetic, as is.** Integer operands divide as integers, so `quantity / 2` truncates; write `quantity / 2.0` for a fraction. A null operand makes that row's value null, and `sum`/`avg`/`min`/`max` skip null rows. Integer overflow is an error; see the warning below.
- **A zero divisor makes the row null.** Every `/` divides by `NULLIF(divisor, 0)`, so `total / quantity` on a row with `quantity = 0` is null for that row and the aggregate skips it, like a null operand. When every row has a zero divisor, `sum`/`avg`/`min`/`max` are null. `count` and `exists` still count the row.
- **Quote a literal-only expression.** YAML reads `column: 1` as a number, not a string, and the load rejects it. Write `column: "1"`.
- **Same result types, numeric only.** `value_type` stays required on `sum`/`min`/`max` and states the result; `avg` is a `double`. Over an expression it must be numeric (`short`, `integer`, `long`, `float`, `double`, `decimal`): `keyword`, `date`, `boolean` and the rest are rejected at load. Pick one that holds what the arithmetic produces. `/` on a `numeric` column or a decimal literal (`* 1.5`) yields fractions, which an integer `value_type` misdescribes and a typed reader can't decode; use `double` or `decimal` there.
- **Load-time errors carry a position.** A malformed expression fails the load naming the field and the 1-based character where parsing stopped, e.g. `` `sum` field `linesValue`: invalid expression `quantity *`: at position 11: expected a column, a number, `-` or `(`, found the end ``. Nesting is capped at 64 levels, counting parentheses, unary minus, and chained operators.
- **Spelling doesn't matter.** Whitespace and redundant parentheses are dropped: `quantity*unit_price` and `quantity * (unit_price)` are the same input, keep the same physical index, and write the same `flusso.lock`. A plain column name is unchanged from before expressions existed.
> ⚠️ **Warning** — integer overflow stalls the whole deployment. An integer result past its type (`integer * integer` beyond 2³¹) makes Postgres fail the document query. That fails the whole batch: the ingest engine restarts and retries it, every index stalls, and WAL stays pinned until the offending row changes. A backfill over such a row fails the same way. Rule it out in the schema: lead with a decimal literal (`1.0 * quantity * unit_price`) so every step runs in `numeric`, or widen the column type (`bigint`, `numeric`) in Postgres. A trailing `* 1.0` is too late, since operators group left to right.

- **Every column in the expression rebuilds the parent.** Changing any of them on an aggregated row rebuilds its parent document; an update that changes none of them, nor the aggregate's key or filter columns, rebuilds nothing.
- **[`flusso check`](cli.md#check) verifies the columns.** Each column the expression names must exist on `table` and be numeric. A plain column keeps its own check: the declared `value_type` must accept its database type.

## ids

`ids` collects the related table's **primary key** into a flat scalar array. It takes no `column`. OpenSearch has no array type, so the mapping is the element type and the value is multi-valued. Project it as a bare `Vec<_>` on the query side, never `Option<Vec<_>>`.

The keys go into the array as they are, so `element_type` has to hold the key's type. [`flusso check`](cli.md#check) compares the two:

| `element_type` | Holds every key of | Holds only some (warning) | Changes them (warning) | Holds none (error) |
| --- | --- | --- | --- | --- |
| `keyword`, `text`, `identifier`, `enum`, `uuid` | any type | | | |
| `short` | `smallint` | wider integers, `numeric`, floats, text | | anything else |
| `integer` | `smallint`, `integer` | `bigint`, `numeric`, floats, text | | anything else |
| `long` | the integers | `numeric`, floats, text | | anything else |
| `float` | `smallint`, `real` | `numeric`, `double precision`, text | `integer`, `bigint`: rounded past 2^24 | anything else |
| `double`, `decimal` | `smallint`, `integer`, `numeric`, floats | text | `bigint`: rounded past 2^53 | anything else |
| `date`, `timestamp` | `date`, `timestamp`, `timestamptz` | `numeric`, floats, text | the integers: read as epoch milliseconds | anything else |
| `boolean` | `boolean` | text | | anything else |
| `binary` | `bytea` | | anything else: indexed as written, not as bytes | |
| `json` | | | | any type |

- **Holds none** is `ids_element_mismatch`: OpenSearch rejects every document that collects a key, so [`run` refuses to start](cli.md#validation-at-start).
- **Holds only some** and **changes them** are `ids_element_lossy`, a warning. A document whose keys don't fit is rejected by OpenSearch; its index's [`on_error`](index-and-on-error.md#on_error) decides what happens next.
- **Special values.** Even a matching pair can't hold a `numeric` or float `NaN` or infinity, or an infinite date; a document collecting one is rejected the same way.

## exists

`exists` is `true` when at least one row of `table` matches, after every filter, and `false` otherwise. It is never null, so project it as a bare `bool`. It works on a `foreign_key`, `through`, or `via` aggregate alike; use it instead of a `count` when the list only filters on presence.

## Across several hops

`via` reaches a table more than one hop away: the user's tasks through their projects, say. Each entry is one table between the parent and `table`, in order from the parent:

| Key | Type | Meaning |
| --- | --- | --- |
| `table` | Postgres identifier | The table this hop passes through. Required. |
| `foreign_key` | Postgres identifier | The column on this hop's table pointing at the table before it (the parent, for the first hop). Required. |
| `filters` | list | Which rows of this hop's table the path passes through. See [Filters](filters-and-soft-delete.md). |
| `on` | list of `{ local, related }` | Extra column equalities between the table before this hop (`local`) and this hop's table (`related`). See [on](#on). |

- **Hops go down foreign keys only.** Each hop's table holds the key. A `column` on a hop (the `belongs_to` direction) and a `through` on a hop (a junction in the middle) are rejected at load.
- **The last hop can still be a junction.** Write it as the aggregate's own `through`, whose `left_key` then points at the last `via` table.
- **Each row counts once.** A row has one parent along a foreign-key chain, so no path reaches it twice; that's why `distinct` stays rejected without `through`. A final junction can reach a target row once per junction row, so `distinct: true` keeps its meaning there: each target row once.
- **[`flusso check`](cli.md#check) verifies the hops**: each hop's `table`, `foreign_key`, `on:` pairs and filter columns.
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

## on

`on` narrows an aggregate's hop with column equalities beside its key, the same as on a join; [Joins](joins.md#on) owns the syntax, the null rule, and what rebuilds. On an aggregate, each hop has its own pairs:

- **The aggregate's own `on`** relates the table right before `table` (`local`) to `table` (`related`): the parent for a one-hop aggregate, the last `via` hop otherwise.
- **A `via` hop's `on`** relates the table before that hop (the parent, for the first) to the hop's table.
- **Not over `through`.** A junction matches on `left_key`/`right_key` only; `on` on an aggregate over `through` is rejected at load. With `via` and a final `through`, the `via` hops still take `on`.

```yaml
# orders → shipments → parcels, every hop scoped to the order's tenant.
- count: parcelCount
  table: parcels
  foreign_key: shipment_id
  on:
    - { local: tenant_id, related: tenant_id }   # shipments.tenant_id = parcels.tenant_id
  via:
    - table: shipments
      foreign_key: order_id
      on:
        - { local: tenant_id, related: tenant_id } # orders.tenant_id = shipments.tenant_id
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

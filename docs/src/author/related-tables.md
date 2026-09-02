# Fold in related tables

Embed rows from other tables into a document as an object, a nested array, or a rollup, and make sure Postgres replicates every table the document reads.

## When to use this

A search document needs more than its root row: a user with their orders, a product with its tags and average rating. Joins fold rows in; aggregates reduce them to one value. Both rebuild the parent document when a related row changes.

## Steps

1. **Pick the verb by where the key lives.** The verb is the type key, and it names which table holds the foreign key.

   | The key is on… | Verb | Renders as |
   | --- | --- | --- |
   | this table, pointing at one related row | `belongs_to` | nullable object |
   | the related table, one row points back | `has_one` | nullable object |
   | the related table, many rows point back | `has_many` | nested array |
   | a junction table | `many_to_many` | nested array |

2. **Embed a parent with `belongs_to`.** `column` is this table's FK and defaults to the field name.

   ```yaml
   - belongs_to: createdBy
     column: created_by
     table: users
     primary_key: id
     fields:
       - keyword: email
         required: true
   ```

3. **Embed children with `has_many`.** `foreign_key` is the related table's column pointing back. Order, cap, and filter the rows; the projection is its own `fields` list and may nest further.

   ```yaml
   - has_many: orders
     table: orders
     foreign_key: user_id
     primary_key: id
     filters:
       - { column: status, op: neq, value: cancelled }
     order_by:
       - { column: placed_at, direction: desc }
     limit: 20
     fields:
       - decimal: total
         required: true
       - has_many: items
         table: order_items
         foreign_key: order_id
         primary_key: id
         fields:
           - integer: quantity
             required: true
   ```

4. **Cross a junction with `many_to_many`.**

   ```yaml
   - many_to_many: tags
     table: tags
     through: { table: post_tags, left_key: post_id, right_key: tag_id }
     primary_key: id
     fields:
       - keyword: name
         required: true
   ```

5. **Roll up instead of embedding.** The op is the type key. A `count` is a non-null `long`; `sum`/`min`/`max` mirror their column and need a `value_type`; `ids` collects primary keys into a flat array.

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
   ```

6. **Check publication coverage.** Every table a join or aggregate reads must be in the publication, or its changes never reach flusso.

   ```sh
   flusso check --config flusso.toml
   ```

   The coverage section lists any table not yet streamed. When the source role owns the tables and holds `CREATE` on the database, `flusso run` extends the publication itself. Otherwise `check` prints the exact `ALTER PUBLICATION … ADD TABLE` to run with a privileged role. Details in [Source: Postgres](../reference/source-postgres.md#capture).

## Options and variations

- **A non-null object.** `required: true` on a `belongs_to`/`has_one` maps the object non-null. Only do this when the FK column is `NOT NULL`.
- **`has_one` with several candidates.** `order_by` picks which row becomes the object.
- **Key arity is enforced.** A `has_many` with a `column`, or a `belongs_to` with a `foreign_key`, is a load-time error naming the right sibling.
- **Deep nesting is fine.** The dev stack's `users` schema goes three levels: user, orders, items.
- **What rebuilds.** A changed related row rebuilds every parent document through the join's key. A deleted `belongs_to` target rebuilds its referrers with a null object. See [Joins](../reference/joins.md#what-a-related-change-rebuilds).

## Related

- [Joins](../reference/joins.md) and [Aggregates](../reference/aggregates.md) for every sibling.
- [Nested collections](../query/nested.md) for querying the arrays this produces.

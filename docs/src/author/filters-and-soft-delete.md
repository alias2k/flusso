# Filter rows and soft-delete

Index a subset of a table, turn a "deleted" flag into a tombstone, and narrow which related rows a join sees.

## When to use this

Only some rows of the root table belong in the index; rows are retired by a flag or timestamp rather than a `DELETE`; or a join should embed only certain children. All three use the same filter grammar, in [Filters and soft_delete](../reference/filters-and-soft-delete.md).

## Steps

1. **Scope the root with `filters`.** Only rows matching every filter become documents.

   ```yaml
   version: 1
   table: item
   primary_key: id
   filters:
     - { column: item_type, op: eq, value: serialized }
     - { column: archived_at, op: is_null }
   fields:
     - keyword: sku
       required: true
   ```

   A row that later stops matching emits a tombstone; a row that starts matching upserts. Backfill applies the same predicate.

2. **Turn a flag into a tombstone with `soft_delete`.** Key it off a column, or off a mapped field with a `when` narrowing.

   ```yaml
   soft_delete:
     column: deleted
   ```

   ```yaml
   soft_delete:
     field: status
     when:
       - { column: archived, op: eq, value: true }
   ```

   When the marker is set, the rebuild emits a `delete` instead of an upsert. Clear it and the next rebuild restores the document.

3. **Narrow a join.** Filters on a join or aggregate restrict which related rows fold in or count.

   ```yaml
   - has_many: orders
     table: orders
     foreign_key: user_id
     primary_key: id
     filters:
       - { column: status, op: not_in, value: [cancelled, refunded] }
     fields:
       - decimal: total
         required: true
   ```

4. **Reach for raw SQL when the structured forms can't say it.**

   ```yaml
   filters:
     - { raw: "amount > 0 AND currency = 'USD'" }
   ```

5. **Verify with a live change.** With `flusso run` going and a stdout sink configured, flip a row's marker and watch the envelope: `"op": "delete"` for the tombstone, `"op": "upsert"` when it's cleared. Or read the index: the document is gone after the tombstone.

## Options and variations

- **Hard deletes need no configuration.** A WAL `DELETE` on a table with a primary key already produces a tombstone. What the table needs is row identity; see [Source: Postgres](../reference/source-postgres.md#server-requirements).
- **The marker is read from the current row**, so a boolean flag and a nullable `deleted_at` both work (`{ column: deleted_at, op: is_not_null }` in a root filter, or a `soft_delete` on the column).
- **Filters are `AND`ed.** For `OR`, use `in`, or a raw filter.
- **A filtered-out row during backfill** costs a no-op delete, nothing more.

## Related

- [Filters and soft_delete](../reference/filters-and-soft-delete.md) for every operator and value shape.
- [Recover from a dropped slot](../operate/dropped-slot.md) for how tombstones and rebuilds interact when the stream is lost.

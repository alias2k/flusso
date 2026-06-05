# Local dev environment

A real, running flusso: Postgres set up for logical replication and OpenSearch,
pre-seeded with data and the publication flusso consumes, plus a config + schema
so you can watch documents stream into OpenSearch (and to stdout for inspection)
as you change rows.

## Layout

A small e-commerce store (users, profiles, addresses, categories, products,
tags, orders, items, reviews) feeding **three** indexes, between them exercising
every feature: all scalar types (incl. a `custom` → `scaled_float`), groups,
one-to-one / one-to-many / many-to-many joins, three levels of nesting, every
aggregate, filters, and soft-delete.

```
docker-compose.yml          Postgres + OpenSearch
dev/
  flusso.toml               source + sinks (opensearch + stdout) + 3 indexes
  users.schema.yml          user + account group + profile + addresses +
                            orders→items + order rollups
  products.schema.yml       product + pricing group (custom scaled_float) +
                            tags (m:n) + reviews + rating rollups
  orders.schema.yml         order + timeline group + line items + rollups
  changes.sql               sample INSERT/UPDATE/DELETE to watch live
  postgres/init/
    01_schema.sql           the 10-table store schema
    02_seed.sql             initial fixtures
    03_replication.sql      publication `flusso` over every table
```

## Run it

1. **Start the stack** (init scripts run once on the fresh volume):

   ```sh
   docker compose up -d
   docker compose ps          # wait for both services to be "healthy"
   ```

2. **Start flusso** — it creates the replication slot if needed, backfills
   OpenSearch, then follows live changes. Logs go to stderr; the stdout sink
   also prints each document to the terminal:

   ```sh
   cargo run -- check --config dev/flusso.toml   # optional: validate first
   cargo run -- run --config dev/flusso.toml
   ```

3. **Make changes** in another terminal and watch them appear:

   ```sh
   psql "postgres://postgres:postgres@127.0.0.1:5432/flusso" -f dev/changes.sql
   ```

   The changes touch all three indexes: a new user, a profile filling in,
   a new order rebuilding both its `orders` doc and the owner's `users` doc, a
   line-item edit reverse-resolving to both, a cancelled order dropping out of
   the user's (filtered) orders array, a new review updating a `products` doc's
   rollups, a re-tag, a reprice, and finally a soft-delete turning a user into a
   `delete`. All are visible in OpenSearch immediately (the sink forces a
   refresh after every flush). Inspect each index:

   ```sh
   curl -s localhost:9200/users/_search?pretty
   curl -s localhost:9200/products/_search?pretty
   curl -s localhost:9200/orders/_search?pretty
   ```

## Notes

- flusso **creates the replication slot automatically** on first connect if it
  does not exist. The publication still needs to exist (created by
  `03_replication.sql` on first boot) because it determines which tables are
  included — that is a schema decision, not a runtime one.
- Only changes made *while flusso is connected* (or buffered in the slot since
  it was created) are captured as live events. The initial backfill handles
  everything that existed before.
- Reset everything (wipes data + slot) with:

  ```sh
  docker compose down -v && docker compose up -d
  ```

- Inspect the slot / publication directly:

  ```sh
  psql "postgres://postgres:postgres@127.0.0.1:5432/flusso" \
    -c "SELECT slot_name, plugin, confirmed_flush_lsn FROM pg_replication_slots;" \
    -c "SELECT pubname, tablename FROM pg_publication_tables;"
  ```

- Query OpenSearch directly:

  ```sh
  curl -s http://localhost:9200/users/_search?pretty
  ```

- **OpenSearch Dashboards** is available at http://localhost:5601 once the
  stack is healthy. Use it to explore indices, run Dev Tools queries, and
  inspect mappings.

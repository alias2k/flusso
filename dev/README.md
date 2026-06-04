# Local dev environment

A real, running flusso: Postgres set up for logical replication and OpenSearch,
pre-seeded with data and the publication flusso consumes, plus a config + schema
so you can watch documents stream into OpenSearch (and to stdout for inspection)
as you change rows.

## Layout

```
docker-compose.yml          Postgres + OpenSearch
dev/
  config.toml               source + sinks (opensearch + stdout) + indexes
  users.schema.yml          the `users` document: user + nested orders
  changes.sql               sample INSERT/UPDATE/DELETE to watch live
  postgres/init/
    01_schema.sql           users + orders tables
    02_seed.sql             initial fixtures
    03_replication.sql      publication `flusso` (slot is created automatically)
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
   cargo run -- check --config dev/config.toml   # optional: validate first
   cargo run -- run --config dev/config.toml
   ```

3. **Make changes** in another terminal and watch them appear:

   ```sh
   psql "postgres://postgres:postgres@127.0.0.1:5432/flusso" -f dev/changes.sql
   ```

   You should see: a new user 4, then user 4 rebuilt with an order, user 1
   re-emitted after its order changes, user 2 with a new email, and user 3
   turning into a `delete` (soft-deleted). All changes are also visible in
   OpenSearch immediately (the sink forces a refresh after every flush).

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

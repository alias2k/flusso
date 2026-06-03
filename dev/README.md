# Local dev environment

A real, running storno: a Postgres set up for logical replication, pre-seeded
with data and the slot/publication storno consumes, plus a config + schema so
you can watch documents stream out as you change rows.

The sink is **stdout** for now (OpenSearch comes later — see the commented
service in `docker-compose.yml` and the sink note in `config.toml`).

## Layout

```
docker-compose.yml          Postgres with wal_level=logical
dev/
  config.toml               source + sink + which indexes to build
  users.schema.yml          the `users` document: user + nested orders
  changes.sql               sample INSERT/UPDATE/DELETE to watch live
  postgres/init/
    01_schema.sql           users + orders tables
    02_seed.sql             initial fixtures
    03_replication.sql      publication `storno` + slot `storno`
```

## Run it

1. **Start Postgres** (init scripts run once on the fresh volume):

   ```sh
   docker compose up -d
   docker compose ps          # wait for "healthy"
   ```

2. **Start storno** — it connects, attaches to the slot, and prints rebuilt
   documents to stdout (logs go to stderr):

   ```sh
   cargo run -p cli -- --config dev/config.toml --pretty
   ```

3. **Make changes** in another terminal and watch them appear:

   ```sh
   psql "postgres://postgres:postgres@127.0.0.1:5432/storno" -f dev/changes.sql
   ```

   You should see, in storno's output: a new user 4, then user 4 rebuilt with
   an order, user 1 re-emitted after its order changes, user 2 with a new email,
   and user 3 turning into a `delete` (soft-deleted).

## Notes

- storno is a **pure consumer**: it does not create the slot or publication, and
  it does not replay rows that existed before the slot. Only changes made *while
  it is connected* (or buffered in the slot since it was created) are emitted.
  That's why `changes.sql` is where the action is, not the seed data.
- Reset everything (wipes data + slot) with:

  ```sh
  docker compose down -v && docker compose up -d
  ```

- Inspect the slot / publication directly:

  ```sh
  psql "postgres://postgres:postgres@127.0.0.1:5432/storno" \
    -c "SELECT slot_name, plugin, confirmed_flush_lsn FROM pg_replication_slots;" \
    -c "SELECT pubname, tablename FROM pg_publication_tables;"
  ```

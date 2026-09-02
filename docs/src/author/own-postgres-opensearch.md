# Your own Postgres and OpenSearch

Prepare a Postgres and an OpenSearch you operate so flusso can stream from one into the other, and verify each prerequisite before the first run.

## When to use this

You've run the [Quickstart](../start/quickstart.md) against the dev stack and now want flusso as a guest in your infrastructure. The requirement tables live in [Source: Postgres](../reference/source-postgres.md#server-requirements) and [Sink: OpenSearch](../reference/sink-opensearch.md#cluster-requirements); these steps verify them in order.

## Steps

1. **Turn on logical replication.** Postgres 14 or newer, with `wal_level = logical`. This needs a restart. Leave room in `max_wal_senders` and `max_replication_slots` for flusso plus any other consumer.

   ```sql
   SHOW wal_level;                 -- logical
   SHOW max_replication_slots;     -- at least 1 free
   ```

2. **Create the role.** `REPLICATION` to stream and create the slot, `SELECT` on every table any index reads.

   ```sql
   CREATE ROLE flusso WITH LOGIN REPLICATION PASSWORD '…';
   GRANT SELECT ON ALL TABLES IN SCHEMA public TO flusso;
   ```

   To let flusso manage the publication too, the role must own those tables and hold `CREATE` on the database. Otherwise flusso prints the SQL and you run it as a privileged role.

3. **Confirm row identity.** Every replicated table needs a single-column primary key or an explicit `REPLICA IDENTITY`. A keyless table is skipped in backfill and errors on a live change.

   ```sql
   SELECT relname FROM pg_class c
   WHERE relkind = 'r' AND relnamespace = 'public'::regnamespace
     AND NOT EXISTS (SELECT 1 FROM pg_index i WHERE i.indrelid = c.oid AND i.indisprimary);
   ```

   An empty result means every table has a primary key.

4. **Create the publication, or let flusso.** With a privileged role, `flusso run` creates and extends it. To do it yourself, take the table list from `flusso check`'s coverage report:

   ```sql
   CREATE PUBLICATION flusso FOR TABLE users, orders, order_items;
   ```

5. **Prepare the OpenSearch user.** OpenSearch 2.x, reachable over HTTP(S), with a user that can create, alias, write, and delete flusso's indexes and the `flusso_meta` index. Verify the endpoint and credentials:

   ```sh
   curl -u flusso:"$OS_PASSWORD" https://search.internal:9200/
   ```

   With `text_analysis = "icu"`, confirm the plugin: `curl …/_cat/plugins | grep analysis-icu`.

6. **Run `check` against both.**

   ```sh
   DATABASE_URL=… PRIMARY_OPENSEARCH_URL=… flusso check --config flusso.toml
   ```

   A passing check means the files load, every declared type matches a live column, and the publication covers every table.

## Options and variations

- **TLS to Postgres.** A managed provider's `DATABASE_URL=…?sslmode=require` works as pasted, but `require` doesn't verify the server. Set `ssl_mode = "verify-full"` and `ssl_root_cert` for production. See [TLS](../reference/source-postgres.md#tls).
- **Self-signed OpenSearch.** `tls_verify = false` on the sink, for development only.
- **WAL retention.** Postgres keeps WAL until the slot confirms it. A flusso that stays down for days grows the WAL. Drop the slot when retiring a deployment: `SELECT pg_drop_replication_slot('flusso');`.
- **Slot and publication names** default to `flusso`; `--slot` and `--publication` change them.

## Related

- [Source: Postgres](../reference/source-postgres.md) and [Sink: OpenSearch](../reference/sink-opensearch.md) for every option.
- [Recover from a dropped slot](../operate/dropped-slot.md) for what happens when the slot goes missing later.

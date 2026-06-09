# Local dev environment

A real, running flusso: Postgres set up for logical replication and OpenSearch,
pre-seeded with data and the publication flusso consumes, plus a config + schema
so you can watch documents stream into OpenSearch (and to stdout for inspection)
as you change rows.

## Two ways to run

- **One-command demo** — everything in containers, *including flusso itself*. No
  Rust toolchain needed; good for a quick look. See [Quick demo](#quick-demo).
- **Dev stack** — Postgres/OpenSearch (+ Prometheus/Grafana) in Docker, with
  flusso run on the host via `cargo run`. Better for iterating. See [Run it](#run-it).

## Quick demo

Brings up Postgres, OpenSearch (+ Dashboards), Prometheus, Grafana, **and flusso
itself** — flusso is built from the repo and runs in the cluster. It's the base
stack with a demo override layered on (the Docker way), so the demo file just
adds the `flusso` service:

```sh
docker compose -f docker-compose.yml -f docker-compose.demo.yml up --build   # first build compiles flusso (~a few min)
```

Once it's up:

- flusso status / metrics — http://localhost:9464/status , http://localhost:9464/metrics
- **Grafana** (flusso dashboard, opens straight in) — http://localhost:3000
- OpenSearch — `curl -s localhost:9200/users/_search?pretty`
- drive live changes:

  ```sh
  psql "postgres://postgres:postgres@127.0.0.1:5432/flusso" -f dev/changes.sql
  ```

flusso reads its baked-in `flusso.lock`; the connection and sink URLs are pointed
at the in-cluster services via `DATABASE_URL` / `PRIMARY_OPENSEARCH_URL` in the
override. Since it's the same Compose project, it shares the base stack's network
and volumes — just don't *also* run a host `cargo run -- run` at the same time
(both would consume the same replication slot). Tear down with
`docker compose -f docker-compose.yml -f docker-compose.demo.yml down -v`.

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
  load.sql                  simulate_production() — continuous read→modify→write load
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
   cargo run -- run --config dev/flusso.toml --http-addr 127.0.0.1:9464
   ```

   `--http-addr` serves the operational surface: `/healthz`, `/readyz`,
   `/status` (live JSON — phase, per-index seeded state, counters, slot lag),
   and `/metrics` (Prometheus). Use port `9464` so the bundled Prometheus
   scrapes it (see [Observability](#observability)).

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

## Observability

The stack includes Prometheus and Grafana, both wired to flusso's metrics.

- Run flusso with `--http-addr 127.0.0.1:9464` so it exposes `/metrics`.
  **Prometheus** (http://localhost:9090) scrapes it via `host.docker.internal`.
- **Grafana** (http://localhost:3000, opens straight in — anonymous admin) comes
  pre-provisioned with a *flusso* dashboard: change throughput, in-flight
  changes (back-pressure), replication slot lag, flush-duration p95, documents
  built, and errors.
- The same metrics export over **OTLP** when an endpoint is configured, e.g.
  `OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4318 cargo run -- run …` — the
  same env vars that already drive trace export.
- Peek at the raw numbers without Grafana:

  ```sh
  curl -s localhost:9464/status | jq        # live pipeline state
  curl -s localhost:9464/metrics            # Prometheus exposition
  ```

### Generate load

To actually *see* throughput, in-flight backlog, slot lag, and flush latency
move, drive sustained traffic with `load.sql` — it defines `simulate_production()`,
a read→modify→write loop (user/product edits, line-item changes that
reverse-resolve into orders + users, new orders, reviews, soft-deletes) that
commits each tick so changes stream out as it runs:

```sh
# host stack (flusso via cargo run) or demo (containerized) — same DB:
psql "postgres://postgres:postgres@127.0.0.1:5432/flusso" -f dev/load.sql
psql "postgres://postgres:postgres@127.0.0.1:5432/flusso" \
  -c "CALL simulate_production(duration_secs => 300, ops_per_tick => 25, sleep_ms => 150)"
```

Tune the rate with `ops_per_tick` / `sleep_ms` (more ops, less sleep = higher
throughput; `sleep_ms => 0` with a big `ops_per_tick` is a burst/stress run).
Watch `flusso_changes_in_flight` climb when the source outruns the sink — that's
back-pressure — and `flusso_replication_slot_lag_bytes` track how far behind
Postgres flusso is.

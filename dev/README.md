# Local dev environment

A real, running flusso to poke at — Postgres wired for logical replication, OpenSearch,
seeded data, the publication, a config + schemas. `just up && just run`, then change a row
and watch the document stream into OpenSearch (and to stdout).

It's the [top-level quickstart](../README.md#quickstart) with the lid off. This doc is for
when you want to know what each piece is doing.

## Recipe cheat-sheet

The everyday loop, wrapped in the [`justfile`](../justfile) (`cargo install just --locked`).
Each recipe prints its raw command; override any variable inline, e.g. `just config=other.toml check`.

| Recipe | What it does |
| --- | --- |
| `just up` / `just ps` / `just down` | Start the Docker stack (waits for healthy) / status / stop. |
| `just reset` | Wipe volumes and re-seed from scratch. |
| `just check` | Validate config + schemas against the DB. |
| `just run` | Backfill + follow; serves `/status` + `/metrics` on `:9464`. |
| `just run-live` | `run` but `--skip-backfill` (resume live only). |
| `just demo` | Everything in containers, flusso included — no host toolchain. |
| `just psql` | Interactive psql on the dev DB. |
| `just api` / `just dev` | Serve the search API / engine + API together. |
| `just status` / `just metrics` / `just eta` | Live state / raw metrics / backlog drain ETA. |
| `just grafana` | Open the Grafana dashboard. |

## Contents

- [Two ways to run](#two-ways-to-run) — host stack vs. one-command demo
- [Run it](#run-it) — the everyday loop (`just up` / `just run` / `just psql`)
- [Quick demo](#quick-demo) — everything in containers, flusso included
- [Layout](#layout) — what's in `dev/` and what it exercises
- [Notes](#notes) — slots, publications, resetting, poking around
- [Observability](#observability) — Prometheus, Grafana, OTLP, generating load

## Two ways to run

- **Dev stack** — Postgres/OpenSearch (+ Prometheus/Grafana) in Docker, with
  flusso itself on the host via `cargo run` (or the `just` recipes that wrap it).
  Faster iteration; this is the one you'll live in. See [Run it](#run-it).
- **One-command demo** — *everything* in containers, flusso included. No Rust
  toolchain needed, good for a quick look or showing someone. See
  [Quick demo](#quick-demo).

## Run it

The shortest path uses the `just` recipes above. Each is a thin wrapper — the raw
`docker compose` / `cargo run` command is right beside it in the file if you'd rather
type it out.

1. **Start the stack.** Init scripts run once on the fresh volume, so the first
   `up` seeds Postgres and waits for everything to report healthy:

   ```sh
   just up                    # docker compose up -d --wait
   just ps                    # docker compose ps — sanity check
   ```

2. **Start flusso.** It creates the replication slot if needed, backfills
   OpenSearch, then follows live changes. Logs go to stderr; the stdout sink also
   prints each document to the terminal so you can watch documents take shape:

   ```sh
   just check                 # optional: validate config + schemas first
   just run                   # backfill + follow
   ```

   `just run` passes `--public-address 127.0.0.1:9464`, which serves the public
   surface: `/healthz`, `/readyz`, `/status` (live JSON — phase, per-index seeded
   state, counters, slot lag), and `/metrics` (Prometheus). Port `9464` is the one
   the bundled Prometheus expects to scrape (see [Observability](#observability)).

   The raw commands behind these are `cargo run -- check --config dev/flusso.toml`
   and `cargo run -- run --config dev/flusso.toml --public-address 127.0.0.1:9464`.
   With `--config` given, `run` recompiles and rewrites `flusso.lock` on start
   (cargo-style), so an edited schema lands in the committable lock with no separate
   `build` step. Pass `--locked` to run the existing lock untouched.

3. **Make changes** in another terminal and watch them appear. `just psql` opens a
   shell on the dev database; to replay the curated sample set, feed it
   `changes.sql`:

   ```sh
   just psql                  # interactive psql on the dev DB
   psql "postgres://postgres:postgres@127.0.0.1:5432/flusso" -f dev/changes.sql
   ```

   Those changes touch all three indexes: a new user, a profile filling in, a new
   order rebuilding both its `orders` doc and the owner's `users` doc, a line-item
   edit reverse-resolving to both, a cancelled order dropping out of the user's
   (filtered) orders array, a new review updating a `products` doc's rollups, a
   re-tag, a reprice, and finally a soft-delete turning a user into a `delete`.
   They all land in OpenSearch immediately — the sink forces a refresh after every
   flush, so there's no waiting around. Inspect each index:

   ```sh
   curl -s localhost:9200/users/_search?pretty
   curl -s localhost:9200/products/_search?pretty
   curl -s localhost:9200/orders/_search?pretty
   ```

## Quick demo

Want the whole thing — Postgres, OpenSearch (+ Dashboards), Prometheus, Grafana,
**and flusso itself** — without a Rust toolchain on your machine? `just demo`
builds flusso from the repo and runs it *in* the cluster. It's the base stack with
a demo override layered on (the Docker way), so the demo file just adds the
`flusso` service:

```sh
just demo
# raw: docker compose -f docker-compose.yml -f docker-compose.demo.yml up --build
```

The first build compiles flusso, so expect a few minutes before anything starts.
Once it's up:

- flusso status / metrics — http://localhost:9464/status , http://localhost:9464/metrics
- **Grafana** (flusso dashboard, opens straight in) — http://localhost:3000
- OpenSearch — `curl -s localhost:9200/users/_search?pretty`
- drive live changes:

  ```sh
  psql "postgres://postgres:postgres@127.0.0.1:5432/flusso" -f dev/changes.sql
  ```

In demo mode flusso reads its baked-in `flusso.lock`, and its connection + sink
URLs are pointed at the in-cluster services via `DATABASE_URL` /
`PRIMARY_OPENSEARCH_URL` set in the override (the full env-var story lives in
[Configuring a deployment](https://alias2k.github.io/flusso/guides/configuration.html)).
It's the same Compose project as the base stack,
so it shares the same network and volumes — which is exactly why you must **not**
*also* run a host `cargo run -- run` at the same time: two flussos fighting over
one replication slot ends about as well as it sounds. Tear it down with:

```sh
docker compose -f docker-compose.yml -f docker-compose.demo.yml down -v
```

## Layout

A small e-commerce store (users, profiles, addresses, categories, products, tags,
orders, items, reviews) feeding **three** indexes that, between them, exercise
every feature: all scalar types (incl. a `custom` → `scaled_float`), objects,
one-to-one / one-to-many / many-to-many joins, three levels of nesting, every
aggregate, filters, and soft-delete. If a feature exists, something in here pokes
it.

```
docker-compose.yml          Postgres + OpenSearch
dev/
  flusso.toml               source + sinks (opensearch + stdout) + 3 indexes
  users.schema.yml          user + account object + profile + addresses +
                            orders→items + order rollups
  products.schema.yml       product + pricing object (custom scaled_float) +
                            tags (m:n) + reviews + rating rollups
  orders.schema.yml         order + timeline object + line items + rollups
  changes.sql               sample INSERT/UPDATE/DELETE to watch live
  load.sql                  simulate_production() — continuous read→modify→write load
  postgres/init/
    01_schema.sql           the 10-table store schema
    02_seed.sql             initial fixtures
    03_replication.sql      publication `flusso` over every table
```

The schema keys themselves are documented in
[Authoring schemas](https://alias2k.github.io/flusso/guides/schema-authoring.html), and
the source/sink options in
[Configuring a deployment](https://alias2k.github.io/flusso/guides/configuration.html).

## Notes

- flusso **creates the replication slot automatically** on first connect if it
  doesn't exist. The publication still has to exist already (created by
  `03_replication.sql` on first boot) because it decides which tables are
  included — that's a schema decision, not a runtime one.
- Only changes made *while flusso is connected* (or buffered in the slot since it
  was created) are captured as live events. The initial backfill handles
  everything that existed before.
- Reset everything (wipes data + slot, re-seeds from scratch) with:

  ```sh
  just reset
  # raw: docker compose down -v && docker compose up -d
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

- **OpenSearch Dashboards** is at http://localhost:5601 once the stack is healthy.
  Use it to browse indices, run Dev Tools queries, and inspect mappings without
  hand-writing `curl` every time.

## Observability

The stack ships Prometheus and Grafana, both wired to flusso's metrics.

- Run flusso with `--public-address 127.0.0.1:9464` (which `just run` does for you)
  so it exposes `/metrics`. **Prometheus** (http://localhost:9090) scrapes it via
  `host.docker.internal`.
- **Grafana** (http://localhost:3000, opens straight in — anonymous admin, no
  login dance) comes pre-provisioned with a *flusso* dashboard: change throughput,
  in-flight changes (back-pressure), replication slot lag, flush-duration p95,
  documents built, errors, and a **backlog drain ETA** (how long until the backlog
  clears at the current rate) plus the slot-lag trend. `just grafana` opens it.
- The ETA comes from **Prometheus recording rules** (`dev/prometheus/rules/`):
  `flusso:slot_lag_bytes_rate5m` is the net drain rate (bytes/s; `< 0` = catching
  up) and `flusso:backlog_drain_eta_seconds` is `lag ÷ drain_rate`, present only
  while actually draining (a flat or growing backlog has no finite ETA). Both are
  queryable directly and alertable.
- The same metrics export over **OTLP** when an endpoint is configured, e.g.
  `OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4318 cargo run -- run …` — the
  same env vars that already drive trace export. See
  [Configuring a deployment](https://alias2k.github.io/flusso/guides/configuration.html#logging--telemetry)
  for the full telemetry story.
- Peek at the raw numbers without Grafana:

  ```sh
  just status                               # live pipeline state (curl /status | json)
  just metrics                              # raw Prometheus exposition
  just eta                                  # backlog drain ETA from the recording rule
  # raw:
  curl -s localhost:9464/status | jq        # live pipeline state
  curl -s localhost:9464/metrics            # Prometheus exposition
  ```

### Generate load

Numbers are more fun when they move. To actually *see* throughput, in-flight
backlog, slot lag, and flush latency do something, drive sustained traffic with
`load.sql` — it defines `simulate_production()`, a read→modify→write loop
(user/product edits, line-item changes that reverse-resolve into orders + users,
new orders, reviews, soft-deletes) that commits each tick so changes stream out as
it runs:

```sh
# host stack (flusso via cargo run) or demo (containerized) — same DB:
psql "postgres://postgres:postgres@127.0.0.1:5432/flusso" -f dev/load.sql
psql "postgres://postgres:postgres@127.0.0.1:5432/flusso" \
  -c "CALL simulate_production(duration_secs => 300, ops_per_tick => 25, sleep_ms => 150)"
```

Tune the rate with `ops_per_tick` / `sleep_ms` (more ops, less sleep = higher
throughput; `sleep_ms => 0` with a big `ops_per_tick` is a burst/stress run — the
"let's see what breaks" setting). Watch `flusso_changes_in_flight` climb when the
source outruns the sink — that's back-pressure — and
`flusso_replication_slot_lag_bytes` track how far behind Postgres flusso is.

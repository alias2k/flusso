# Local dev environment

A real, running flusso to poke at: Postgres wired for logical replication, OpenSearch, seeded data, the publication, and a config with three indexes. `just up && just run`, then change a row and watch the document stream out.

The guided walkthrough is the manual's [Quickstart](https://alias2k.github.io/flusso/start/quickstart.html); this file is the map of what's in here.

## Recipes

Wrapped in the [`justfile`](../justfile) (`cargo install just --locked`). Each prints its raw command; override a variable inline, `just config=other.toml check`.

| Recipe | Does |
| --- | --- |
| `just up` / `just ps` / `just down` | start the Docker stack and wait for healthy / status / stop |
| `just reset` / `just reset db` | wipe volumes and re-seed / re-seed only Postgres |
| `just check` / `just check-offline` | validate config + schemas, with / without the database |
| `just run` / `just run-live` | backfill + follow, serving `/status` and `/metrics` on `:9464` / skip the backfill |
| `just design` / `just design-dev` | open the visual designer / with the Vite dev server for hot reload |
| `just demo` | everything in containers, flusso included; no host toolchain |
| `just psql` | psql on the dev database |
| `just api` / `just dev` | the example search API / engine + API together |
| `just status` / `just metrics` / `just eta` | live status / raw Prometheus exposition / backlog drain ETA |
| `just grafana` | open the dashboard |
| `just bench` / `just bench-components` / `just bench-scenario` | the benchmarks: in-process, Docker-backed components, a scenario on the real binary ([Benchmarks](https://alias2k.github.io/flusso/contribute/benchmarks.html)) |
| `just load` | soak: production-like load for N users that you watch; not a benchmark |
| `just test` / `just test-all` / `just doc` / `just lint` / `just ci` | the quality gate, mirroring CI |

## Two ways to run

- **Dev stack.** Postgres, OpenSearch, Dashboards, Prometheus, and Grafana in Docker; flusso on the host via `cargo run`. Faster iteration.
- **Demo.** `just demo` layers `docker-compose.demo.yml` on the base stack to add a `flusso` service built from the `Dockerfile`'s `demo` target (release binary plus the dev config baked into `/app/flusso.lock`). Same Compose project, so it shares network and volumes. Don't run a host flusso at the same time; both would consume the same replication slot. Tear down with `docker compose -f docker-compose.yml -f docker-compose.demo.yml down -v`.

## Layout

A small store (users, profiles, addresses, categories, products, tags, orders, items, reviews; the three root tables carry `updated_at`, the benchmark's latency marker) feeding three indexes that between them exercise every feature: all scalar types including a `custom` scaled_float, objects, every join verb, three levels of nesting, every aggregate, filters, ordered enums, and soft-delete.

```text
docker-compose.yml          Postgres + OpenSearch + Dashboards + Prometheus + Grafana
docker-compose.demo.yml     adds the flusso service
dev/
  flusso.toml               source + sinks (opensearch + stdout) + 3 indexes
  users.schema.yml          user + account object + profile + addresses + orders→items + rollups
  products.schema.yml       product + pricing object + tags (m:n) + reviews + rating rollups
  orders.schema.yml         order + timeline object + line items + rollups
  changes.sql               curated INSERT/UPDATE/DELETE set to watch live
  load.sql                  simulate_production(): sustained read→modify→write load
  postgres/init/            01 schema · 02 seed · 03 publication over every table
  prometheus/               scrape config + the backlog-drain recording rules
  grafana/provisioning/     datasource + the flusso dashboard
  search-api/               an axum consumer of flusso-query (`flusso-dev-search-api`, unpublished)
  query-e2e/                the combined-search live e2e (`flusso-query-e2e`, unpublished)
  bench/                    the scenario harness (`flusso-bench`, unpublished): the real binary under load
```

## Useful commands

```sh
psql "postgres://postgres:postgres@127.0.0.1:5432/flusso" -f dev/changes.sql     # replay the change set
psql "postgres://postgres:postgres@127.0.0.1:5432/flusso" -f dev/load.sql \
  -c "CALL simulate_production(duration_secs => 300, ops_per_tick => 25, sleep_ms => 150)"
psql "postgres://postgres:postgres@127.0.0.1:5432/flusso" \
  -c "SELECT slot_name, confirmed_flush_lsn FROM pg_replication_slots;" \
  -c "SELECT pubname, tablename FROM pg_publication_tables;"
curl -s localhost:9200/users/_search?pretty
```

OpenSearch Dashboards is at `localhost:5601`, Prometheus at `:9090`, Grafana at `:3000` (anonymous admin, dashboard pre-provisioned). Prometheus scrapes the host at `host.docker.internal:9464`, which is why `just run` passes `--public-address 127.0.0.1:9464`.

What the pages mean, and how to read them in production, is in the manual: [Watch it run](https://alias2k.github.io/flusso/operate/watch-it-run.html), [Metrics](https://alias2k.github.io/flusso/reference/metrics.html). The schema keys are in the [Reference](https://alias2k.github.io/flusso/reference/schema-top-level.html).
